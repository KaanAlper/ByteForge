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

/// Karşılaştırma türü (Cheat Engine "scan type"). `operand`/`operand2`
/// yalnızca ilgili türlerde kullanılır; göreli türler (`Increased` vb.)
/// önceki değeri (`old`) gerektirir.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Compare {
    /// == operand (kesin değer).
    Exact,
    /// != operand.
    NotEqual,
    /// > operand.
    Greater,
    /// < operand.
    Less,
    /// operand <= x <= operand2.
    Between,
    /// İlk tarama: her aday alınır (bilinmeyen başlangıç değeri).
    Unknown,
    /// x > önceki.
    Increased,
    /// x < önceki.
    Decreased,
    /// x != önceki.
    Changed,
    /// x == önceki.
    Unchanged,
    /// x == önceki + operand.
    IncreasedBy,
    /// x == önceki - operand.
    DecreasedBy,
}

impl Compare {
    /// Bu karşılaştırma önceki değeri (snapshot) gerektiriyor mu?
    pub fn needs_previous(self) -> bool {
        matches!(
            self,
            Compare::Increased
                | Compare::Decreased
                | Compare::Changed
                | Compare::Unchanged
                | Compare::IncreasedBy
                | Compare::DecreasedBy
        )
    }
    /// İlk tarama için, kesin bir needle baytı olmadan (fuzzy) mı çalışır?
    /// Unknown ve göreli türler ilk aşamada tüm belleği anlık görüntüler.
    pub fn is_fuzzy_initial(self) -> bool {
        self == Compare::Unknown
    }
}

/// Bir bayt dizisini türüne göre sayıya (f64) çözer. Sıralama/aritmetik
/// karşılaştırmalar bunun üzerinden yapılır (tipik oyun değerleri < 2^53).
pub fn decode_num(bytes: &[u8], ty: ValueType) -> Option<f64> {
    Some(match ty {
        ValueType::U8 => *bytes.first()? as f64,
        ValueType::I32 => i32::from_le_bytes(bytes.get(..4)?.try_into().ok()?) as f64,
        ValueType::I64 => i64::from_le_bytes(bytes.get(..8)?.try_into().ok()?) as f64,
        ValueType::F32 => f32::from_le_bytes(bytes.get(..4)?.try_into().ok()?) as f64,
        ValueType::F64 => f64::from_le_bytes(bytes.get(..8)?.try_into().ok()?),
    })
}

/// Verilen türde iki sayının "eşit" sayılma toleransı. Tamsayılarda 0.5
/// (tam eşitlik), kayan noktada küçük bir mutlak epsilon.
fn eps(ty: ValueType) -> f64 {
    match ty {
        ValueType::F32 | ValueType::F64 => 1e-4,
        _ => 0.5,
    }
}

/// Bir adayın (yeni bayt `new`, opsiyonel önceki bayt `old`) verilen
/// karşılaştırmayı sağlayıp sağlamadığı. Göreli türlerde `old` yoksa false.
pub fn matches(
    new: &[u8],
    old: Option<&[u8]>,
    ty: ValueType,
    cmp: Compare,
    operand: f64,
    operand2: f64,
) -> bool {
    let Some(n) = decode_num(new, ty) else {
        return false;
    };
    let e = eps(ty);
    match cmp {
        Compare::Unknown => true,
        Compare::Exact => (n - operand).abs() < e,
        Compare::NotEqual => (n - operand).abs() >= e,
        Compare::Greater => n > operand,
        Compare::Less => n < operand,
        Compare::Between => n >= operand.min(operand2) && n <= operand.max(operand2),
        Compare::Increased
        | Compare::Decreased
        | Compare::Changed
        | Compare::Unchanged
        | Compare::IncreasedBy
        | Compare::DecreasedBy => {
            let Some(o) = old.and_then(|b| decode_num(b, ty)) else {
                return false;
            };
            match cmp {
                Compare::Increased => n > o,
                Compare::Decreased => n < o,
                Compare::Changed => (n - o).abs() >= e,
                Compare::Unchanged => (n - o).abs() < e,
                Compare::IncreasedBy => (n - (o + operand)).abs() < e,
                Compare::DecreasedBy => (n - (o - operand)).abs() < e,
                _ => unreachable!(),
            }
        }
    }
}

/// Bir bayt dizisini türüne göre insan-okunur metne çözer (UI için).
pub fn format_value(bytes: &[u8], ty: ValueType) -> String {
    match ty {
        ValueType::U8 => bytes.first().map(|v| v.to_string()),
        ValueType::I32 => bytes
            .get(..4)
            .and_then(|b| b.try_into().ok())
            .map(|b| i32::from_le_bytes(b).to_string()),
        ValueType::I64 => bytes
            .get(..8)
            .and_then(|b| b.try_into().ok())
            .map(|b| i64::from_le_bytes(b).to_string()),
        ValueType::F32 => bytes
            .get(..4)
            .and_then(|b| b.try_into().ok())
            .map(|b| f32::from_le_bytes(b).to_string()),
        ValueType::F64 => bytes
            .get(..8)
            .and_then(|b| b.try_into().ok())
            .map(|b| f64::from_le_bytes(b).to_string()),
    }
    .unwrap_or_else(|| "?".into())
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

    #[test]
    fn decode_and_compare() {
        let a = 100i32.to_le_bytes();
        let b = 150i32.to_le_bytes();
        assert!(matches(&b, Some(&a), ValueType::I32, Compare::Increased, 0.0, 0.0));
        assert!(matches(&a, Some(&b), ValueType::I32, Compare::Decreased, 0.0, 0.0));
        assert!(matches(&b, Some(&a), ValueType::I32, Compare::Changed, 0.0, 0.0));
        assert!(matches(&a, Some(&a), ValueType::I32, Compare::Unchanged, 0.0, 0.0));
        assert!(matches(&b, Some(&a), ValueType::I32, Compare::IncreasedBy, 50.0, 0.0));
        assert!(!matches(&b, Some(&a), ValueType::I32, Compare::IncreasedBy, 40.0, 0.0));
        assert!(matches(&a, None, ValueType::I32, Compare::Exact, 100.0, 0.0));
        assert!(matches(&a, None, ValueType::I32, Compare::Greater, 50.0, 0.0));
        assert!(matches(&a, None, ValueType::I32, Compare::Between, 50.0, 200.0));
        assert!(matches(&a, None, ValueType::I32, Compare::Unknown, 0.0, 0.0));
        // göreli karşılaştırma önceki değer olmadan çalışmaz
        assert!(!matches(&b, None, ValueType::I32, Compare::Increased, 0.0, 0.0));
    }

    #[test]
    fn float_compare_tolerance() {
        let a = 1.5f32.to_le_bytes();
        assert!(matches(&a, None, ValueType::F32, Compare::Exact, 1.5, 0.0));
        assert!(!matches(&a, None, ValueType::F32, Compare::Exact, 1.6, 0.0));
    }

    #[test]
    fn formats_values() {
        assert_eq!(format_value(&42i32.to_le_bytes(), ValueType::I32), "42");
        assert_eq!(format_value(&[7u8], ValueType::U8), "7");
    }
}
