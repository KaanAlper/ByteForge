use crate::commands::{validate_file, ApiError, APK_EXTS};
use crate::deploy::{run, tool};
use byteforge_core::smali::{
    apktool_build_args, apktool_decode_args, apply_forced_return, find_methods, ForcedReturn,
    SmaliMethod,
};
use serde::Serialize;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const MAX_RESULTS: usize = 300;

/// Decode edilmiş projeler için kök: `~/.cache/byteforge/decoded`.
fn decoded_root() -> Result<PathBuf, ApiError> {
    let home = std::env::var("HOME").map_err(|_| ApiError::ProcessFailed {
        message: "HOME ortam değişkeni tanımlı değil".into(),
    })?;
    Ok(Path::new(&home)
        .join(".cache")
        .join("byteforge")
        .join("decoded"))
}

/// Frontend'den gelen bir out_dir'in gerçekten decoded kökü altında olduğunu doğrular.
fn safe_decoded_dir(out_dir: &str) -> Result<PathBuf, ApiError> {
    let root = decoded_root()?
        .canonicalize()
        .map_err(|e| ApiError::InvalidPath {
            message: format!("decoded kök yok: {e}"),
        })?;
    let dir = PathBuf::from(out_dir)
        .canonicalize()
        .map_err(|e| ApiError::InvalidPath {
            message: format!("dizin çözümlenemedi: {e}"),
        })?;
    if !dir.starts_with(&root) {
        return Err(ApiError::InvalidPath {
            message: "dizin decoded alanının dışında".into(),
        });
    }
    Ok(dir)
}

/// out_dir altındaki göreli bir dosya yolunu, sınır dışına çıkmadığını doğrulayarak birleştirir.
fn safe_join(out_dir: &str, rel: &str) -> Result<PathBuf, ApiError> {
    let base = safe_decoded_dir(out_dir)?;
    let joined = base
        .join(rel)
        .canonicalize()
        .map_err(|e| ApiError::InvalidPath {
            message: format!("dosya çözümlenemedi: {e}"),
        })?;
    if !joined.starts_with(&base) {
        return Err(ApiError::InvalidPath {
            message: "dosya proje alanının dışında".into(),
        });
    }
    Ok(joined)
}

/// Bir dizindeki tüm `.smali` dosyalarını özyinelemeli toplar.
fn collect_smali(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_smali(&path, out);
        } else if path.extension().and_then(|e| e.to_str()) == Some("smali") {
            out.push(path);
        }
    }
}

#[derive(Serialize)]
pub struct DecodeResult {
    pub out_dir: String,
    pub smali_files: usize,
}

/// APK'yı apktool ile `~/.cache/byteforge/decoded/<ad>` altına açar.
#[tauri::command]
pub fn decode_apk(path: String) -> Result<DecodeResult, ApiError> {
    let apk = validate_file(&path, APK_EXTS)?;
    let apktool = tool("apktool")?;

    let stem = apk
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "app".into());
    let root = decoded_root()?;
    std::fs::create_dir_all(&root).map_err(|e| ApiError::Io {
        message: e.to_string(),
    })?;
    let out_dir = root.join(&stem);
    let _ = std::fs::remove_dir_all(&out_dir);

    run(&apktool, &apktool_decode_args(&apk, &out_dir))?;

    let mut files = Vec::new();
    collect_smali(&out_dir, &mut files);
    Ok(DecodeResult {
        out_dir: out_dir.to_string_lossy().into_owned(),
        smali_files: files.len(),
    })
}

/// Smali decode önbellek durumu.
#[derive(Serialize)]
pub struct SmaliCacheStatus {
    pub cached: bool,
    pub out_dir: String,
    pub files: usize,
}

/// `~/.cache/byteforge/decoded/<apk>` zaten decode edilmiş mi? Varsa Smali sekmesi
/// buton beklemeden anlık açılır.
#[tauri::command]
pub fn smali_cache_status(path: String) -> Result<SmaliCacheStatus, ApiError> {
    let apk = validate_file(&path, APK_EXTS)?;
    let stem = apk
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "app".into());
    let out_dir = decoded_root()?.join(&stem);
    let mut files = Vec::new();
    // apktool.yml varlığı = geçerli bir decode projesi.
    let valid = out_dir.join("apktool.yml").is_file();
    if valid {
        collect_smali(&out_dir, &mut files);
    }
    Ok(SmaliCacheStatus {
        cached: valid && !files.is_empty(),
        out_dir: out_dir.to_string_lossy().into_owned(),
        files: files.len(),
    })
}

/// APK'yı apktool ile **canlı akışla** decode eder: apktool aşama satırları
/// (`I: Baksmaling...`) UI'a anlık gelir (donma yok). Sonuç `on_event` ile.
#[tauri::command]
pub fn decode_apk_stream(
    path: String,
    on_event: tauri::ipc::Channel<crate::stream::StreamEvent>,
) -> Result<(), ApiError> {
    let apk = validate_file(&path, APK_EXTS)?;
    let apktool = tool("apktool")?;
    let stem = apk
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "app".into());
    let root = decoded_root()?;
    std::fs::create_dir_all(&root).map_err(|e| ApiError::Io {
        message: e.to_string(),
    })?;
    let out_dir = root.join(&stem);
    let _ = std::fs::remove_dir_all(&out_dir);

    crate::stream::run_stream(
        apktool,
        apktool_decode_args(&apk, &out_dir),
        out_dir,
        "smali",
        on_event,
    );
    Ok(())
}

#[derive(Serialize)]
pub struct SmaliMatch {
    /// out_dir'e göreli dosya yolu.
    pub file: String,
    pub line: usize,
    pub text: String,
}

/// Decode edilmiş projede metin araması (case-insensitive, ilk MAX_RESULTS eşleşme).
#[tauri::command]
pub fn search_smali(out_dir: String, query: String) -> Result<Vec<SmaliMatch>, ApiError> {
    let base = safe_decoded_dir(&out_dir)?;
    let needle = query.to_lowercase();
    if needle.is_empty() {
        return Ok(Vec::new());
    }

    let mut files = Vec::new();
    collect_smali(&base, &mut files);

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
                results.push(SmaliMatch {
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

/// Sezgisel puanlanmış smali hedef adayı.
#[derive(Serialize)]
pub struct SmaliTarget {
    pub file: String,
    pub signature: String,
    pub return_type: String,
    pub score: i32,
    pub confidence: String,
    pub reasons: Vec<String>,
    /// bool/int dönüş → doğrudan "return true" ile zorlanabilir.
    pub patchable: bool,
    /// Amaç kategorisi (character/economy/purchase_vip/ads/reward/other).
    pub category: String,
    /// Kategoriye önerilen yama (return_true / return_max_int).
    pub suggested_template: String,
    /// Satın-alma/işlem akışı → çökme riski.
    pub risky: bool,
}

/// Decode edilmiş projede sezgisel hedef taraması: her metodu semantik puanlar
/// (isim/bool/parametresiz/erişim kelimeleri) ve en yüksek adayları döndürür.
#[tauri::command]
pub fn smali_smart_targets(out_dir: String) -> Result<Vec<SmaliTarget>, ApiError> {
    let base = safe_decoded_dir(&out_dir)?;
    let mut files = Vec::new();
    collect_smali(&base, &mut files);

    let mut out: Vec<SmaliTarget> = Vec::new();
    for file in files {
        let Ok(content) = std::fs::read_to_string(&file) else {
            continue;
        };
        let rel = file
            .strip_prefix(&base)
            .unwrap_or(&file)
            .to_string_lossy()
            .into_owned();
        // Sınıf yolu: baştaki "smali*/" segmentini at → "com/google/..." vb.
        let class_path = rel.split_once('/').map(|(_, r)| r).unwrap_or(&rel);
        // Üçüncü-parti SDK / kütüphane kodunu tamamen ele.
        if byteforge_core::heuristic::is_library_path(class_path) {
            continue;
        }
        for m in find_methods(&content) {
            // İsim = imzada '(' öncesi; parametresiz = "()".
            let name = m.signature.split('(').next().unwrap_or(&m.signature);
            let ret_bool = m.return_type == "Z";
            let parameterless = m.signature.contains("()");
            let s = byteforge_core::heuristic::score_symbol(
                name,
                Some(ret_bool),
                Some(parameterless),
            );
            if s.excluded || s.score < 40 {
                continue;
            }
            let patchable = matches!(m.return_type.as_str(), "Z" | "B" | "S" | "C" | "I");
            out.push(SmaliTarget {
                file: rel.clone(),
                signature: m.signature.clone(),
                return_type: m.return_type.clone(),
                score: s.score,
                confidence: s.confidence,
                reasons: s.reasons,
                patchable,
                category: s.category,
                suggested_template: s.suggested_template,
                risky: s.risky,
            });
        }
        if out.len() > 4000 {
            break;
        }
    }
    out.sort_by_key(|t| std::cmp::Reverse(t.score));
    out.truncate(300);
    Ok(out)
}

/// Bir kural eşleşmesi + hangi dosyada bulunduğu (frontend gezinmesi için).
#[derive(Serialize)]
pub struct SmaliRuleHit {
    /// out_dir'e göreli dosya yolu.
    pub file: String,
    pub rule: String,
    pub category: String,
    pub method: Option<String>,
    pub line: usize,
    pub snippet: String,
    pub suggestion: String,
}

/// Üst sınır — çok büyük projelerde UI'yi boğmamak için.
const MAX_RULE_HITS: usize = 500;

/// Decode edilmiş projede "hızlı kural" taraması: premium boolean getter'ları,
/// imza doğrulama, faturalandırma, root/lisans kalıpları. Yama uygulamaz.
#[tauri::command]
pub fn scan_smali_rules(out_dir: String) -> Result<Vec<SmaliRuleHit>, ApiError> {
    let base = safe_decoded_dir(&out_dir)?;
    let mut files = Vec::new();
    collect_smali(&base, &mut files);

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
        for h in byteforge_core::smali_rules::scan_rules(&content) {
            results.push(SmaliRuleHit {
                file: rel.clone(),
                rule: h.rule,
                category: h.category,
                method: h.method,
                line: h.line,
                snippet: h.snippet,
                suggestion: h.suggestion,
            });
            if results.len() >= MAX_RULE_HITS {
                return Ok(results);
            }
        }
    }
    Ok(results)
}

/// Bir smali dosyasındaki metodları listeler.
#[tauri::command]
pub fn list_smali_methods(out_dir: String, file: String) -> Result<Vec<SmaliMethod>, ApiError> {
    let path = safe_join(&out_dir, &file)?;
    let text = std::fs::read_to_string(&path).map_err(|e| ApiError::Io {
        message: e.to_string(),
    })?;
    Ok(find_methods(&text))
}

/// Bir metoda zorunlu dönüş yaması uygular (dosyayı yerinde günceller).
#[tauri::command]
pub fn patch_smali_method(
    out_dir: String,
    file: String,
    method_signature: String,
    forced: ForcedReturn,
) -> Result<(), ApiError> {
    let path = safe_join(&out_dir, &file)?;
    let text = std::fs::read_to_string(&path).map_err(|e| ApiError::Io {
        message: e.to_string(),
    })?;
    let patched = apply_forced_return(&text, &method_signature, forced).map_err(ApiError::from)?;
    std::fs::write(&path, patched).map_err(|e| ApiError::Io {
        message: e.to_string(),
    })?;
    Ok(())
}

/// Decode edilmiş projeyi apktool ile yeniden paketler. Çıktı APK yolunu döndürür
/// (imzasız — sonra M5 'zipalign + imzala' ile imzalanmalı).
#[tauri::command]
pub fn build_apk(out_dir: String) -> Result<String, ApiError> {
    let base = safe_decoded_dir(&out_dir)?;
    let apktool = tool("apktool")?;
    let name = base
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "app".into());
    let output = base
        .parent()
        .unwrap_or(Path::new("."))
        .join(format!("{name}-patched.apk"));
    run(&apktool, &apktool_build_args(&base, &output))?;
    Ok(output.to_string_lossy().into_owned())
}

/// Checkpoint (decode dizini yedeği) kökü.
fn checkpoints_root() -> Result<PathBuf, ApiError> {
    let home = std::env::var("HOME").map_err(|_| ApiError::ProcessFailed {
        message: "HOME tanımlı değil".into(),
    })?;
    Ok(Path::new(&home)
        .join(".cache")
        .join("byteforge")
        .join("checkpoints"))
}

/// Bir dizini özyinelemeli kopyalar.
fn copy_dir_all(src: &Path, dst: &Path) -> Result<(), ApiError> {
    std::fs::create_dir_all(dst).map_err(|e| ApiError::Io {
        message: e.to_string(),
    })?;
    let entries = std::fs::read_dir(src).map_err(|e| ApiError::Io {
        message: e.to_string(),
    })?;
    for entry in entries.flatten() {
        let ty = match entry.file_type() {
            Ok(t) => t,
            Err(_) => continue,
        };
        let dest = dst.join(entry.file_name());
        if ty.is_dir() {
            copy_dir_all(&entry.path(), &dest)?;
        } else {
            std::fs::copy(entry.path(), &dest).map_err(|e| ApiError::Io {
                message: e.to_string(),
            })?;
        }
    }
    Ok(())
}

#[derive(Serialize)]
pub struct CheckpointInfo {
    pub name: String,
    pub path: String,
}

/// Decode edilmiş projenin anlık yedeğini (checkpoint) alır.
#[tauri::command]
pub fn create_checkpoint(out_dir: String) -> Result<CheckpointInfo, ApiError> {
    let base = safe_decoded_dir(&out_dir)?;
    let stem = base
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "proj".into());
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let name = format!("{stem}__{ts}");
    let dest = checkpoints_root()?.join(&name);
    copy_dir_all(&base, &dest)?;
    Ok(CheckpointInfo {
        name,
        path: dest.to_string_lossy().into_owned(),
    })
}

/// Bu proje için alınmış checkpoint'leri (en yeni önce) listeler.
#[tauri::command]
pub fn list_checkpoints(out_dir: String) -> Result<Vec<CheckpointInfo>, ApiError> {
    let base = safe_decoded_dir(&out_dir)?;
    let stem = base
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let prefix = format!("{stem}__");
    let root = checkpoints_root()?;
    let mut out = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&root) {
        for e in entries.flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            if name.starts_with(&prefix) {
                out.push(CheckpointInfo {
                    name,
                    path: e.path().to_string_lossy().into_owned(),
                });
            }
        }
    }
    out.sort_by(|a, b| b.name.cmp(&a.name));
    Ok(out)
}

/// Bir checkpoint'i decode dizinine geri yükler.
#[tauri::command]
pub fn restore_checkpoint(checkpoint_path: String, out_dir: String) -> Result<String, ApiError> {
    let base = safe_decoded_dir(&out_dir)?;
    let root = checkpoints_root()?
        .canonicalize()
        .map_err(|e| ApiError::InvalidPath {
            message: format!("checkpoint kökü yok: {e}"),
        })?;
    let cp = PathBuf::from(&checkpoint_path)
        .canonicalize()
        .map_err(|e| ApiError::InvalidPath {
            message: format!("checkpoint yok: {e}"),
        })?;
    if !cp.starts_with(&root) {
        return Err(ApiError::InvalidPath {
            message: "checkpoint alanının dışında".into(),
        });
    }
    let _ = std::fs::remove_dir_all(&base);
    copy_dir_all(&cp, &base)?;
    Ok("Checkpoint geri yüklendi.".into())
}

/// Bir XML satırından `attr="değer"` değerini çıkarır.
fn extract_attr(line: &str, attr: &str) -> Option<String> {
    let pat = format!("{attr}=\"");
    let start = line.find(&pat)? + pat.len();
    let rest = &line[start..];
    let end = rest.find('"')?;
    Some(rest[..end].to_string())
}

/// Decode edilmiş AndroidManifest.xml'deki uses-permission adlarını listeler.
#[tauri::command]
pub fn list_manifest_permissions(out_dir: String) -> Result<Vec<String>, ApiError> {
    let base = safe_decoded_dir(&out_dir)?;
    let manifest = base.join("AndroidManifest.xml");
    let text = std::fs::read_to_string(&manifest).map_err(|e| ApiError::Io {
        message: e.to_string(),
    })?;
    let mut perms = Vec::new();
    for line in text.lines() {
        if line.contains("uses-permission") {
            if let Some(name) = extract_attr(line, "android:name") {
                perms.push(name);
            }
        }
    }
    Ok(perms)
}

/// Seçilen izinleri manifest'ten kaldırır. Kaldırılan izin sayısını döndürür.
#[tauri::command]
pub fn remove_manifest_permissions(
    out_dir: String,
    permissions: Vec<String>,
) -> Result<usize, ApiError> {
    let base = safe_decoded_dir(&out_dir)?;
    let manifest = base.join("AndroidManifest.xml");
    let text = std::fs::read_to_string(&manifest).map_err(|e| ApiError::Io {
        message: e.to_string(),
    })?;
    let drop: HashSet<&String> = permissions.iter().collect();
    let mut removed = 0usize;
    let kept: Vec<&str> = text
        .lines()
        .filter(|line| {
            if line.contains("uses-permission") {
                if let Some(name) = extract_attr(line, "android:name") {
                    if drop.contains(&name) {
                        removed += 1;
                        return false;
                    }
                }
            }
            true
        })
        .collect();
    std::fs::write(&manifest, kept.join("\n")).map_err(|e| ApiError::Io {
        message: e.to_string(),
    })?;
    Ok(removed)
}

/// Bir smali dosyasının ham içeriğini döndürür (kod editörü).
#[tauri::command]
pub fn read_smali_file(out_dir: String, file: String) -> Result<String, ApiError> {
    let path = safe_join(&out_dir, &file)?;
    std::fs::read_to_string(&path).map_err(|e| ApiError::Io {
        message: e.to_string(),
    })
}

/// Bir smali dosyasının içeriğini yazar (kod editöründen kaydet).
#[tauri::command]
pub fn write_smali_file(out_dir: String, file: String, content: String) -> Result<(), ApiError> {
    let path = safe_join(&out_dir, &file)?;
    std::fs::write(&path, content).map_err(|e| ApiError::Io {
        message: e.to_string(),
    })?;
    Ok(())
}
