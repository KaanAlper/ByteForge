//! String çözücü (deobfuscator) — saf Rust, harici bağımlılık yok.
//!
//! Tersine mühendislikte sık karşılaşılan basit gizleme yöntemlerini dener:
//! Base64, onaltılık (hex), tek-bayt XOR (kaba kuvvet) ve ROT13. Yazdırılabilir
//! ASCII üreten adayları puanıyla döndürür.

use serde::Serialize;

/// Çözülmüş bir aday.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Candidate {
    /// Yöntem: "base64" / "hex" / "xor" / "rot13".
    pub method: String,
    /// XOR anahtarı (yalnızca xor için, 0xNN).
    pub key: Option<u8>,
    /// Çözülmüş metin.
    pub text: String,
    /// Yazdırılabilirlik puanı (0-100).
    pub score: u8,
}

/// Bir baytın "okunabilir" (yazdırılabilir ASCII veya boşluk) olup olmadığı.
fn is_readable(b: u8) -> bool {
    b == b'\t' || b == b'\n' || b == b'\r' || (0x20..=0x7E).contains(&b)
}

/// Bayt dizisinin yazdırılabilirlik oranı (0-100).
fn printable_score(bytes: &[u8]) -> u8 {
    if bytes.is_empty() {
        return 0;
    }
    let ok = bytes.iter().filter(|&&b| is_readable(b)).count();
    ((ok * 100) / bytes.len()) as u8
}

/// Standart Base64 çözer (padding'li/padsız). Geçersizse None.
fn base64_decode(s: &str) -> Option<Vec<u8>> {
    fn val(c: u8) -> Option<u8> {
        match c {
            b'A'..=b'Z' => Some(c - b'A'),
            b'a'..=b'z' => Some(c - b'a' + 26),
            b'0'..=b'9' => Some(c - b'0' + 52),
            b'+' => Some(62),
            b'/' => Some(63),
            _ => None,
        }
    }
    let clean: Vec<u8> = s
        .bytes()
        .filter(|&c| c != b'=' && !c.is_ascii_whitespace())
        .collect();
    if clean.len() < 4 {
        return None;
    }
    let mut out = Vec::with_capacity(clean.len() * 3 / 4);
    let mut acc: u32 = 0;
    let mut bits = 0u32;
    for &c in &clean {
        let v = val(c)? as u32;
        acc = (acc << 6) | v;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
        }
    }
    Some(out)
}

/// Onaltılık dize çözer (boşlukları yok sayar). Geçersizse None.
fn hex_decode(s: &str) -> Option<Vec<u8>> {
    let clean: Vec<u8> = s.bytes().filter(|c| !c.is_ascii_whitespace()).collect();
    if clean.len() < 2 || !clean.len().is_multiple_of(2) {
        return None;
    }
    let mut out = Vec::with_capacity(clean.len() / 2);
    let mut i = 0;
    while i < clean.len() {
        let hi = (clean[i] as char).to_digit(16)?;
        let lo = (clean[i + 1] as char).to_digit(16)?;
        out.push((hi * 16 + lo) as u8);
        i += 2;
    }
    Some(out)
}

fn rot13(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            'A'..='Z' => (((c as u8 - b'A' + 13) % 26) + b'A') as char,
            'a'..='z' => (((c as u8 - b'a' + 13) % 26) + b'a') as char,
            _ => c,
        })
        .collect()
}

/// Yazdırılabilir bir metin oluşturur (okunamayan baytları '.' yapar).
fn to_text(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|&b| if is_readable(b) { b as char } else { '.' })
        .collect()
}

const MIN_SCORE: u8 = 80;

/// Harf + boşluk oranı (0-100) — "kelime benzeri" metni ölçmek için.
/// XOR kaba kuvvetinde sembol gürültüsünü gerçek metinden ayırır.
fn wordiness(bytes: &[u8]) -> u8 {
    if bytes.is_empty() {
        return 0;
    }
    let good = bytes
        .iter()
        .filter(|&&b| b.is_ascii_alphabetic() || b == b' ')
        .count();
    ((good * 100) / bytes.len()) as u8
}

/// Girdiyi çeşitli yöntemlerle çözmeyi dener. Deterministik yöntemler
/// (base64/hex/rot13) her zaman önce; XOR adayları kelime-benzerliği oranına
/// göre elenip sıralanır (kısa girdide sahte pozitif taşmasını önler).
pub fn deobfuscate(input: &str) -> Vec<Candidate> {
    let trimmed = input.trim();
    let mut det: Vec<Candidate> = Vec::new();

    // Base64
    if let Some(bytes) = base64_decode(trimmed) {
        let score = printable_score(&bytes);
        if score >= MIN_SCORE {
            det.push(Candidate {
                method: "base64".into(),
                key: None,
                text: to_text(&bytes),
                score,
            });
        }
    }

    // Hex
    if let Some(bytes) = hex_decode(trimmed) {
        let score = printable_score(&bytes);
        if score >= MIN_SCORE {
            det.push(Candidate {
                method: "hex".into(),
                key: None,
                text: to_text(&bytes),
                score,
            });
        }
    }

    // ROT13 (yalnızca harf içeriyorsa anlamlı).
    if trimmed.chars().any(|c| c.is_ascii_alphabetic()) {
        let r = rot13(trimmed);
        if r != trimmed {
            det.push(Candidate {
                method: "rot13".into(),
                key: None,
                text: r,
                score: 100,
            });
        }
    }

    // Tek-bayt XOR kaba kuvvet — kelime-benzerliği >= 75 olanlar (sembol
    // gürültüsü elenir), en iyi 16 anahtar.
    let raw = trimmed.as_bytes();
    let mut xors: Vec<Candidate> = Vec::new();
    if !raw.is_empty() {
        for key in 1u8..=255 {
            let xored: Vec<u8> = raw.iter().map(|&b| b ^ key).collect();
            if printable_score(&xored) < 95 {
                continue;
            }
            let w = wordiness(&xored);
            if w >= 75 {
                xors.push(Candidate {
                    method: "xor".into(),
                    key: Some(key),
                    text: to_text(&xored),
                    score: w,
                });
            }
        }
        xors.sort_by_key(|c| std::cmp::Reverse(c.score));
        xors.truncate(16);
    }

    det.extend(xors);
    det
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_base64() {
        // "Hello, World!" base64 = SGVsbG8sIFdvcmxkIQ==
        let c = deobfuscate("SGVsbG8sIFdvcmxkIQ==");
        assert!(c
            .iter()
            .any(|x| x.method == "base64" && x.text.contains("Hello, World!")));
    }

    #[test]
    fn decodes_hex() {
        // "secret" hex = 736563726574
        let c = deobfuscate("736563726574");
        assert!(c.iter().any(|x| x.method == "hex" && x.text == "secret"));
    }

    #[test]
    fn xor_brute_recovers_key() {
        // Gerçekçi uzunlukta düz metin → yalnızca gerçek anahtar okunabilir çıkar.
        let key = 0x2A;
        let plain = "unlockAllPremiumFeatures";
        let enc: String = plain.bytes().map(|b| (b ^ key) as char).collect();
        let c = deobfuscate(&enc);
        assert!(
            c.iter()
                .any(|x| x.method == "xor" && x.key == Some(key) && x.text == plain),
            "beklenen XOR anahtarı {key:#x} adaylar arasında yok: {c:?}"
        );
    }

    #[test]
    fn rot13_roundtrip() {
        let c = deobfuscate("Uryyb");
        assert!(c.iter().any(|x| x.method == "rot13" && x.text == "Hello"));
    }

    #[test]
    fn garbage_yields_low_or_none_base64() {
        // Rastgele düz metin base64 gibi çözülse de okunamaz → düşük skorla elenir
        let c = deobfuscate("!!!***???");
        assert!(!c.iter().any(|x| x.method == "hex"));
    }
}
