use crate::commands::{validate_file, ApiError, APK_EXTS};
use crate::deploy::{run, tool};
use serde::Serialize;
use std::path::{Path, PathBuf};

const MAX_RESULTS: usize = 300;

/// jadx Java kaynak çıktısı kökü.
fn jadx_root() -> Result<PathBuf, ApiError> {
    let home = std::env::var("HOME").map_err(|_| ApiError::ProcessFailed {
        message: "HOME tanımlı değil".into(),
    })?;
    Ok(Path::new(&home).join(".cache").join("byteforge").join("jadx"))
}

/// out_dir'in jadx kökü altında olduğunu doğrular.
fn safe_jadx_dir(out_dir: &str) -> Result<PathBuf, ApiError> {
    let root = jadx_root()?
        .canonicalize()
        .map_err(|e| ApiError::InvalidPath {
            message: format!("jadx kökü yok: {e}"),
        })?;
    let dir = PathBuf::from(out_dir)
        .canonicalize()
        .map_err(|e| ApiError::InvalidPath {
            message: format!("dizin çözümlenemedi: {e}"),
        })?;
    if !dir.starts_with(&root) {
        return Err(ApiError::InvalidPath {
            message: "dizin jadx alanının dışında".into(),
        });
    }
    Ok(dir)
}

fn safe_join(out_dir: &str, rel: &str) -> Result<PathBuf, ApiError> {
    let base = safe_jadx_dir(out_dir)?;
    let joined = base
        .join(rel)
        .canonicalize()
        .map_err(|e| ApiError::InvalidPath {
            message: format!("dosya çözümlenemedi: {e}"),
        })?;
    if !joined.starts_with(&base) {
        return Err(ApiError::InvalidPath {
            message: "dosya jadx alanının dışında".into(),
        });
    }
    Ok(joined)
}

fn collect_java(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_java(&path, out);
        } else if path.extension().and_then(|e| e.to_str()) == Some("java") {
            out.push(path);
        }
    }
}

#[derive(Serialize)]
pub struct JadxResult {
    pub out_dir: String,
    pub java_files: usize,
}

/// APK'yı jadx ile Java kaynağına decompile eder (`--no-res`, sadece kod).
#[tauri::command]
pub fn jadx_decompile(path: String) -> Result<JadxResult, ApiError> {
    let apk = validate_file(&path, APK_EXTS)?;
    let jadx = tool("jadx")?;
    let stem = apk
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "app".into());
    let root = jadx_root()?;
    std::fs::create_dir_all(&root).map_err(|e| ApiError::Io {
        message: e.to_string(),
    })?;
    let out_dir = root.join(&stem);
    let _ = std::fs::remove_dir_all(&out_dir);

    // jadx karmaşık sınıflarda hata/uyarı verip non-zero dönebilir (örn. 535 hata).
    // Ancak dosyaların %99'u çıkarılmışsa bunu başarılı kabul ediyoruz.
    let _ = run(
        &jadx,
        &[
            "-d".into(),
            out_dir.to_string_lossy().into_owned(),
            "--no-res".into(),
            "--show-bad-code".into(),
            apk.to_string_lossy().into_owned(),
        ],
    );

    let mut files = Vec::new();
    collect_java(&out_dir, &mut files);
    if files.is_empty() {
        return Err(ApiError::ProcessFailed {
            message: "jadx hiçbir Java dosyası üretemedi".into(),
        });
    }
    Ok(JadxResult {
        out_dir: out_dir.to_string_lossy().into_owned(),
        java_files: files.len(),
    })
}

/// jadx çıktısının önbellek durumu — Java sekmesi açılınca kontrol edilir.
#[derive(Serialize)]
pub struct CacheStatus {
    pub cached: bool,
    pub out_dir: String,
    pub files: usize,
}

/// `~/.cache/byteforge/jadx/<apk>` altında zaten Java çıktısı var mı? Varsa
/// kullanıcı tekrar decompile etmeden doğrudan arama/görüntüleme moduna geçebilir.
#[tauri::command]
pub fn jadx_cache_status(path: String) -> Result<CacheStatus, ApiError> {
    let apk = validate_file(&path, APK_EXTS)?;
    let stem = apk
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "app".into());
    let out_dir = jadx_root()?.join(&stem);
    let mut files = Vec::new();
    if out_dir.exists() {
        collect_java(&out_dir, &mut files);
    }
    Ok(CacheStatus {
        cached: !files.is_empty(),
        out_dir: out_dir.to_string_lossy().into_owned(),
        files: files.len(),
    })
}

/// APK'yı jadx ile **canlı akışla** decompile eder: ilerleme ve sonuç
/// `on_event` kanalıyla gelir (UI donmaz). Kısmi başarı toleranslı.
#[tauri::command]
pub fn jadx_decompile_stream(
    path: String,
    on_event: tauri::ipc::Channel<crate::stream::StreamEvent>,
) -> Result<(), ApiError> {
    let apk = validate_file(&path, APK_EXTS)?;
    let jadx = tool("jadx")?;
    let stem = apk
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "app".into());
    let root = jadx_root()?;
    std::fs::create_dir_all(&root).map_err(|e| ApiError::Io {
        message: e.to_string(),
    })?;
    let out_dir = root.join(&stem);
    let _ = std::fs::remove_dir_all(&out_dir);

    let args = vec![
        "-d".into(),
        out_dir.to_string_lossy().into_owned(),
        "--no-res".into(),
        "--show-bad-code".into(),
        "--threads-count".into(),
        num_cpus_arg(),
        apk.to_string_lossy().into_owned(),
    ];
    crate::stream::run_stream(jadx, args, out_dir, "java", on_event);
    Ok(())
}

/// jadx iş parçacığı sayısı — çekirdek sayısına göre (varsayılan 4).
fn num_cpus_arg() -> String {
    std::thread::available_parallelism()
        .map(|n| n.get().to_string())
        .unwrap_or_else(|_| "4".into())
}

#[derive(Serialize)]
pub struct JavaMatch {
    pub file: String,
    pub line: usize,
    pub text: String,
}

/// Decompile edilmiş Java kaynağında metin araması.
#[tauri::command]
pub fn search_java(out_dir: String, query: String) -> Result<Vec<JavaMatch>, ApiError> {
    let base = safe_jadx_dir(&out_dir)?;
    let needle = query.to_lowercase();
    if needle.is_empty() {
        return Ok(Vec::new());
    }
    let mut files = Vec::new();
    collect_java(&base, &mut files);

    let mut results = Vec::new();
    for file in files {
        let Ok(content) = std::fs::read_to_string(&file) else {
            continue;
        };
        let rel = file
            .strip_prefix(&base)
            .unwrap_or(&file)
            .to_string_lossy()
            .into_owned();
        for (i, line) in content.lines().enumerate() {
            if line.to_lowercase().contains(&needle) {
                results.push(JavaMatch {
                    file: rel.clone(),
                    line: i + 1,
                    text: line.trim().to_string(),
                });
                if results.len() >= MAX_RESULTS {
                    return Ok(results);
                }
            }
        }
    }
    Ok(results)
}

/// Bir Java dosyasının içeriğini döndürür (salt-okunur görüntüleme).
#[tauri::command]
pub fn read_java(out_dir: String, file: String) -> Result<String, ApiError> {
    let path = safe_join(&out_dir, &file)?;
    std::fs::read_to_string(&path).map_err(|e| ApiError::Io {
        message: e.to_string(),
    })
}
