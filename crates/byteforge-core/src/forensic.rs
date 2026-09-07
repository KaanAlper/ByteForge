//! Adli fark raporu — bir `ArchiveDiff`'i (orijinal vs modlanmış APK) insan-okur
//! bulgulara dönüştürür. Saf Rust, dosya-adı sınıflandırması.

use crate::diff::ArchiveDiff;
use serde::Serialize;

/// Tek bir adli bulgu.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Finding {
    /// Önem: "yüksek" / "orta" / "bilgi".
    pub severity: String,
    /// Kısa başlık (ne olduğu).
    pub title: String,
    /// Açıklama / çıkarım (modcunun ne yapmış olabileceği).
    pub detail: String,
    /// İlgili giriş adları.
    pub entries: Vec<String>,
    /// Özel kategori (UI vurgusu için): "mod_menu" | "" (genel).
    #[serde(default)]
    pub category: String,
}

fn is_dex(n: &str) -> bool {
    n.ends_with(".dex")
}
fn is_so(n: &str) -> bool {
    n.ends_with(".so")
}

/// Dosya adı bir mod-menü / loader / enjeksiyon kütüphanesini çağrıştırıyor mu?
/// (libmenu.so, libmod.so, libimgui, libfrida-gadget, libsubstrate...)
fn is_modmenu_lib(n: &str) -> bool {
    let base = n.rsplit('/').next().unwrap_or(n).to_lowercase();
    const KEYS: &[&str] = &[
        "menu",
        "mod",
        "imgui",
        "overlay",
        "cheat",
        "hack",
        "aimbot",
        "esp",
        "loader",
        "inject",
        "frida",
        "gadget",
        "substrate",
        "xposed",
        "hook",
        "trainer",
        "mennu",
        "modmenu",
    ];
    KEYS.iter().any(|k| base.contains(k))
}
fn is_signature(n: &str) -> bool {
    let up = n.to_uppercase();
    up.starts_with("META-INF/")
        && (up.ends_with(".RSA")
            || up.ends_with(".DSA")
            || up.ends_with(".EC")
            || up.ends_with(".SF")
            || up.ends_with("MANIFEST.MF"))
}
fn is_manifest(n: &str) -> bool {
    n == "AndroidManifest.xml"
}
fn is_arsc(n: &str) -> bool {
    n == "resources.arsc"
}
fn is_res(n: &str) -> bool {
    n.starts_with("res/")
}
fn is_asset(n: &str) -> bool {
    n.starts_with("assets/")
}

fn push_if_any(out: &mut Vec<Finding>, sev: &str, title: &str, detail: &str, entries: Vec<String>) {
    push_cat(out, sev, title, detail, entries, "");
}

fn push_cat(
    out: &mut Vec<Finding>,
    sev: &str,
    title: &str,
    detail: &str,
    entries: Vec<String>,
    category: &str,
) {
    if !entries.is_empty() {
        out.push(Finding {
            severity: sev.to_string(),
            title: title.to_string(),
            detail: detail.to_string(),
            entries,
            category: category.to_string(),
        });
    }
}

/// Bir `ArchiveDiff`'ten önem sırasına göre bulgular üretir.
pub fn forensic_report(diff: &ArchiveDiff) -> Vec<Finding> {
    let mut out: Vec<Finding> = Vec::new();

    // İmza değişikliği (yeniden imzalanmış = resmi olmayan build).
    let sig: Vec<String> = diff
        .modified
        .iter()
        .chain(&diff.added)
        .chain(&diff.removed)
        .filter(|n| is_signature(n))
        .cloned()
        .collect();
    push_if_any(
        &mut out,
        "yüksek",
        "İmza bloğu yeniden düzenlenmiş",
        "META-INF imza dosyaları değişmiş → paket yeniden imzalanmış (resmi Play sürümü değil).",
        dedup(sig),
    );

    // Dalvik bytecode (classes.dex) değişikliği.
    let dex: Vec<String> = diff
        .modified
        .iter()
        .filter(|n| is_dex(n))
        .cloned()
        .collect();
    push_if_any(
        &mut out,
        "yüksek",
        "Dalvik bytecode değiştirilmiş",
        "classes*.dex değişmiş → smali/Java düzeyinde kod yaması (mantık/kilit müdahalesi olası).",
        dex,
    );

    // Native kütüphane değişikliği.
    let so_mod: Vec<String> = diff.modified.iter().filter(|n| is_so(n)).cloned().collect();
    push_if_any(
        &mut out,
        "yüksek",
        "Native kütüphane değiştirilmiş",
        "Bir .so değişmiş → opcode/IL2CPP yaması (ör. Return True enjeksiyonu) olası.",
        so_mod,
    );

    // Yeni eklenen native kütüphaneler — mod-menü/loader olanları ayır.
    let so_add: Vec<String> = diff.added.iter().filter(|n| is_so(n)).cloned().collect();
    let (menu_so, other_so): (Vec<String>, Vec<String>) =
        so_add.into_iter().partition(|n| is_modmenu_lib(n));
    // Ekstra eklenen DEX (mod menü kodu enjeksiyonu — ör. classes12.dex).
    let dex_add: Vec<String> = diff.added.iter().filter(|n| is_dex(n)).cloned().collect();

    // AKILLI TEŞHİS: mod menü .so'su ve/veya ekstra DEX → tek birleşik kart.
    if !menu_so.is_empty() || !dex_add.is_empty() {
        let mut entries = menu_so.clone();
        entries.extend(dex_add.clone());
        let detail = if !menu_so.is_empty() && !dex_add.is_empty() {
            "Yeni mod-menü/loader .so'su + ekstra DEX birlikte eklenmiş → klasik \
             MOD MENÜSÜ enjeksiyonu: loader .so ekrana çizim/hook yapar, ekstra DEX \
             menü kodunu taşır. Genellikle uygulama sınıfı (Application) değiştirilerek \
             başlatılır."
                .to_string()
        } else if !menu_so.is_empty() {
            "Mod-menü/loader çağrıştıran yeni bir .so eklenmiş (ör. libmenu.so) → \
             oyun içi overlay menü veya hook/enjeksiyon yükleyicisi olası."
                .to_string()
        } else {
            "Pakete ekstra DEX eklenmiş (ör. classes12.dex) → orijinalde olmayan kod \
             enjekte edilmiş (mod menü mantığı / loader). Manifest'te değişen Application \
             sınıfı bu DEX'i yüklüyor olabilir."
                .to_string()
        };
        push_cat(
            &mut out,
            "yüksek",
            "Mod Menüsü / Loader Enjeksiyonu",
            &detail,
            dedup(entries),
            "mod_menu",
        );
    }

    // Diğer yeni .so'lar (mod-menü kalıbına uymayan) — genel enjeksiyon uyarısı.
    push_if_any(
        &mut out,
        "yüksek",
        "Yeni native kütüphane eklenmiş",
        "Pakete yeni .so eklenmiş → yükleyici veya enjeksiyon olası.",
        other_so,
    );

    // Manifest değişikliği.
    let man: Vec<String> = diff
        .modified
        .iter()
        .filter(|n| is_manifest(n))
        .cloned()
        .collect();
    push_if_any(
        &mut out,
        "orta",
        "AndroidManifest değiştirilmiş",
        "Manifest değişmiş → izin ekleme, debuggable/allowBackup açma veya bileşen değişikliği olası.",
        man,
    );

    // Kaynak/asset değişikliği (düşük önem).
    let res: Vec<String> = diff
        .modified
        .iter()
        .chain(&diff.added)
        .filter(|n| is_arsc(n) || is_res(n))
        .cloned()
        .collect();
    push_if_any(
        &mut out,
        "bilgi",
        "Kaynaklar değiştirilmiş",
        "resources.arsc / res/ değişmiş → metin, ikon veya yapılandırma düzenlemesi.",
        dedup(res),
    );

    let asset: Vec<String> = diff
        .modified
        .iter()
        .chain(&diff.added)
        .filter(|n| is_asset(n))
        .cloned()
        .collect();
    push_if_any(
        &mut out,
        "bilgi",
        "Asset'ler değiştirilmiş",
        "assets/ değişmiş → gömülü veri/yapılandırma (ör. sunucu URL'si, bayrak dosyası).",
        dedup(asset),
    );

    // Silinen girişler (imza dışında).
    let removed: Vec<String> = diff
        .removed
        .iter()
        .filter(|n| !is_signature(n))
        .cloned()
        .collect();
    push_if_any(
        &mut out,
        "bilgi",
        "Silinen girişler",
        "Orijinalde olup modlanmışta olmayan girişler (reklam/analitik temizliği olası).",
        removed,
    );

    out
}

fn dedup(mut v: Vec<String>) -> Vec<String> {
    v.sort();
    v.dedup();
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    fn owned(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn flags_resign_and_dex_and_so() {
        let diff = ArchiveDiff {
            sha256_a: "a".into(),
            sha256_b: "b".into(),
            added: owned(&["lib/arm64-v8a/libcustom.so"]),
            removed: owned(&["META-INF/CERT.RSA"]),
            modified: owned(&[
                "classes.dex",
                "lib/arm64-v8a/libil2cpp.so",
                "AndroidManifest.xml",
                "META-INF/CERT.SF",
            ]),
            unchanged: 100,
        };
        let r = forensic_report(&diff);
        let titles: Vec<&str> = r.iter().map(|f| f.title.as_str()).collect();
        assert!(titles.contains(&"İmza bloğu yeniden düzenlenmiş"));
        assert!(titles.contains(&"Dalvik bytecode değiştirilmiş"));
        assert!(titles.contains(&"Native kütüphane değiştirilmiş"));
        // libcustom.so mod-menü kalıbına uymaz → genel "yeni .so" kartı.
        assert!(titles.contains(&"Yeni native kütüphane eklenmiş"));
        assert!(titles.contains(&"AndroidManifest değiştirilmiş"));
        // Yüksek önemli bulgular başta olmalı.
        assert_eq!(r[0].severity, "yüksek");
    }

    #[test]
    fn detects_mod_menu_so_and_extra_dex() {
        // bitlifemodzapk kalıbı: libmenu.so + classes12.dex enjekte.
        let diff = ArchiveDiff {
            added: owned(&["lib/arm64-v8a/libmenu.so", "classes12.dex"]),
            modified: owned(&["AndroidManifest.xml"]),
            ..Default::default()
        };
        let r = forensic_report(&diff);
        let card = r
            .iter()
            .find(|f| f.category == "mod_menu")
            .expect("mod_menu kartı olmalı");
        assert_eq!(card.title, "Mod Menüsü / Loader Enjeksiyonu");
        assert_eq!(card.severity, "yüksek");
        assert!(card
            .entries
            .contains(&"lib/arm64-v8a/libmenu.so".to_string()));
        assert!(card.entries.contains(&"classes12.dex".to_string()));
        // Mod-menü .so'su genel "yeni native kütüphane" kartına düşmemeli.
        assert!(!r
            .iter()
            .any(|f| f.title == "Yeni native kütüphane eklenmiş"));
    }

    #[test]
    fn extra_dex_only_is_mod_menu() {
        let diff = ArchiveDiff {
            added: owned(&["classes12.dex"]),
            ..Default::default()
        };
        let r = forensic_report(&diff);
        assert!(r.iter().any(|f| f.category == "mod_menu"));
    }

    #[test]
    fn severity_ordering_high_before_info() {
        let diff = ArchiveDiff {
            modified: owned(&["classes.dex", "res/values/strings.xml"]),
            ..Default::default()
        };
        let r = forensic_report(&diff);
        let high = r.iter().position(|f| f.severity == "yüksek").unwrap();
        let info = r.iter().position(|f| f.severity == "bilgi").unwrap();
        assert!(high < info);
    }

    #[test]
    fn identical_archive_no_findings() {
        let diff = ArchiveDiff {
            sha256_a: "x".into(),
            sha256_b: "x".into(),
            unchanged: 500,
            ..Default::default()
        };
        assert!(forensic_report(&diff).is_empty());
    }

    #[test]
    fn signature_detection_case_insensitive() {
        assert!(is_signature("META-INF/CERT.RSA"));
        assert!(is_signature("META-INF/cert.sf"));
        assert!(is_signature("META-INF/MANIFEST.MF"));
        assert!(!is_signature("META-INF/services/foo"));
        assert!(!is_signature("classes.dex"));
    }
}
