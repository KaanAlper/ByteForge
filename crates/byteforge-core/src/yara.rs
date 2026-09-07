//! Yara imza taraması — native Rust (yara-x, VirusTotal'ın resmi Rust yeniden
//! yazımı). Bir ikilide bilinen kalıpları arar: packer, kripto sabitleri,
//! anti-debug/anti-tamper, gömülü sertifika vb. Mevcut string-tabanlı
//! packer/anti-tamper tespitinin üstüne gerçek bir kural motoru koyar.

use serde::Serialize;

/// Tek bir Yara eşleşmesi.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct YaraMatch {
    /// Kural adı.
    pub rule: String,
    /// Kategori/etiketler (tags).
    pub tags: Vec<String>,
    /// Açıklama (meta: description).
    pub description: String,
    /// Kaç kalıp eşleşti.
    pub hit_count: usize,
}

/// Gömülü, elle seçilmiş kural seti. Genişletilebilir; kullanıcı kuralları
/// sonraki iterasyonda eklenebilir.
const BUILTIN_RULES: &str = r#"
rule UPX_Packer : packer {
    meta:
        description = "UPX ile paketlenmiş — sıkıştırılmış/korumalı"
    strings:
        $a = "UPX!"
        $b = "UPX0"
        $c = "UPX1"
    condition:
        2 of them
}

rule AES_Crypto : crypto {
    meta:
        description = "AES S-box sabiti — gömülü şifreleme"
    strings:
        $sbox = { 63 7c 77 7b f2 6b 6f c5 30 01 67 2b fe d7 ab 76 }
    condition:
        $sbox
}

rule Ptrace_AntiDebug : anti_debug {
    meta:
        description = "ptrace anti-debug — hata ayıklamayı engelleme"
    strings:
        $p = "ptrace"
        $t = "TracerPid"
    condition:
        any of them
}

rule Root_Detection : anti_tamper {
    meta:
        description = "Root tespiti — su/Magisk/busybox aranıyor"
    strings:
        $a = "/system/bin/su"
        $b = "/system/xbin/su"
        $c = "magisk"
        $d = "busybox"
        $e = "Superuser.apk"
    condition:
        2 of them
}

rule Frida_Detection : anti_tamper {
    meta:
        description = "Frida/enstrümantasyon tespiti"
    strings:
        $a = "frida"
        $b = "gum-js-loop"
        $c = "gadget"
        $d = "LIBFRIDA"
    condition:
        2 of them
}

rule Emulator_Detection : anti_emulator {
    meta:
        description = "Emülatör tespiti — Genymotion/QEMU/goldfish"
    strings:
        $a = "goldfish"
        $b = "ranchu"
        $c = "vbox86"
        $d = "genymotion"
        $e = "qemu"
    condition:
        2 of them
}

rule Embedded_URL_HTTP : network {
    meta:
        description = "Gömülü HTTP(S) uç noktaları"
    strings:
        $h = "https://" nocase
    condition:
        #h > 3
}

rule Debug_Symbols_Present : info {
    meta:
        description = "Debug/log izleri (loglama açık)"
    strings:
        $a = "__android_log_print"
        $b = "JNI_OnLoad"
    condition:
        any of them
}
"#;

/// Verilen baytları gömülü kural setiyle tarar ve eşleşmeleri döndürür.
pub fn scan(data: &[u8]) -> Result<Vec<YaraMatch>, String> {
    let mut compiler = yara_x::Compiler::new();
    compiler
        .add_source(BUILTIN_RULES)
        .map_err(|e| format!("kural derleme hatası: {e}"))?;
    let rules = compiler.build();
    let mut scanner = yara_x::Scanner::new(&rules);
    let results = scanner
        .scan(data)
        .map_err(|e| format!("tarama hatası: {e}"))?;

    let mut out = Vec::new();
    for r in results.matching_rules() {
        let hit_count: usize = r.patterns().map(|p| p.matches().count()).sum();
        let description = r
            .metadata()
            .into_iter()
            .find(|(k, _)| *k == "description")
            .and_then(|(_, v)| match v {
                yara_x::MetaValue::String(s) => Some(s.to_string()),
                _ => None,
            })
            .unwrap_or_default();
        out.push(YaraMatch {
            rule: r.identifier().to_string(),
            tags: r.tags().map(|t| t.identifier().to_string()).collect(),
            description,
            hit_count,
        });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rules_compile_and_scan_clean() {
        // Boş/temiz veri → hata yok, eşleşme yok.
        let m = scan(b"hello world, nothing suspicious here").unwrap();
        assert!(m.is_empty());
    }

    #[test]
    fn detects_ptrace() {
        let data = b"....ptrace....TracerPid....";
        let m = scan(data).unwrap();
        assert!(m.iter().any(|x| x.rule == "Ptrace_AntiDebug"));
    }

    #[test]
    fn detects_upx() {
        let data = b"stuff UPX! more UPX0 and UPX1 here";
        let m = scan(data).unwrap();
        let upx = m.iter().find(|x| x.rule == "UPX_Packer").expect("UPX bulunmalı");
        assert!(upx.tags.contains(&"packer".to_string()));
        assert!(!upx.description.is_empty());
    }
}
