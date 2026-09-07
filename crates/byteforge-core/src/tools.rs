use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct ToolStatus {
    pub name: String,
    pub available: bool,
    /// Bulunduysa mutlak yol, yoksa kurulum ipucu.
    pub detail: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ToolReport {
    pub tools: Vec<ToolStatus>,
}

/// PATH üzerinde bir yürütülebilir arar (which mantığı).
pub fn find_executable(name: &str) -> Option<String> {
    let path_var = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path_var) {
        // Doğrudan ad (Unix; Windows'ta tam ad verilmişse).
        let direct = dir.join(name);
        if direct.is_file() {
            return Some(direct.to_string_lossy().to_string());
        }
        // Windows: uzantısız adları PATHEXT uzantılarıyla dene (adb.exe vb.).
        #[cfg(windows)]
        {
            let exts = std::env::var("PATHEXT").unwrap_or_else(|_| ".EXE;.BAT;.CMD".to_string());
            for ext in exts.split(';') {
                let ext = ext.trim();
                if ext.is_empty() {
                    continue;
                }
                let candidate = dir.join(format!("{name}{ext}"));
                if candidate.is_file() {
                    return Some(candidate.to_string_lossy().to_string());
                }
            }
        }
    }
    None
}

/// Faz 0/1 için gereken araçları denetler.
pub fn check_tools() -> ToolReport {
    let wanted = [
        ("adb", "Android platform-tools kurun (paket: android-tools)"),
        ("apktool", "AUR: yay -S android-apktool-bin"),
        ("apksigner", "AUR: yay -S android-sdk-build-tools + PATH"),
        ("zipalign", "AUR: yay -S android-sdk-build-tools + PATH"),
        ("jadx", "Java decompiler — sudo pacman -S jadx"),
        (
            "java",
            "JDK/JRE kurun (apktool/apksigner/jadx için gerekli)",
        ),
    ];
    let tools = wanted
        .iter()
        .map(|(name, hint)| match find_executable(name) {
            Some(path) => ToolStatus {
                name: name.to_string(),
                available: true,
                detail: path,
            },
            None => ToolStatus {
                name: name.to_string(),
                available: false,
                detail: hint.to_string(),
            },
        })
        .collect();
    ToolReport { tools }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_known_executable() {
        // `sh` her POSIX sisteminde PATH'te bulunur.
        assert!(find_executable("sh").is_some());
    }

    #[test]
    fn missing_executable_is_none() {
        assert!(find_executable("definitely-not-a-real-binary-xyz123").is_none());
    }

    #[test]
    fn report_lists_expected_tools() {
        let r = check_tools();
        let names: Vec<&str> = r.tools.iter().map(|t| t.name.as_str()).collect();
        assert!(names.contains(&"adb"));
        assert!(names.contains(&"apktool"));
        assert!(names.contains(&"apksigner"));
        assert!(names.contains(&"zipalign"));
    }
}
