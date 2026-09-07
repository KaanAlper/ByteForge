use crate::commands::{validate_any_file, validate_file, ApiError, APK_EXTS};
use crate::deploy::{run, tool};
use byteforge_core::archive::extract_entries_with_ext;
use serde::Serialize;
use std::path::Path;

#[derive(Serialize)]
pub struct SplitResult {
    pub out_dir: String,
    pub apks: Vec<String>,
}

/// Bir XAPK/APKS/ZIP içindeki tüm `.apk` parçalarını çıkarır.
#[tauri::command]
pub fn extract_splits(path: String) -> Result<SplitResult, ApiError> {
    let pkg = validate_file(&path, APK_EXTS)?;
    let home = std::env::var("HOME").map_err(|_| ApiError::ProcessFailed {
        message: "HOME tanımlı değil".into(),
    })?;
    let stem = pkg
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "pkg".into());
    let dir = Path::new(&home)
        .join(".cache")
        .join("byteforge")
        .join("splits")
        .join(&stem);
    let _ = std::fs::remove_dir_all(&dir);

    let apks = extract_entries_with_ext(&pkg, &dir, ".apk").map_err(ApiError::from)?;
    Ok(SplitResult {
        out_dir: dir.to_string_lossy().into_owned(),
        apks: apks
            .iter()
            .map(|p| p.to_string_lossy().into_owned())
            .collect(),
    })
}

/// Çıkarılan split APK'ları `adb install-multiple -r` ile tek seferde kurar.
#[tauri::command]
pub fn install_splits(serial: Option<String>, apks: Vec<String>) -> Result<String, ApiError> {
    if apks.is_empty() {
        return Err(ApiError::InvalidPath {
            message: "kurulacak APK yok".into(),
        });
    }
    let adb = tool("adb")?;
    let mut args: Vec<String> = Vec::new();
    if let Some(s) = &serial {
        args.push("-s".into());
        args.push(s.clone());
    }
    args.push("install-multiple".into());
    args.push("-r".into());
    for a in &apks {
        let p = validate_any_file(a)?;
        args.push(p.to_string_lossy().into_owned());
    }
    let out = run(&adb, &args)?;
    Ok(if out.trim().is_empty() {
        "Split kurulum tamamlandı.".into()
    } else {
        out.trim().to_string()
    })
}
