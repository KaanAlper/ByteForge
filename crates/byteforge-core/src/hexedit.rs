use crate::{CoreError, Result};

/// "1F 20 03 D5" / "1f2003d5" gibi bir hex dizesini baytlara çevirir.
pub fn parse_hex(input: &str) -> Result<Vec<u8>> {
    let cleaned: String = input.chars().filter(|c| !c.is_whitespace()).collect();
    if cleaned.is_empty() {
        return Err(CoreError::Hex("boş hex girdisi".into()));
    }
    if !cleaned.len().is_multiple_of(2) {
        return Err(CoreError::Hex("hex hane sayısı çift olmalı".into()));
    }
    (0..cleaned.len())
        .step_by(2)
        .map(|i| {
            u8::from_str_radix(&cleaned[i..i + 2], 16)
                .map_err(|_| CoreError::Hex(format!("geçersiz hex: {}", &cleaned[i..i + 2])))
        })
        .collect()
}

/// `haystack` içinde `needle`'ın geçtiği tüm başlangıç ofsetlerini döndürür.
pub fn search_bytes(haystack: &[u8], needle: &[u8]) -> Vec<usize> {
    if needle.is_empty() || needle.len() > haystack.len() {
        return Vec::new();
    }
    (0..=haystack.len() - needle.len())
        .filter(|&i| &haystack[i..i + needle.len()] == needle)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_hex_with_and_without_spaces() {
        assert_eq!(
            parse_hex("1F 20 03 D5").unwrap(),
            vec![0x1F, 0x20, 0x03, 0xD5]
        );
        assert_eq!(parse_hex("1f2003d5").unwrap(), vec![0x1F, 0x20, 0x03, 0xD5]);
    }

    #[test]
    fn parse_hex_rejects_bad_input() {
        assert!(parse_hex("ABC").is_err()); // tek hane
        assert!(parse_hex("ZZ").is_err()); // geçersiz
        assert!(parse_hex("   ").is_err()); // boş
    }

    #[test]
    fn search_bytes_finds_all_offsets() {
        let hay = b"\x01\xAA\xBB\x01\xAA\xBB\xCC";
        assert_eq!(search_bytes(hay, &[0xAA, 0xBB]), vec![1, 4]);
    }

    #[test]
    fn search_bytes_no_match_or_empty() {
        assert!(search_bytes(b"\x01\x02", &[0x03]).is_empty());
        assert!(search_bytes(b"\x01", &[]).is_empty());
        assert!(search_bytes(b"\x01", &[0x01, 0x02]).is_empty()); // needle > haystack
    }
}
