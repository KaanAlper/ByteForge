use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;
use once_cell::sync::Lazy;

static ICON_CACHE: Lazy<Mutex<HashMap<String, Option<String>>>> = Lazy::new(|| Mutex::new(HashMap::new()));
static DESKTOP_MAP: Lazy<Mutex<Option<HashMap<String, String>>>> = Lazy::new(|| Mutex::new(None));

pub fn get_icon_for_process(name: &str) -> Option<String> {
    let mut cache = ICON_CACHE.lock().unwrap();
    if let Some(cached) = cache.get(name) {
        return cached.clone();
    }
    
    let mut d_map_guard = DESKTOP_MAP.lock().unwrap();
    if d_map_guard.is_none() {
        *d_map_guard = Some(build_desktop_map());
    }
    let d_map = d_map_guard.as_ref().unwrap();
    
    // Process names can be cut off, or full names.
    // Let's try exact match, then lowercase, then prefix.
    let mut icon_name = d_map.get(name).cloned();
    if icon_name.is_none() {
        icon_name = d_map.get(&name.to_lowercase()).cloned();
    }
    
    // Also try finding an icon directly by process name if no desktop file matched
    let icon_name = icon_name.unwrap_or_else(|| name.to_string());
    
    let path = find_icon_file(&icon_name);
    let result = path.and_then(|p| {
        // Read file and convert to base64
        if let Ok(bytes) = fs::read(&p) {
            use base64::{Engine as _, engine::general_purpose};
            let b64 = general_purpose::STANDARD.encode(&bytes);
            let ext = p.extension().unwrap_or_default().to_string_lossy();
            let mime = if ext == "svg" { "image/svg+xml" } else { "image/png" };
            Some(format!("data:{};base64,{}", mime, b64))
        } else {
            None
        }
    });
    
    cache.insert(name.to_string(), result.clone());
    result
}

fn build_desktop_map() -> HashMap<String, String> {
    let mut map = HashMap::new();
    let dirs = vec![
        shellexpand::tilde("~/.local/share/applications").to_string(),
        "/usr/share/applications".to_string(),
    ];
    
    for dir in dirs {
        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.flatten() {
                if let Ok(content) = fs::read_to_string(entry.path()) {
                    let mut exec = String::new();
                    let mut name_val = String::new();
                    let mut icon = String::new();
                    for line in content.lines() {
                        if line.starts_with("Exec=") {
                            let raw = line.trim_start_matches("Exec=");
                            // Extract just the binary name
                            if let Some(bin) = raw.split_whitespace().next() {
                                if let Some(base) = bin.split('/').next_back() {
                                    exec = base.to_string();
                                }
                            }
                        } else if line.starts_with("Name=") {
                            name_val = line.trim_start_matches("Name=").to_lowercase();
                        } else if line.starts_with("Icon=") {
                            icon = line.trim_start_matches("Icon=").to_string();
                        }
                    }
                    if !icon.is_empty() {
                        if !exec.is_empty() {
                            map.insert(exec.clone(), icon.clone());
                            // also insert first 15 chars of exec to match Linux comm
                            if exec.len() > 15 {
                                map.insert(exec[..15].to_string(), icon.clone());
                            }
                        }
                        if !name_val.is_empty() {
                            map.insert(name_val.clone(), icon.clone());
                            let clean_name = name_val.replace(" ", "");
                            if clean_name.len() > 15 {
                                map.insert(clean_name[..15].to_string(), icon.clone());
                            } else {
                                map.insert(clean_name, icon.clone());
                            }
                        }
                    }
                }
            }
        }
    }
    map
}

fn find_icon_file(icon_name: &str) -> Option<PathBuf> {
    if icon_name.contains('/') && PathBuf::from(icon_name).exists() {
        return Some(PathBuf::from(icon_name));
    }
    
    let base_dirs = vec![
        shellexpand::tilde("~/.local/share/icons/hicolor").to_string(),
        "/usr/share/icons/hicolor".to_string(),
        "/usr/share/pixmaps".to_string(),
    ];
    
    let sizes = vec!["32x32", "48x48", "64x64", "128x128", "256x256", "16x16", "24x24", "22x22", "512x512"];
    let extensions = vec!["png", "svg", "xpm"];
    
    for ext in &extensions {
        // Check pixmaps
        let p = PathBuf::from(format!("/usr/share/pixmaps/{}.{}", icon_name, ext));
        if p.exists() { return Some(p); }
        
        for base in &base_dirs {
            // scalable
            let p = PathBuf::from(format!("{}/scalable/apps/{}.{}", base, icon_name, ext));
            if p.exists() { return Some(p); }
            
            // sized
            for size in &sizes {
                let p = PathBuf::from(format!("{}/{}/apps/{}.{}", base, size, icon_name, ext));
                if p.exists() { return Some(p); }
            }
        }
    }
    None
}