//! Shannon entropisi — sıkıştırılmış/şifreli/paketlenmiş veri göstergesi.
//!
//! Bayt başına entropi 0 (tek değer) ile 8 (tam rastgele) arasındadır.
//! ~7.2+ bit/bayt genellikle sıkıştırma/şifreleme/packing işaretidir; imza
//! tabanlı paketleyici tespitini (bilinmeyen packer'lar için) tamamlar.

use serde::Serialize;

/// Bir bayt dizisinin Shannon entropisi (bit/bayt, 0.0–8.0).
pub fn shannon_entropy(bytes: &[u8]) -> f64 {
    if bytes.is_empty() {
        return 0.0;
    }
    let mut counts = [0u64; 256];
    for &b in bytes {
        counts[b as usize] += 1;
    }
    let len = bytes.len() as f64;
    let mut entropy = 0.0f64;
    for &c in counts.iter() {
        if c == 0 {
            continue;
        }
        let p = c as f64 / len;
        entropy -= p * p.log2();
    }
    entropy
}

/// Entropiye göre insan-okur sınıflandırma.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EntropyClass {
    /// < 5.0 — düz/yapısal veri (kod, metin).
    Low,
    /// 5.0–7.2 — normal ikili (karışık kod/veri).
    Normal,
    /// > 7.2 — sıkıştırılmış/şifreli/paketlenmiş olası.
    High,
}

/// Entropi değerini sınıfa dönüştürür.
pub fn classify(entropy: f64) -> EntropyClass {
    if entropy > 7.2 {
        EntropyClass::High
    } else if entropy >= 5.0 {
        EntropyClass::Normal
    } else {
        EntropyClass::Low
    }
}

/// Entropi + sınıf birlikte (UI/analiz için).
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct EntropyInfo {
    /// bit/bayt (0.0–8.0), 3 ondalık yuvarlanmış.
    pub bits: f64,
    pub class: EntropyClass,
}

/// Bir bayt dizisini analiz eder.
pub fn analyze(bytes: &[u8]) -> EntropyInfo {
    let raw = shannon_entropy(bytes);
    EntropyInfo {
        bits: (raw * 1000.0).round() / 1000.0,
        class: classify(raw),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_is_zero() {
        assert_eq!(shannon_entropy(&[]), 0.0);
    }

    #[test]
    fn single_value_is_zero() {
        assert_eq!(shannon_entropy(&[0x41; 1000]), 0.0);
        assert_eq!(classify(0.0), EntropyClass::Low);
    }

    #[test]
    fn uniform_bytes_max_entropy() {
        // 0..=255 her biri bir kez → tam 8 bit/bayt.
        let all: Vec<u8> = (0..=255).collect();
        let e = shannon_entropy(&all);
        assert!((e - 8.0).abs() < 1e-9, "entropi {e} ≈ 8 olmalı");
        assert_eq!(classify(e), EntropyClass::High);
    }

    #[test]
    fn two_values_one_bit() {
        // yarı yarıya iki değer → 1 bit/bayt
        let mut v = vec![0u8; 500];
        v.extend(std::iter::repeat_n(0xFFu8, 500));
        let e = shannon_entropy(&v);
        assert!((e - 1.0).abs() < 1e-9);
        assert_eq!(classify(e), EntropyClass::Low);
    }

    #[test]
    fn analyze_rounds_and_classifies() {
        let info = analyze(&[0x41; 100]);
        assert_eq!(info.bits, 0.0);
        assert_eq!(info.class, EntropyClass::Low);
    }
}
