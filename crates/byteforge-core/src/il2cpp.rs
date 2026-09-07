use serde::Serialize;

/// global-metadata.dat sihirli sayısı.
const IL2CPP_MAGIC: u32 = 0xFAB1_1BAF;

/// global-metadata.dat başlığından çıkarılan temel bilgi.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Il2CppMetadata {
    pub valid: bool,
    pub version: i32,
    pub size: u64,
}

/// global-metadata.dat baytlarının başlığını ayrıştırır (magic + version).
pub fn parse_metadata_header(bytes: &[u8]) -> Il2CppMetadata {
    if bytes.len() < 8 {
        return Il2CppMetadata {
            valid: false,
            version: 0,
            size: bytes.len() as u64,
        };
    }
    let magic = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
    if magic != IL2CPP_MAGIC {
        return Il2CppMetadata {
            valid: false,
            version: 0,
            size: bytes.len() as u64,
        };
    }
    let version = i32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]);
    Il2CppMetadata {
        valid: true,
        version,
        size: bytes.len() as u64,
    }
}

/// Bir dizenin IL2CPP sembol adına benzeyip benzemediği (sınıf/metod/alan).
fn is_symbol_like(s: &str) -> bool {
    if s.len() < 3 || s.len() > 200 {
        return false;
    }
    let first_ok = s
        .chars()
        .next()
        .map(|c| c.is_ascii_alphabetic() || c == '_' || c == '<')
        .unwrap_or(false);
    first_ok
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || "_.<>`$".contains(c))
}

/// global-metadata.dat baytlarından sembol adlarını (sınıf/metod/alan/parametre)
/// çıkarır: null-sonlandırmalı ASCII string tablosunu tarar, sembol-benzeri
/// olanları alır, tekilleştirip sıralar. Metadata sürümünden bağımsız çalışır.
pub fn dump_metadata_symbols(bytes: &[u8]) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur: Vec<u8> = Vec::new();
    for &b in bytes {
        if b == 0 {
            if cur.len() >= 3 {
                if let Ok(s) = std::str::from_utf8(&cur) {
                    if is_symbol_like(s) {
                        out.push(s.to_string());
                    }
                }
            }
            cur.clear();
        } else if b.is_ascii_graphic() {
            cur.push(b);
        } else {
            cur.clear();
        }
    }
    out.sort();
    out.dedup();
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_symbol_like_names() {
        // İki geçerli isim (null ile ayrılmış) + gürültü.
        let mut data = Vec::new();
        data.extend_from_slice(b"get_isPremium\0");
        data.extend_from_slice(b"CheckLicense\0");
        data.extend_from_slice(&[0x01, 0x02, 0x03, 0x00]); // sembol değil
        data.extend_from_slice(b"<Module>\0");
        let syms = dump_metadata_symbols(&data);
        assert!(syms.contains(&"get_isPremium".to_string()));
        assert!(syms.contains(&"CheckLicense".to_string()));
        assert!(syms.contains(&"<Module>".to_string()));
        assert_eq!(syms.len(), 3); // gürültü elenmiş
    }

    #[test]
    fn parses_valid_header() {
        let mut data = Vec::new();
        data.extend_from_slice(&IL2CPP_MAGIC.to_le_bytes());
        data.extend_from_slice(&29i32.to_le_bytes());
        data.extend_from_slice(&[0u8; 40]);
        let m = parse_metadata_header(&data);
        assert!(m.valid);
        assert_eq!(m.version, 29);
    }

    #[test]
    fn rejects_bad_magic() {
        let data = [0u8; 16];
        assert!(!parse_metadata_header(&data).valid);
    }

    #[test]
    fn rejects_too_small() {
        assert!(!parse_metadata_header(&[1, 2, 3]).valid);
    }
}
