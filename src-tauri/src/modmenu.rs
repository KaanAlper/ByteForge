//! Yüzen Mod Menüsü enjeksiyon hattı (orkestrasyon).
//!
//! decode (apktool) → manifest izni → MenuLoader.smali enjekte → launcher
//! onCreate kancası → statik smali-boolean özellikleri uygula → yeniden derle.
//! IL2CPP/native özellikleri ertelenir (Native sekmesinde uygulanır).

use crate::commands::{validate_file, ApiError, APK_EXTS};
use crate::deploy::{run, tool};
use byteforge_core::modmenu::{
    ensure_permission, insert_oncreate_hook, menu_loader_smali, FeatureTarget, MenuConfig,
    HOOK_LINE, OVERLAY_PERMISSION,
};
use byteforge_core::smali::{
    apktool_build_args, apktool_decode_args, apply_forced_return, ForcedReturn,
};
use serde::Serialize;
use std::path::{Path, PathBuf};

fn modmenu_root() -> Result<PathBuf, ApiError> {
    let home = std::env::var("HOME").map_err(|_| ApiError::ProcessFailed {
        message: "HOME tanımlı değil".into(),
    })?;
    let dir = Path::new(&home).join(".cache").join("byteforge").join("modmenu");
    std::fs::create_dir_all(&dir).map_err(|e| ApiError::Io {
        message: e.to_string(),
    })?;
    Ok(dir)
}

/// Bir sınıf/dosya yolunu (ör. "com/app/MainActivity") tüm `smali*` dizinlerinde
/// arar, bulursa tam yolu döndürür.
fn find_smali_file(out_dir: &Path, rel_class: &str) -> Option<PathBuf> {
    let entries = std::fs::read_dir(out_dir).ok()?;
    for entry in entries.flatten() {
        let p = entry.path();
        if p.is_dir()
            && p.file_name()
                .and_then(|n| n.to_str())
                .map(|n| n == "smali" || n.starts_with("smali_"))
                .unwrap_or(false)
        {
            let candidate = p.join(format!("{rel_class}.smali"));
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

/// Launcher activity adını (ör. ".MainActivity" / "com.app.MainActivity") paket
/// adıyla çözüp smali göreli yoluna çevirir ("com/app/MainActivity").
fn activity_rel_class(name: &str, package: &str) -> String {
    let full = if let Some(stripped) = name.strip_prefix('.') {
        format!("{package}.{stripped}")
    } else if !name.contains('.') {
        format!("{package}.{name}")
    } else {
        name.to_string()
    };
    full.replace('.', "/")
}

/// Enjeksiyon sonucu.
#[derive(Serialize)]
pub struct InjectResult {
    /// Yeniden derlenmiş APK yolu (imzasız — Dağıt sekmesinde imzalayın).
    pub out_apk: String,
    /// Uygulanan adımlar (insan-okur).
    pub steps: Vec<String>,
    /// Statik uygulanamayan (Native sekmesinde uygulanacak) IL2CPP/native özellikler.
    pub deferred_native: Vec<String>,
}

/// Bir APK'ya yüzen mod menüsü enjekte eder ve yeniden derler (imzasız).
#[tauri::command]
pub fn inject_mod_menu(path: String, config: MenuConfig) -> Result<InjectResult, ApiError> {
    let apk = validate_file(&path, APK_EXTS)?;
    let apktool = tool("apktool")?;
    let mut steps: Vec<String> = Vec::new();

    let stem = apk
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "app".into());
    let out_dir = modmenu_root()?.join(&stem);
    let _ = std::fs::remove_dir_all(&out_dir);

    // 1) Decode.
    run(&apktool, &apktool_decode_args(&apk, &out_dir))?;
    steps.push("APK apktool ile açıldı".into());

    // 2) Manifest izni (SYSTEM_ALERT_WINDOW).
    let manifest_path = out_dir.join("AndroidManifest.xml");
    let mx = std::fs::read_to_string(&manifest_path).map_err(|e| ApiError::Io {
        message: e.to_string(),
    })?;
    let mx2 = ensure_permission(&mx, OVERLAY_PERMISSION).map_err(ApiError::from)?;
    std::fs::write(&manifest_path, mx2).map_err(|e| ApiError::Io {
        message: e.to_string(),
    })?;
    steps.push("SYSTEM_ALERT_WINDOW izni eklendi".into());

    // 3) MenuLoader.smali enjekte (smali/com/byteforge/modmenu/).
    let loader_dir = out_dir.join("smali/com/byteforge/modmenu");
    std::fs::create_dir_all(&loader_dir).map_err(|e| ApiError::Io {
        message: e.to_string(),
    })?;
    std::fs::write(loader_dir.join("MenuLoader.smali"), menu_loader_smali(&config)).map_err(
        |e| ApiError::Io {
            message: e.to_string(),
        },
    )?;
    steps.push("MenuLoader.smali enjekte edildi".into());

    // 4) Launcher activity onCreate'ine kanca ekle (orijinal binary manifest'ten bul).
    let profile = byteforge_core::analyze_archive(&apk).map_err(ApiError::from)?;
    let manifest = profile.manifest.ok_or_else(|| ApiError::Manifest {
        message: "manifest çözümlenemedi — launcher activity bulunamadı".into(),
    })?;
    let package = manifest.package.clone().unwrap_or_default();
    let launcher = manifest
        .launchable_activities
        .first()
        .cloned()
        .ok_or_else(|| ApiError::Manifest {
            message: "launcher (MAIN/LAUNCHER) activity bulunamadı".into(),
        })?;
    let rel = activity_rel_class(&launcher, &package);
    let activity_file = find_smali_file(&out_dir, &rel).ok_or_else(|| ApiError::SmaliPatch {
        message: format!("launcher activity smali'si bulunamadı: {rel}.smali"),
    })?;
    let atext = std::fs::read_to_string(&activity_file).map_err(|e| ApiError::Io {
        message: e.to_string(),
    })?;
    let hooked = insert_oncreate_hook(&atext, HOOK_LINE).map_err(ApiError::from)?;
    std::fs::write(&activity_file, hooked).map_err(|e| ApiError::Io {
        message: e.to_string(),
    })?;
    steps.push(format!("Kanca eklendi: {launcher} → onCreate"));

    // 5) Statik smali-boolean özellikleri uygula; IL2CPP/native'i ertele.
    let mut deferred: Vec<String> = Vec::new();
    for f in &config.features {
        match &f.target {
            FeatureTarget::SmaliBoolean {
                class,
                method,
                ret,
            } => {
                // "Lcom/app/Billing;" → "com/app/Billing"
                let rel = class.trim_start_matches('L').trim_end_matches(';');
                let Some(file) = find_smali_file(&out_dir, rel) else {
                    deferred.push(format!("{} (sınıf bulunamadı: {class})", f.name));
                    continue;
                };
                let text = std::fs::read_to_string(&file).map_err(|e| ApiError::Io {
                    message: e.to_string(),
                })?;
                let forced = if *ret {
                    ForcedReturn::True
                } else {
                    ForcedReturn::False
                };
                match apply_forced_return(&text, method, forced) {
                    Ok(patched) => {
                        std::fs::write(&file, patched).map_err(|e| ApiError::Io {
                            message: e.to_string(),
                        })?;
                        steps.push(format!("Özellik uygulandı (smali): {}", f.name));
                    }
                    Err(e) => deferred.push(format!("{} (yama hatası: {e})", f.name)),
                }
            }
            FeatureTarget::Il2cppRva { offset, template } => {
                deferred.push(format!(
                    "{} → libil2cpp.so @0x{offset:x} = {template} (Native sekmesinde uygula)",
                    f.name
                ));
            }
            FeatureTarget::NativeOffset { offset, template } => {
                deferred.push(format!(
                    "{} → native @0x{offset:x} = {template} (Native sekmesinde uygula)",
                    f.name
                ));
            }
        }
    }

    // 6) Yeniden derle (imzasız).
    let out_apk = modmenu_root()?.join(format!("{stem}-modmenu.apk"));
    run(&apktool, &apktool_build_args(&out_dir, &out_apk))?;
    steps.push("APK yeniden derlendi (imzasız)".into());

    Ok(InjectResult {
        out_apk: out_apk.to_string_lossy().into_owned(),
        steps,
        deferred_native: deferred,
    })
}
