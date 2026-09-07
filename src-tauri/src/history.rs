use crate::commands::{validate_any_file, ApiError};
use byteforge_core::hexedit::parse_hex;
use byteforge_core::patch::write_patched_file;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// Uygulanan tek bir baytsal yama kaydı (geri alınabilir).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatchRecord {
    pub id: u64,
    pub ts: u64,
    /// "so" | "hex"
    pub kind: String,
    pub path: String,
    pub offset: usize,
    pub old_hex: String,
    pub new_hex: String,
    pub note: String,
    #[serde(default)]
    pub reverted: bool,
}

fn history_file() -> Result<PathBuf, ApiError> {
    let home = std::env::var("HOME").map_err(|_| ApiError::ProcessFailed {
        message: "HOME tanımlı değil".into(),
    })?;
    let dir = Path::new(&home).join(".config").join("byteforge");
    std::fs::create_dir_all(&dir).map_err(|e| ApiError::Io {
        message: e.to_string(),
    })?;
    Ok(dir.join("patch-history.json"))
}

fn read_all() -> Vec<PatchRecord> {
    let Ok(path) = history_file() else {
        return Vec::new();
    };
    std::fs::read_to_string(path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn write_all(records: &[PatchRecord]) -> Result<(), ApiError> {
    let path = history_file()?;
    let json = serde_json::to_string_pretty(records).map_err(|e| ApiError::Io {
        message: e.to_string(),
    })?;
    std::fs::write(path, json).map_err(|e| ApiError::Io {
        message: e.to_string(),
    })
}

fn now_micros() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_micros() as u64)
        .unwrap_or(0)
}

/// Bir yama kaydını günlüğe ekler (patch komutları tarafından çağrılır).
pub(crate) fn append_record(
    kind: &str,
    path: &str,
    offset: usize,
    old_hex: &str,
    new_hex: &str,
    note: &str,
) {
    let mut records = read_all();
    records.push(PatchRecord {
        id: now_micros(),
        ts: now_micros() / 1000,
        kind: kind.to_string(),
        path: path.to_string(),
        offset,
        old_hex: old_hex.to_string(),
        new_hex: new_hex.to_string(),
        note: note.to_string(),
        reverted: false,
    });
    let _ = write_all(&records);
}

/// Tüm yama geçmişini (en yeni önce) döndürür.
#[tauri::command]
pub fn patch_history() -> Result<Vec<PatchRecord>, ApiError> {
    let mut records = read_all();
    records.reverse();
    Ok(records)
}

/// Bir yamayı, eski baytları geri yazarak geri alır ve kaydı işaretler.
#[tauri::command]
pub fn revert_patch(id: u64) -> Result<String, ApiError> {
    let mut records = read_all();
    let rec = records
        .iter()
        .find(|r| r.id == id)
        .cloned()
        .ok_or_else(|| ApiError::InvalidPath {
            message: "yama kaydı bulunamadı".into(),
        })?;
    let old = parse_hex(&rec.old_hex).map_err(ApiError::from)?;
    let canonical = validate_any_file(&rec.path)?;
    write_patched_file(&canonical, rec.offset, &old).map_err(ApiError::from)?;

    for r in records.iter_mut() {
        if r.id == id {
            r.reverted = true;
        }
    }
    write_all(&records)?;
    Ok(format!("geri alındı: {} @0x{:x}", rec.path, rec.offset))
}

/// Tüm yama geçmişini temizler (dosyalara dokunmaz).
#[tauri::command]
pub fn clear_patch_history() -> Result<(), ApiError> {
    write_all(&[])
}

/// Belirli bir dosya+ofsetteki (geri alınmamış) yamayı bulup geri alır.
/// Native panelde "MODLU" bir fonksiyonu tek tıkla orijinaline döndürmek için.
/// Yol canonicalize edilerek geçmişteki kayıtla eşleştirilir.
#[tauri::command]
pub fn revert_patch_at(path: String, offset: usize) -> Result<String, ApiError> {
    let canonical = validate_any_file(&path)?;
    let target = canonical.to_string_lossy();
    let mut records = read_all();
    let id = records
        .iter()
        .find(|r| !r.reverted && r.offset == offset && r.path == target)
        .map(|r| r.id)
        .ok_or_else(|| ApiError::InvalidPath {
            message: "bu ofset için geçmişte yama kaydı yok (bu oturumda yamalanmamış olabilir)"
                .into(),
        })?;
    let rec = records.iter().find(|r| r.id == id).cloned().unwrap();
    let old = parse_hex(&rec.old_hex).map_err(ApiError::from)?;
    write_patched_file(&canonical, rec.offset, &old).map_err(ApiError::from)?;
    for r in records.iter_mut() {
        if r.id == id {
            r.reverted = true;
        }
    }
    write_all(&records)?;
    Ok(format!("geri alındı: {} @0x{:x}", rec.path, rec.offset))
}

/// Bir dosya+ofsette geri alınabilir (geri alınmamış) yama kaydı var mı?
/// Native panelde Revert butonunu göstermek için.
#[tauri::command]
pub fn has_revertible_patch(path: String, offset: usize) -> Result<bool, ApiError> {
    let canonical = validate_any_file(&path)?;
    let target = canonical.to_string_lossy();
    Ok(read_all()
        .iter()
        .any(|r| !r.reverted && r.offset == offset && r.path == target))
}

/// Yama geçmişini paylaşılabilir bir tarif (recipe) JSON'una dönüştürür.
/// Geri alınmış kayıtları atlar; mutlak yol yerine dosya adı taşır.
#[tauri::command]
pub fn export_recipe(name: String, note: String, app_hint: String) -> Result<String, ApiError> {
    let records = read_all();
    let steps: Vec<byteforge_core::recipe::RecipeStep> = records
        .iter()
        .filter(|r| !r.reverted)
        .map(|r| byteforge_core::recipe::RecipeStep {
            kind: r.kind.clone(),
            target: Path::new(&r.path)
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| r.path.clone()),
            offset: r.offset,
            old_hex: r.old_hex.clone(),
            new_hex: r.new_hex.clone(),
            note: r.note.clone(),
        })
        .collect();
    let recipe = byteforge_core::recipe::Recipe {
        version: byteforge_core::recipe::RECIPE_VERSION,
        name,
        note,
        app_hint,
        steps,
    };
    byteforge_core::recipe::to_json(&recipe).map_err(ApiError::from)
}

/// Bir tarif JSON'unu ayrıştırır (biçim/sürüm doğrulamasıyla).
#[tauri::command]
pub fn import_recipe(json: String) -> Result<byteforge_core::recipe::Recipe, ApiError> {
    byteforge_core::recipe::parse_recipe(&json).map_err(ApiError::from)
}

/// Bir tarif adımının, seçilen dosyaya karşı durumu.
#[derive(Serialize)]
pub struct StepCheck {
    pub index: usize,
    pub target: String,
    pub note: String,
    pub status: byteforge_core::recipe::StepStatus,
    /// Dosya adı bu adımın hedefiyle eşleşiyor mu (basename).
    pub target_matches: bool,
}

/// Bir tarifi seçilen dosyaya karşı doğrular (yazma yapmaz). Yalnızca dosya
/// adı eşleşen adımlar bayt-düzeyi doğrulanır; diğerleri OutOfRange raporlanır.
#[tauri::command]
pub fn verify_recipe(json: String, path: String) -> Result<Vec<StepCheck>, ApiError> {
    let recipe = byteforge_core::recipe::parse_recipe(&json).map_err(ApiError::from)?;
    let canonical = validate_any_file(&path)?;
    let base = canonical
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let bytes = std::fs::read(&canonical).map_err(|e| ApiError::Io {
        message: e.to_string(),
    })?;
    let checks = recipe
        .steps
        .iter()
        .enumerate()
        .map(|(i, s)| {
            let matches = s.target == base;
            StepCheck {
                index: i,
                target: s.target.clone(),
                note: s.note.clone(),
                status: if matches {
                    byteforge_core::recipe::verify_step(&bytes, s)
                } else {
                    byteforge_core::recipe::StepStatus::OutOfRange
                },
                target_matches: matches,
            }
        })
        .collect();
    Ok(checks)
}
