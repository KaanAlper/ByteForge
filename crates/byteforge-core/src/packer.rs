//! Koruyucu / paketleyici (packer/protector) tespiti — saf Rust, imza tabanlı.
//!
//! Android koruyucuları genellikle karakteristik `.so` / asset dosyaları bırakır;
//! Windows PE paketleyicileri ise tanınabilir section adları kullanır. Bu modül
//! harici araç olmadan bu parmak izlerini eşleştirir.

use serde::Serialize;

/// Tespit edilen bir koruyucu/paketleyici.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PackerHit {
    /// Ürün adı (ör. "Tencent Legu").
    pub name: String,
    /// Sağlayıcı/menşe (ör. "Tencent", "VMProtect Software").
    pub vendor: String,
    /// Güven düzeyi: "kesin" (benzersiz imza) / "olası" (paylaşılan gösterge).
    pub confidence: String,
    /// Eşleşmeye yol açan kanıt (dosya/section adı).
    pub evidence: String,
}

struct Sig {
    name: &'static str,
    vendor: &'static str,
    /// Eşleşme kalıbı (alt-dizi, küçük harf karşılaştırılır).
    needle: &'static str,
    /// Benzersiz/güçlü imza mı? (kesin vs olası)
    strong: bool,
}

/// Android koruyucu parmak izleri (dosya/entry adına göre, küçük harf alt-dizi).
const ANDROID_SIGS: &[Sig] = &[
    // Bangcle / SecNeo
    Sig {
        name: "Bangcle",
        vendor: "Bangcle (SecNeo)",
        needle: "libsecexe.so",
        strong: true,
    },
    Sig {
        name: "Bangcle",
        vendor: "Bangcle (SecNeo)",
        needle: "libsecmain.so",
        strong: true,
    },
    Sig {
        name: "Bangcle",
        vendor: "Bangcle (SecNeo)",
        needle: "bangcle_classes.jar",
        strong: true,
    },
    Sig {
        name: "SecNeo",
        vendor: "Bangcle (SecNeo)",
        needle: "libdexhelper.so",
        strong: true,
    },
    Sig {
        name: "SecNeo",
        vendor: "Bangcle (SecNeo)",
        needle: "libshella-",
        strong: true,
    },
    // Tencent Legu
    Sig {
        name: "Tencent Legu",
        vendor: "Tencent",
        needle: "libshellx-",
        strong: true,
    },
    Sig {
        name: "Tencent Legu",
        vendor: "Tencent",
        needle: "libshell-super.",
        strong: true,
    },
    Sig {
        name: "Tencent Legu",
        vendor: "Tencent",
        needle: "libtup.so",
        strong: false,
    },
    Sig {
        name: "Tencent Legu",
        vendor: "Tencent",
        needle: "libexec.so",
        strong: false,
    },
    Sig {
        name: "Tencent Legu",
        vendor: "Tencent",
        needle: "tosversion",
        strong: true,
    },
    // Qihoo 360 Jiagu
    Sig {
        name: "Qihoo 360 Jiagu",
        vendor: "Qihoo 360",
        needle: "libjiagu.so",
        strong: true,
    },
    Sig {
        name: "Qihoo 360 Jiagu",
        vendor: "Qihoo 360",
        needle: "libjiagu_art.so",
        strong: true,
    },
    Sig {
        name: "Qihoo 360 Jiagu",
        vendor: "Qihoo 360",
        needle: "libjiagu_x86.so",
        strong: true,
    },
    Sig {
        name: "Qihoo 360 Jiagu",
        vendor: "Qihoo 360",
        needle: "libjiagu_a64.so",
        strong: true,
    },
    // Ijiami
    Sig {
        name: "Ijiami",
        vendor: "Ijiami",
        needle: "ijiami.dat",
        strong: true,
    },
    Sig {
        name: "Ijiami",
        vendor: "Ijiami",
        needle: "libexecmain.so",
        strong: true,
    },
    Sig {
        name: "Ijiami",
        vendor: "Ijiami",
        needle: "ijiami.ajm",
        strong: true,
    },
    // Baidu
    Sig {
        name: "Baidu Protect",
        vendor: "Baidu",
        needle: "libbaiduprotect.so",
        strong: true,
    },
    Sig {
        name: "Baidu Protect",
        vendor: "Baidu",
        needle: "baiduprotect",
        strong: false,
    },
    // NetEase / Nagain
    Sig {
        name: "NetEase NAGA",
        vendor: "NetEase",
        needle: "libnesec.so",
        strong: true,
    },
    // Alibaba (Mobile Security)
    Sig {
        name: "Alibaba Jaq",
        vendor: "Alibaba",
        needle: "libmobisec.so",
        strong: true,
    },
    Sig {
        name: "Alibaba Jaq",
        vendor: "Alibaba",
        needle: "libpreverify1.so",
        strong: true,
    },
    // DexProtector
    Sig {
        name: "DexProtector",
        vendor: "Licel",
        needle: "libdexprotector.so",
        strong: true,
    },
    Sig {
        name: "DexProtector",
        vendor: "Licel",
        needle: "dexprotector.",
        strong: false,
    },
    // AppSuit / Kony / Promon
    Sig {
        name: "Promon SHIELD",
        vendor: "Promon",
        needle: "libshield.so",
        strong: false,
    },
    Sig {
        name: "AppSuit",
        vendor: "STEALIEN",
        needle: "libappsuit.so",
        strong: true,
    },
    // Virbox
    Sig {
        name: "Virbox Protector",
        vendor: "Senseshield",
        needle: "libv7.so",
        strong: false,
    },
    Sig {
        name: "Virbox Protector",
        vendor: "Senseshield",
        needle: "libvbox.so",
        strong: true,
    },
    // Mobile Tencent / DexGuard (yeniden adlandırdığı için zayıf gösterge)
    Sig {
        name: "DexGuard (olası)",
        vendor: "Guardsquare",
        needle: "libpreverify.so",
        strong: false,
    },
];

/// Windows PE paketleyici parmak izleri (PE section adına göre, tam eşleşme küçük harf).
struct PeSig {
    name: &'static str,
    vendor: &'static str,
    section: &'static str,
    strong: bool,
}

const PE_SIGS: &[PeSig] = &[
    PeSig {
        name: "UPX",
        vendor: "UPX",
        section: "upx0",
        strong: true,
    },
    PeSig {
        name: "UPX",
        vendor: "UPX",
        section: "upx1",
        strong: true,
    },
    PeSig {
        name: "UPX",
        vendor: "UPX",
        section: "upx2",
        strong: false,
    },
    PeSig {
        name: "Themida / WinLicense",
        vendor: "Oreans",
        section: ".themida",
        strong: true,
    },
    PeSig {
        name: "Themida / WinLicense",
        vendor: "Oreans",
        section: ".winlice",
        strong: true,
    },
    PeSig {
        name: "VMProtect",
        vendor: "VMProtect Software",
        section: ".vmp0",
        strong: true,
    },
    PeSig {
        name: "VMProtect",
        vendor: "VMProtect Software",
        section: ".vmp1",
        strong: true,
    },
    PeSig {
        name: "VMProtect",
        vendor: "VMProtect Software",
        section: ".vmp2",
        strong: false,
    },
    PeSig {
        name: "ASPack",
        vendor: "StarForce",
        section: ".aspack",
        strong: true,
    },
    PeSig {
        name: "ASPack",
        vendor: "StarForce",
        section: ".adata",
        strong: false,
    },
    PeSig {
        name: "Enigma Protector",
        vendor: "Enigma",
        section: ".enigma1",
        strong: true,
    },
    PeSig {
        name: "Enigma Protector",
        vendor: "Enigma",
        section: ".enigma2",
        strong: true,
    },
    PeSig {
        name: "MPRESS",
        vendor: "MPRESS",
        section: ".mpress1",
        strong: true,
    },
    PeSig {
        name: "MPRESS",
        vendor: "MPRESS",
        section: ".mpress2",
        strong: true,
    },
    PeSig {
        name: "Petite",
        vendor: "Petite",
        section: ".petite",
        strong: true,
    },
    PeSig {
        name: "PECompact",
        vendor: "Bitsum",
        section: "pec1",
        strong: true,
    },
    PeSig {
        name: "PECompact",
        vendor: "Bitsum",
        section: "pec2",
        strong: true,
    },
    PeSig {
        name: "FSG",
        vendor: "FSG",
        section: "fsg!",
        strong: true,
    },
    PeSig {
        name: "NsPack",
        vendor: "NsPack",
        section: ".nsp0",
        strong: true,
    },
    PeSig {
        name: "Obsidium",
        vendor: "Obsidium",
        section: ".obsidiu",
        strong: true,
    },
];

fn confidence(strong: bool) -> String {
    if strong {
        "kesin".into()
    } else {
        "olası".into()
    }
}

/// APK entry adları listesinden Android koruyucularını tespit eder.
/// Aynı ürün birden çok kez eşleşse bile tek kayıt döner (en güçlü kanıtla).
pub fn detect_android_packers(entry_names: &[String]) -> Vec<PackerHit> {
    let lower: Vec<String> = entry_names.iter().map(|e| e.to_lowercase()).collect();
    let mut hits: Vec<PackerHit> = Vec::new();
    for sig in ANDROID_SIGS {
        if let Some(ev) = lower.iter().find(|e| e.contains(sig.needle)) {
            upsert(&mut hits, sig.name, sig.vendor, sig.strong, ev);
        }
    }
    hits
}

/// PE section adlarından Windows paketleyicilerini tespit eder.
pub fn detect_pe_packers(sections: &[String]) -> Vec<PackerHit> {
    let lower: Vec<String> = sections.iter().map(|s| s.to_lowercase()).collect();
    let mut hits: Vec<PackerHit> = Vec::new();
    for sig in PE_SIGS {
        if let Some(ev) = lower.iter().find(|s| s.as_str() == sig.section) {
            upsert(&mut hits, sig.name, sig.vendor, sig.strong, ev);
        }
    }
    hits
}

/// Aynı ürün adı için kaydı ekler ya da daha güçlü kanıtla günceller.
fn upsert(hits: &mut Vec<PackerHit>, name: &str, vendor: &str, strong: bool, evidence: &str) {
    if let Some(existing) = hits.iter_mut().find(|h| h.name == name) {
        // Zaten "kesin" ise dokunma; değilse ve yeni imza güçlüyse yükselt.
        if strong && existing.confidence != "kesin" {
            existing.confidence = confidence(true);
            existing.evidence = evidence.to_string();
        }
        return;
    }
    hits.push(PackerHit {
        name: name.to_string(),
        vendor: vendor.to_string(),
        confidence: confidence(strong),
        evidence: evidence.to_string(),
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn owned(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn detects_tencent_legu_strong() {
        let hits = detect_android_packers(&owned(&[
            "classes.dex",
            "lib/arm64-v8a/libshellx-super.2019.so",
            "assets/tosversion",
        ]));
        assert!(hits.iter().any(|h| h.name == "Tencent Legu"));
        let h = hits.iter().find(|h| h.name == "Tencent Legu").unwrap();
        assert_eq!(h.confidence, "kesin");
    }

    #[test]
    fn detects_jiagu_and_bangcle() {
        let hits = detect_android_packers(&owned(&[
            "lib/armeabi-v7a/libjiagu.so",
            "assets/bangcle_classes.jar",
        ]));
        assert!(hits.iter().any(|h| h.name == "Qihoo 360 Jiagu"));
        assert!(hits.iter().any(|h| h.name == "Bangcle"));
    }

    #[test]
    fn weak_only_signal_is_probable() {
        // Yalnızca libtup.so/libexec.so → Legu "olası"
        let hits = detect_android_packers(&owned(&["lib/arm64-v8a/libtup.so"]));
        let h = hits.iter().find(|h| h.name == "Tencent Legu").unwrap();
        assert_eq!(h.confidence, "olası");
    }

    #[test]
    fn clean_apk_has_no_packer() {
        let hits = detect_android_packers(&owned(&[
            "classes.dex",
            "AndroidManifest.xml",
            "lib/arm64-v8a/libunity.so",
            "assets/bin/Data/globalgamemanagers",
        ]));
        assert!(hits.is_empty());
    }

    #[test]
    fn detects_pe_vmprotect_and_upx() {
        let hits = detect_pe_packers(&owned(&[".text", ".vmp0", ".vmp1", ".rsrc"]));
        assert!(hits.iter().any(|h| h.name == "VMProtect"));
        let upx = detect_pe_packers(&owned(&["UPX0", "UPX1", ".rsrc"]));
        assert!(upx.iter().any(|h| h.name == "UPX"));
    }

    #[test]
    fn clean_pe_has_no_packer() {
        let hits = detect_pe_packers(&owned(&[".text", ".rdata", ".data", ".rsrc", ".reloc"]));
        assert!(hits.is_empty());
    }

    #[test]
    fn duplicate_product_deduped() {
        let hits = detect_android_packers(&owned(&[
            "lib/arm64-v8a/libjiagu.so",
            "lib/armeabi-v7a/libjiagu_art.so",
        ]));
        assert_eq!(
            hits.iter().filter(|h| h.name == "Qihoo 360 Jiagu").count(),
            1
        );
    }
}
