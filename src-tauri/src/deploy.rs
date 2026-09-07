use crate::commands::{validate_file, ApiError, APK_EXTS};
use byteforge_core::sign::{
    adb_install_args, apksigner_sign_args, keytool_genkey_args, parse_adb_devices,
    signed_output_path, zipalign_args, AdbDevice,
};
use byteforge_core::tools::find_executable;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;
use tauri::ipc::Channel;
use tauri::State;

/// Bir yürütülebiliri PATH'te bulur ya da ToolMissing döndürür.
pub(crate) fn tool(name: &str) -> Result<String, ApiError> {
    find_executable(name).ok_or_else(|| ApiError::ToolMissing {
        message: format!("{name} bulunamadı — PATH'te kurulu olmalı"),
    })
}

/// Bir komutu args-vektörüyle çalıştırır (shell yok → enjeksiyon yok).
pub(crate) fn run(bin: &str, args: &[String]) -> Result<String, ApiError> {
    crate::console::log(format!("$ {} {}", bin, args.join(" ")));
    let output = Command::new(bin).args(args).output().map_err(|e| {
        crate::console::log(format!("! {bin} çalıştırılamadı: {e}"));
        ApiError::ProcessFailed {
            message: format!("{bin} çalıştırılamadı: {e}"),
        }
    })?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    if !stdout.trim().is_empty() {
        crate::console::log(stdout.trim().to_string());
    }
    if !stderr.trim().is_empty() {
        crate::console::log(stderr.trim().to_string());
    }
    if !output.status.success() {
        let msg = if stderr.trim().is_empty() { &stdout } else { &stderr };
        return Err(ApiError::ProcessFailed {
            message: format!("{bin} hata verdi: {}", msg.trim()),
        });
    }
    Ok(stdout.into_owned())
}

/// `~/.config/byteforge/debug.keystore` — yoksa keytool ile bir kez üretir.
fn ensure_debug_keystore() -> Result<PathBuf, ApiError> {
    let home = std::env::var("HOME").map_err(|_| ApiError::ProcessFailed {
        message: "HOME ortam değişkeni tanımlı değil".into(),
    })?;
    let dir = Path::new(&home).join(".config").join("byteforge");
    std::fs::create_dir_all(&dir).map_err(|e| ApiError::Io {
        message: e.to_string(),
    })?;
    let keystore = dir.join("debug.keystore");
    if !keystore.exists() {
        let keytool = tool("keytool")?;
        run(&keytool, &keytool_genkey_args(&keystore))?;
    }
    Ok(keystore)
}

/// Bir APK'yı zipalign'lar ve debug anahtarıyla (v1/v2/v3) yeniden imzalar.
/// İmzalı çıktının yolunu döndürür.
#[tauri::command]
pub fn resign_apk(path: String) -> Result<String, ApiError> {
    let input = validate_file(&path, APK_EXTS)?;
    let keystore = ensure_debug_keystore()?;
    let zipalign = tool("zipalign")?;
    let apksigner = tool("apksigner")?;

    let output = signed_output_path(&input);
    run(&zipalign, &zipalign_args(&input, &output))?;
    run(&apksigner, &apksigner_sign_args(&keystore, &output))?;
    Ok(output.to_string_lossy().into_owned())
}

/// Bağlı ADB cihazlarını listeler.
#[tauri::command]
pub fn list_adb_devices() -> Result<Vec<AdbDevice>, ApiError> {
    let adb = tool("adb")?;
    let out = run(&adb, &["devices".to_string()])?;
    Ok(parse_adb_devices(&out))
}

/// Bir APK'yı cihaza yükler (`adb install -r -d`). `serial` boşsa varsayılan cihaz.
#[tauri::command]
pub fn install_apk(serial: Option<String>, path: String) -> Result<String, ApiError> {
    let apk = validate_file(&path, APK_EXTS)?;
    let adb = tool("adb")?;
    let out = run(&adb, &adb_install_args(serial.as_deref(), &apk))?;
    let combined = if out.trim().is_empty() {
        "Yükleme tamamlandı.".to_string()
    } else {
        out.trim().to_string()
    };
    Ok(combined)
}

/// Çalışan `adb logcat` sürecini tutan uygulama durumu.
#[derive(Default)]
pub struct LogcatState(pub Mutex<Option<Child>>);

/// Varsa mevcut logcat sürecini durdurur.
fn stop_existing(state: &State<LogcatState>) {
    if let Some(mut child) = state.0.lock().unwrap().take() {
        let _ = child.kill();
    }
}

/// Canlı `adb logcat` akışı başlatır; her satırı `on_line` kanalına gönderir.
#[tauri::command]
pub fn start_logcat(
    state: State<LogcatState>,
    serial: Option<String>,
    on_line: Channel<String>,
) -> Result<(), ApiError> {
    stop_existing(&state);
    let adb = tool("adb")?;
    let mut cmd = Command::new(&adb);
    if let Some(s) = &serial {
        cmd.arg("-s").arg(s);
    }
    cmd.arg("logcat").arg("-v").arg("time");
    cmd.stdout(Stdio::piped()).stderr(Stdio::null());

    let mut child = cmd.spawn().map_err(|e| ApiError::ProcessFailed {
        message: format!("adb logcat başlatılamadı: {e}"),
    })?;
    let stdout = child.stdout.take().ok_or_else(|| ApiError::ProcessFailed {
        message: "logcat stdout alınamadı".into(),
    })?;

    std::thread::spawn(move || {
        let reader = BufReader::new(stdout);
        for line in reader.lines().map_while(Result::ok) {
            if on_line.send(line).is_err() {
                break;
            }
        }
    });

    *state.0.lock().unwrap() = Some(child);
    Ok(())
}

/// Canlı logcat akışını durdurur.
#[tauri::command]
pub fn stop_logcat(state: State<LogcatState>) -> Result<(), ApiError> {
    stop_existing(&state);
    Ok(())
}

/// Bir paketi cihazda başlatır (monkey ile LAUNCHER intent'i).
#[tauri::command]
pub fn launch_app(serial: Option<String>, package: String) -> Result<String, ApiError> {
    if package.is_empty()
        || !package
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '_')
    {
        return Err(ApiError::InvalidPath {
            message: "geçersiz paket adı".into(),
        });
    }
    let adb = tool("adb")?;
    let mut args: Vec<String> = Vec::new();
    if let Some(s) = &serial {
        args.push("-s".into());
        args.push(s.clone());
    }
    args.push("shell".into());
    args.push("monkey".into());
    args.push("-p".into());
    args.push(package);
    args.push("-c".into());
    args.push("android.intent.category.LAUNCHER".into());
    args.push("1".into());
    run(&adb, &args)?;
    Ok("Cihazda başlatıldı.".into())
}

/// Mod-farkındalıklı imzalama sonucu.
#[derive(serde::Serialize)]
pub struct ResignResult {
    pub signed_path: String,
    /// Cache'teki yamalı .so'lar APK'ya dahil edildi mi?
    pub included_mods: bool,
    /// Dahil edilen yamalı native giriş sayısı.
    pub mod_count: usize,
    pub note: String,
}

/// Bir APK'yı imzalar — AMA önce, bu APK için Atölye'de yamalanmış (cache'te
/// `.bak`'li) libil2cpp.so'lar varsa onları APK'ya geri paketler. Böylece
/// "Profil'de yaptığın yamalar" Dağıt'tan da telefona gider.
#[tauri::command]
pub fn resign_apk_with_mods(path: String) -> Result<ResignResult, ApiError> {
    let input = validate_file(&path, APK_EXTS)?;

    // Bu APK'nın IL2CPP cache dizini (extract_il2cpp ile aynı stem mantığı).
    let stem = input
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let home = std::env::var("HOME").map_err(|_| ApiError::ProcessFailed {
        message: "HOME tanımlı değil".into(),
    })?;
    let cache = PathBuf::from(&home)
        .join(".cache/byteforge/il2cpp")
        .join(&stem);

    // Yamalı .so'ları topla: libil2cpp-<abi>.so + kardeş .bak varsa yamalanmış.
    let mut replace: std::collections::HashMap<String, Vec<u8>> = std::collections::HashMap::new();
    if let Ok(rd) = std::fs::read_dir(&cache) {
        for e in rd.flatten() {
            let p = e.path();
            let fname = p.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
            // libil2cpp-arm64-v8a.so → abi=arm64-v8a
            let Some(abi) = fname
                .strip_prefix("libil2cpp-")
                .and_then(|r| r.strip_suffix(".so"))
            else {
                continue;
            };
            let bak = p.with_extension("so.bak");
            if !bak.exists() {
                continue; // hiç yama uygulanmamış
            }
            if let Ok(data) = std::fs::read(&p) {
                replace.insert(format!("lib/{abi}/libil2cpp.so"), data);
            }
        }
    }

    let keystore = ensure_debug_keystore()?;
    let zipalign = tool("zipalign")?;
    let apksigner = tool("apksigner")?;

    // Kaynak: modlar varsa modlu geçici APK, yoksa orijinal.
    let source = if replace.is_empty() {
        input.clone()
    } else {
        let modded = input.with_file_name(format!(
            "{}-modlu.apk",
            input.file_stem().unwrap_or_default().to_string_lossy()
        ));
        crate::commands::repack_replacing(&input, &replace, &modded)?;
        modded
    };

    let output = signed_output_path(&source);
    run(&zipalign, &zipalign_args(&source, &output))?;
    run(&apksigner, &apksigner_sign_args(&keystore, &output))?;

    let mod_count = replace.len();
    let note = if mod_count > 0 {
        format!("{mod_count} yamalı native kütüphane APK'ya dahil edildi ve imzalandı.")
    } else {
        "Bu APK için yamalı .so bulunamadı — orijinal imzalandı.".into()
    };
    Ok(ResignResult {
        signed_path: output.to_string_lossy().into_owned(),
        included_mods: mod_count > 0,
        mod_count,
        note,
    })
}
