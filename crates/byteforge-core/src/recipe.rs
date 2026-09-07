//! Yama tarifleri (patch recipes) — paylaşılabilir JSON içe/dışa aktarımı.
//!
//! Bir tarif, bir dosyaya uygulanacak baytsal yama adımlarının taşınabilir
//! kaydıdır. Mutlak yol yerine dosya adı (basename) taşır. İçe aktarımda her
//! adım hedef dosyaya karşı doğrulanabilir (mevcut baytlar `old_hex` ile eşleşiyor mu).

use crate::hexedit::parse_hex;
use crate::{CoreError, Result};
use serde::{Deserialize, Serialize};

/// Tarifteki tek bir yama adımı.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecipeStep {
    /// "so" | "hex" (kaynak yama türü).
    pub kind: String,
    /// Hedef dosya adı (basename — taşınabilirlik için, mutlak yol değil).
    pub target: String,
    pub offset: usize,
    pub old_hex: String,
    pub new_hex: String,
    #[serde(default)]
    pub note: String,
}

/// Paylaşılabilir bir yama tarifi.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Recipe {
    /// Biçim sürümü (ileri uyumluluk).
    pub version: u32,
    pub name: String,
    #[serde(default)]
    pub note: String,
    /// Hangi uygulamaya ait olduğu ipucu (paket adı / dosya adı).
    #[serde(default)]
    pub app_hint: String,
    pub steps: Vec<RecipeStep>,
}

/// Geçerli tarif biçim sürümü.
pub const RECIPE_VERSION: u32 = 1;

/// Bir tarifi biçimli JSON'a dönüştürür.
pub fn to_json(recipe: &Recipe) -> Result<String> {
    serde_json::to_string_pretty(recipe)
        .map_err(|e| CoreError::Hex(format!("tarif serileştirilemedi: {e}")))
}

/// JSON'dan bir tarif ayrıştırır (biçim/sürüm doğrulamasıyla).
pub fn parse_recipe(json: &str) -> Result<Recipe> {
    let recipe: Recipe = serde_json::from_str(json)
        .map_err(|e| CoreError::Hex(format!("geçersiz tarif JSON'u: {e}")))?;
    if recipe.version == 0 || recipe.version > RECIPE_VERSION {
        return Err(CoreError::Hex(format!(
            "desteklenmeyen tarif sürümü: {}",
            recipe.version
        )));
    }
    Ok(recipe)
}

/// Bir adımın bir dosyaya uygulanabilirlik durumu.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StepStatus {
    /// Mevcut baytlar `old_hex` ile eşleşiyor — güvenle uygulanabilir.
    Applicable,
    /// Baytlar zaten `new_hex` — yama önceden uygulanmış.
    AlreadyApplied,
    /// Mevcut baytlar ne eskiye ne yeniye uyuyor — dosya farklı/uyumsuz.
    Mismatch,
    /// Ofset dosya sınırının dışında.
    OutOfRange,
    /// Adımın hex alanları çözümlenemedi.
    BadStep,
}

/// Bir adımı dosya baytlarına karşı doğrular (yazma yapmaz).
pub fn verify_step(file_bytes: &[u8], step: &RecipeStep) -> StepStatus {
    let (Ok(old), Ok(new)) = (parse_hex(&step.old_hex), parse_hex(&step.new_hex)) else {
        return StepStatus::BadStep;
    };
    if old.is_empty() {
        return StepStatus::BadStep;
    }
    let end = match step.offset.checked_add(old.len()) {
        Some(e) if e <= file_bytes.len() => e,
        _ => return StepStatus::OutOfRange,
    };
    let cur = &file_bytes[step.offset..end];
    if cur == old.as_slice() {
        StepStatus::Applicable
    } else if new.len() == old.len() && cur == new.as_slice() {
        StepStatus::AlreadyApplied
    } else {
        StepStatus::Mismatch
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn step() -> RecipeStep {
        RecipeStep {
            kind: "so".into(),
            target: "libil2cpp.so".into(),
            offset: 4,
            old_hex: "00 00 80 52".into(),
            new_hex: "20 00 80 52".into(),
            note: "isPremium → true".into(),
        }
    }

    #[test]
    fn json_roundtrip() {
        let r = Recipe {
            version: RECIPE_VERSION,
            name: "BitLife premium".into(),
            note: "test".into(),
            app_hint: "com.candywriter.bitlife".into(),
            steps: vec![step()],
        };
        let json = to_json(&r).unwrap();
        let back = parse_recipe(&json).unwrap();
        assert_eq!(r, back);
    }

    #[test]
    fn rejects_future_version() {
        let json = r#"{"version":999,"name":"x","steps":[]}"#;
        assert!(parse_recipe(json).is_err());
    }

    #[test]
    fn verify_applicable() {
        // offset 4'te 00 00 80 52 var
        let bytes = vec![0xFF, 0xFF, 0xFF, 0xFF, 0x00, 0x00, 0x80, 0x52, 0xAA];
        assert_eq!(verify_step(&bytes, &step()), StepStatus::Applicable);
    }

    #[test]
    fn verify_already_applied() {
        // offset 4'te new_hex (20 00 80 52) var
        let bytes = vec![0xFF, 0xFF, 0xFF, 0xFF, 0x20, 0x00, 0x80, 0x52, 0xAA];
        assert_eq!(verify_step(&bytes, &step()), StepStatus::AlreadyApplied);
    }

    #[test]
    fn verify_mismatch_and_out_of_range() {
        let bytes = vec![0u8; 9]; // ne eski ne yeni
        assert_eq!(verify_step(&bytes, &step()), StepStatus::Mismatch);
        let short = vec![0u8; 5]; // offset 4 + 4 bayt > 5
        assert_eq!(verify_step(&short, &step()), StepStatus::OutOfRange);
    }
}
