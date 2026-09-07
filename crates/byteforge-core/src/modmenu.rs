//! Yüzen Mod Menüsü enjektörü — saf Rust çekirdek.
//!
//! Bir APK'ya sıfırdan hile menüsü enjekte etme hattının bileşenleri:
//! yapılandırma modeli, manifest izni ekleme, launcher activity'nin onCreate'ine
//! kanca ekleme ve menü loader smali şablonu üretimi.
//!
//! NOT (iskelet): üretilen loader şu an minimal — enjeksiyonun uçtan uca
//! çalıştığını kanıtlayan bir Toast gösterir. Tam yüzen/sürüklenebilir overlay
//! UI'si (WindowManager + toggle'lar) cihazda iterasyon gerektirir ve bu
//! şablonun genişletileceği yerdir (aşağıda `menu_loader_smali` içinde işaretli).

use crate::{CoreError, Result};
use serde::{Deserialize, Serialize};

/// Bir menü özelliğinin uygulanacağı hedef.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum FeatureTarget {
    /// Smali boolean getter'ı zorla (statik yama, yeniden derlemede uygulanır).
    SmaliBoolean {
        /// Sınıf smali imzası, ör. "Lcom/app/Billing;".
        class: String,
        /// Metod imzası, ör. "isPremium()Z".
        method: String,
        /// true → return true, false → return false.
        ret: bool,
    },
    /// IL2CPP RVA ofseti (libil2cpp.so'da native yama — Native sekmesinde uygulanır).
    Il2cppRva {
        offset: u64,
        /// Şablon kimliği: "return_true" | "return_false" | ...
        template: String,
    },
    /// Doğrudan native ofset (herhangi bir .so).
    NativeOffset { offset: u64, template: String },
}

/// Menüye eklenecek tek bir özellik (toggle).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Feature {
    /// Kullanıcıya görünen ad, ör. "VIP / Kilitleri Aç".
    pub name: String,
    pub target: FeatureTarget,
}

/// Enjekte edilecek mod menüsünün tam yapılandırması.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MenuConfig {
    /// Menü başlığı, ör. "Android Koç v1.0".
    pub title: String,
    pub features: Vec<Feature>,
}

/// Loader sınıfının smali tipi imzası.
pub const MENU_LOADER_CLASS: &str = "Lcom/byteforge/modmenu/MenuLoader;";

/// Menüyü başlatan tek satırlık kanca (launcher onCreate'ine eklenir).
/// `p0` = Activity (this).
pub const HOOK_LINE: &str =
    "    invoke-static {p0}, Lcom/byteforge/modmenu/MenuLoader;->init(Landroid/app/Activity;)V";

/// Menünün gerektirdiği izin.
pub const OVERLAY_PERMISSION: &str = "android.permission.SYSTEM_ALERT_WINDOW";

/// Metni smali string literal için kaçışlar (tırnak, ters bölü, yeni satır).
fn escape_smali_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => {}
            c => out.push(c),
        }
    }
    out
}

/// Verilen yapılandırma için MenuLoader smali kaynağını üretir.
///
/// İSKELET: init(Activity) çağrıldığında menü başlığını ve özellik sayısını
/// bir Toast ile gösterir (enjeksiyonun çalıştığını kanıtlar). Gerçek yüzen
/// overlay UI'si (WindowManager.addView + sürükleme + toggle'lar) bu metodun
/// genişletileceği yerdir; toggle'ların runtime etkisi ayrı bir hook loader
/// (.so) veya bellek yamalayıcı gerektirir (statik özellikler zaten yeniden
/// derlemede uygulanır).
pub fn menu_loader_smali(config: &MenuConfig) -> String {
    let banner = escape_smali_string(&format!(
        "APKForge ModMenu: {} ({} ozellik) yuklendi",
        config.title,
        config.features.len()
    ));
    format!(
        r#".class public Lcom/byteforge/modmenu/MenuLoader;
.super Ljava/lang/Object;
.source "MenuLoader.java"

# APKForge tarafından enjekte edilen yüzen mod menüsü yükleyicisi (iskelet).
# init() launcher activity'nin onCreate'inden çağrılır.

.method public static init(Landroid/app/Activity;)V
    .locals 3
    .param p0, "activity"    # Landroid/app/Activity;

    # --- İSKELET: menü yüklendi bildirimi (Toast) ---
    # TODO: burada WindowManager ile yüzen sürüklenebilir overlay + toggle'lar
    #       oluşturulacak. Şimdilik enjeksiyonun çalıştığını doğrular.
    const-string v0, "{banner}"

    const/4 v1, 0x1

    invoke-static {{p0, v0, v1}}, Landroid/widget/Toast;->makeText(Landroid/content/Context;Ljava/lang/CharSequence;I)Landroid/widget/Toast;

    move-result-object v2

    invoke-virtual {{v2}}, Landroid/widget/Toast;->show()V

    return-void
.end method
"#
    )
}

/// AndroidManifest.xml (apktool metin biçimi) içine bir izni ekler (yoksa).
/// İdempotent: izin zaten varsa metni değiştirmeden döndürür.
pub fn ensure_permission(manifest_xml: &str, permission: &str) -> Result<String> {
    if manifest_xml.contains(permission) {
        return Ok(manifest_xml.to_string());
    }
    // <manifest ...> açılış etiketinin kapanış '>'ini bul.
    let mpos = manifest_xml.find("<manifest").ok_or_else(|| {
        CoreError::SmaliPatch("AndroidManifest.xml'de <manifest> etiketi bulunamadı".into())
    })?;
    let close = manifest_xml[mpos..]
        .find('>')
        .ok_or_else(|| CoreError::SmaliPatch("<manifest> etiketi kapatılmamış".into()))?
        + mpos;
    let insert_at = close + 1;
    let line = format!("\n    <uses-permission android:name=\"{permission}\"/>");
    let mut out = String::with_capacity(manifest_xml.len() + line.len());
    out.push_str(&manifest_xml[..insert_at]);
    out.push_str(&line);
    out.push_str(&manifest_xml[insert_at..]);
    Ok(out)
}

/// Bir activity smali'sinin onCreate(Bundle) metoduna, süper çağrısından sonra
/// tek satırlık kancayı ekler. onCreate yoksa hata; invoke-super bulunamazsa
/// prolog (.locals/.registers) satırından sonra ekler.
pub fn insert_oncreate_hook(smali: &str, hook_line: &str) -> Result<String> {
    let lines: Vec<&str> = smali.lines().collect();
    // onCreate(Landroid/os/Bundle;)V metodunun başını bul.
    let start = lines
        .iter()
        .position(|l| {
            let t = l.trim_start();
            t.starts_with(".method") && t.contains("onCreate(Landroid/os/Bundle;)V")
        })
        .ok_or_else(|| {
            CoreError::SmaliPatch("launcher activity'de onCreate(Bundle) bulunamadı".into())
        })?;
    let end = (start + 1..lines.len())
        .find(|&j| lines[j].trim_start().starts_with(".end method"))
        .ok_or_else(|| CoreError::SmaliPatch("onCreate .end method bulunamadı".into()))?;

    // Ekleme noktası: invoke-super ...->onCreate( satırından sonra; yoksa prolog.
    let super_pos = (start + 1..end).find(|&j| {
        let t = lines[j].trim_start();
        t.starts_with("invoke-super") && t.contains("->onCreate(")
    });
    let prologue_pos = (start + 1..end).find(|&j| {
        let t = lines[j].trim_start();
        t.starts_with(".locals") || t.starts_with(".registers")
    });
    let insert_after = super_pos
        .or(prologue_pos)
        .ok_or_else(|| CoreError::SmaliPatch("onCreate içinde ekleme noktası bulunamadı".into()))?;

    let mut out: Vec<String> = Vec::with_capacity(lines.len() + 2);
    for (i, l) in lines.iter().enumerate() {
        out.push((*l).to_string());
        if i == insert_after {
            out.push(String::new());
            out.push(hook_line.to_string());
        }
    }
    Ok(out.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_config() -> MenuConfig {
        MenuConfig {
            title: "Android Koç v1.0".into(),
            features: vec![
                Feature {
                    name: "VIP / Kilitleri Aç".into(),
                    target: FeatureTarget::SmaliBoolean {
                        class: "Lcom/app/Billing;".into(),
                        method: "isPremium()Z".into(),
                        ret: true,
                    },
                },
                Feature {
                    name: "Sonsuz Para".into(),
                    target: FeatureTarget::Il2cppRva {
                        offset: 0x1234,
                        template: "return_max_int".into(),
                    },
                },
            ],
        }
    }

    #[test]
    fn config_json_roundtrip() {
        let c = sample_config();
        let json = serde_json::to_string(&c).unwrap();
        let back: MenuConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(c, back);
    }

    #[test]
    fn loader_smali_has_class_and_init_and_title() {
        let s = menu_loader_smali(&sample_config());
        assert!(s.contains(".class public Lcom/byteforge/modmenu/MenuLoader;"));
        assert!(s.contains("init(Landroid/app/Activity;)V"));
        assert!(s.contains("2 ozellik"));
        assert!(s.contains("Android Koç v1.0"));
        // Toast çağrısı üretilmiş olmalı.
        assert!(s.contains("Landroid/widget/Toast;->makeText"));
    }

    #[test]
    fn loader_smali_escapes_quotes() {
        let cfg = MenuConfig {
            title: "He\"llo".into(),
            features: vec![],
        };
        let s = menu_loader_smali(&cfg);
        assert!(s.contains("He\\\"llo"));
    }

    #[test]
    fn ensure_permission_adds_once() {
        let m = r#"<?xml version="1.0"?>
<manifest xmlns:android="http://schemas.android.com/apk/res/android" package="com.x">
    <application android:label="X"></application>
</manifest>"#;
        let out = ensure_permission(m, OVERLAY_PERMISSION).unwrap();
        assert!(out.contains("android.permission.SYSTEM_ALERT_WINDOW"));
        assert_eq!(out.matches("SYSTEM_ALERT_WINDOW").count(), 1);
        // İkinci kez idempotent.
        let out2 = ensure_permission(&out, OVERLAY_PERMISSION).unwrap();
        assert_eq!(out2.matches("SYSTEM_ALERT_WINDOW").count(), 1);
    }

    #[test]
    fn insert_hook_after_super_oncreate() {
        let smali = r#".class public Lcom/app/MainActivity;
.super Landroid/app/Activity;

.method public onCreate(Landroid/os/Bundle;)V
    .locals 1
    invoke-super {p0, p1}, Landroid/app/Activity;->onCreate(Landroid/os/Bundle;)V
    const/4 v0, 0x0
    return-void
.end method
"#;
        let out = insert_oncreate_hook(smali, HOOK_LINE).unwrap();
        assert!(out.contains("MenuLoader;->init(Landroid/app/Activity;)V"));
        // Kanca super çağrısından SONRA gelmeli.
        let sup = out.find("->onCreate(Landroid/os/Bundle;)V\n").unwrap();
        let hook = out.find("MenuLoader;->init").unwrap();
        assert!(hook > sup);
    }

    #[test]
    fn insert_hook_errors_without_oncreate() {
        let smali = ".class public Lcom/app/X;\n.super Ljava/lang/Object;\n";
        assert!(insert_oncreate_hook(smali, HOOK_LINE).is_err());
    }
}
