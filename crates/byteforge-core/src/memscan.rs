//! Bellek tarayıcı çekirdeği (Cheat Engine tarzı) — saf, test edilebilir mantık.
//!
//! OS etkileşimi (süreç listeleme, /proc/pid/mem okuma) src-tauri katmanındadır;
//! burada yalnızca /proc/pid/maps ayrıştırma, değer kodlama ve bayt arama var.

use serde::{Deserialize, Serialize};

/// Taranacak/yazılacak değerin türü.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ValueType {
    I32,
    I64,
    F32,
    F64,
    U8,
}

impl ValueType {
    /// Bu türün bayt cinsinden genişliği.
    pub fn width(self) -> usize {
        match self {
            ValueType::U8 => 1,
            ValueType::I32 | ValueType::F32 => 4,
            ValueType::I64 | ValueType::F64 => 8,
        }
    }
}

/// Bir metinsel değeri türüne göre little-endian baytlara kodlar.
/// Sayısal ayrıştırma başarısızsa None.
pub fn encode_value(text: &str, ty: ValueType) -> Option<Vec<u8>> {
    let t = text.trim();
    match ty {
        ValueType::U8 => t.parse::<u8>().ok().map(|v| v.to_le_bytes().to_vec()),
        ValueType::I32 => t.parse::<i32>().ok().map(|v| v.to_le_bytes().to_vec()),
        ValueType::I64 => t.parse::<i64>().ok().map(|v| v.to_le_bytes().to_vec()),
        ValueType::F32 => t.parse::<f32>().ok().map(|v| v.to_le_bytes().to_vec()),
        ValueType::F64 => t.parse::<f64>().ok().map(|v| v.to_le_bytes().to_vec()),
    }
}

/// /proc/pid/maps içindeki tek bir bellek bölgesi.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MemRegion {
    pub start: u64,
    pub end: u64,
    /// İzinler, ör. "rw-p".
    pub perms: String,
    /// Eşlenen yol (varsa), ör. "[heap]", "/lib/...".
    pub path: String,
}

impl MemRegion {
    pub fn is_readable(&self) -> bool {
        self.perms.starts_with('r')
    }
    pub fn is_writable(&self) -> bool {
        self.perms.as_bytes().get(1) == Some(&b'w')
    }
    pub fn size(&self) -> u64 {
        self.end.saturating_sub(self.start)
    }
}

/// /proc/pid/maps metnini bölgelere ayrıştırır.
pub fn parse_maps(text: &str) -> Vec<MemRegion> {
    let mut out = Vec::new();
    for line in text.lines() {
        // Biçim: "start-end perms offset dev inode  path"
        let mut parts = line.splitn(6, ' ');
        let range = parts.next().unwrap_or("");
        let perms = parts.next().unwrap_or("");
        // offset, dev, inode atlanır
        let (_offset, _dev, _inode) = (parts.next(), parts.next(), parts.next());
        let path = parts.next().unwrap_or("").trim().to_string();

        let Some((s, e)) = range.split_once('-') else {
            continue;
        };
        let (Ok(start), Ok(end)) = (u64::from_str_radix(s, 16), u64::from_str_radix(e, 16)) else {
            continue;
        };
        if perms.len() < 4 {
            continue;
        }
        out.push(MemRegion {
            start,
            end,
            perms: perms.to_string(),
            path,
        });
    }
    out
}

/// Bir tampon içinde `needle`'ın tüm ofsetlerini bulur.
pub fn find_all(haystack: &[u8], needle: &[u8]) -> Vec<usize> {
    if needle.is_empty() || haystack.len() < needle.len() {
        return Vec::new();
    }
    let mut hits = Vec::new();
    let mut i = 0;
    while i + needle.len() <= haystack.len() {
        if &haystack[i..i + needle.len()] == needle {
            hits.push(i);
        }
        i += 1;
    }
    hits
}

/// Bir bölgenin taranmaya değer olup olmadığı: okunabilir, yazılabilir (değer
/// değiştirilebilir olmalı) ve salt-kod (r-x) olmayan.
pub fn is_scannable(r: &MemRegion) -> bool {
    r.is_readable() && r.is_writable() && r.size() > 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_values() {
        assert_eq!(encode_value("1", ValueType::I32).unwrap(), vec![1, 0, 0, 0]);
        assert_eq!(
            encode_value("256", ValueType::I32).unwrap(),
            vec![0, 1, 0, 0]
        );
        assert_eq!(encode_value("255", ValueType::U8).unwrap(), vec![255]);
        assert_eq!(
            encode_value("1.0", ValueType::F32).unwrap(),
            1.0f32.to_le_bytes().to_vec()
        );
        assert!(encode_value("abc", ValueType::I32).is_none());
        assert!(encode_value("300", ValueType::U8).is_none()); // taşma
    }

    #[test]
    fn value_widths() {
        assert_eq!(ValueType::I32.width(), 4);
        assert_eq!(ValueType::I64.width(), 8);
        assert_eq!(ValueType::U8.width(), 1);
        assert_eq!(ValueType::F64.width(), 8);
    }

    #[test]
    fn parses_maps_lines() {
        let text = "\
55a1b2c3d000-55a1b2c3e000 r-xp 00000000 08:02 100    /usr/bin/app
55a1b2c3f000-55a1b2c40000 rw-p 00001000 08:02 100    [heap]
7fff00000000-7fff00001000 ---p 00000000 00:00 0 ";
        let regions = parse_maps(text);
        assert_eq!(regions.len(), 3);
        assert_eq!(regions[0].start, 0x55a1b2c3d000);
        assert_eq!(regions[0].end, 0x55a1b2c3e000);
        assert!(regions[0].is_readable());
        assert!(!regions[0].is_writable()); // r-x
        assert_eq!(regions[1].path, "[heap]");
        assert!(regions[1].is_writable());
        assert!(is_scannable(&regions[1])); // rw heap
        assert!(!is_scannable(&regions[0])); // r-x kod
        assert!(!is_scannable(&regions[2])); // ---p
    }

    #[test]
    fn find_all_offsets() {
        let hay = [0u8, 1, 0, 1, 0, 1];
        assert_eq!(find_all(&hay, &[1, 0]), vec![1, 3]);
        assert_eq!(find_all(&hay, &[9]), Vec::<usize>::new());
        assert_eq!(find_all(&[], &[1]), Vec::<usize>::new());
    }
}
