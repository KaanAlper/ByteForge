//! .NET / Mono metadata okuyucu (PC Unity Mono — Assembly-CSharp.dll).
//!
//! Bir managed PE'nin CLI başlığından metadata köküne ulaşır ve `#Strings`
//! heap'ini (tüm tip/metod/alan adları) dökümler. IL2CPP dump'ının Mono
//! karşılığı — full IL decompile değil, sembol adları (versiyon-bağımsız).

use crate::pe::pe_rva_to_offset;
use crate::{CoreError, Result};
use goblin::pe::PE;

/// Metadata kökü imzası: "BSJB".
const METADATA_SIGNATURE: u32 = 0x424A_5342;

/// Bir dizenin .NET sembol adına benzeyip benzemediği (tip/metod/alan).
fn is_symbol_like(s: &str) -> bool {
    if s.len() < 2 || s.len() > 512 {
        return false;
    }
    let first = s.chars().next().unwrap();
    if !(first.is_ascii_alphabetic() || first == '_' || first == '<') {
        return false;
    }
    s.chars()
        .all(|c| c.is_ascii_alphanumeric() || "_.<>`/@$-+".contains(c))
}

/// Metadata kökünden `#Strings` heap'ini bulup null-sonlandırmalı sembol
/// adlarını çıkarır (sıralı, tekilleştirilmiş). Kök geçersizse hata.
pub fn parse_metadata_strings(root: &[u8]) -> Result<Vec<String>> {
    if root.len() < 20 {
        return Err(CoreError::UnknownFormat);
    }
    let sig = u32::from_le_bytes([root[0], root[1], root[2], root[3]]);
    if sig != METADATA_SIGNATURE {
        return Err(CoreError::UnknownFormat);
    }
    // sig(4) major(2) minor(2) reserved(4) length(4) → version string
    let ver_len = u32::from_le_bytes([root[12], root[13], root[14], root[15]]) as usize;
    // Version string 4-bayt hizalı.
    let ver_padded = ver_len.div_ceil(4) * 4;
    let mut pos = 16 + ver_padded;
    if pos + 4 > root.len() {
        return Err(CoreError::UnknownFormat);
    }
    // flags(2) streams(2)
    let streams = u16::from_le_bytes([root[pos + 2], root[pos + 3]]) as usize;
    pos += 4;

    let mut strings_region: Option<(usize, usize)> = None;
    for _ in 0..streams {
        if pos + 8 > root.len() {
            break;
        }
        let off =
            u32::from_le_bytes([root[pos], root[pos + 1], root[pos + 2], root[pos + 3]]) as usize;
        let size = u32::from_le_bytes([root[pos + 4], root[pos + 5], root[pos + 6], root[pos + 7]])
            as usize;
        pos += 8;
        // Ad: null-sonlandırmalı ASCII, 4-bayt hizalı.
        let name_start = pos;
        while pos < root.len() && root[pos] != 0 {
            pos += 1;
        }
        let name = std::str::from_utf8(&root[name_start..pos]).unwrap_or("");
        pos += 1; // null
        pos = pos.div_ceil(4) * 4; // hizala
        if name == "#Strings" {
            strings_region = Some((off, size));
        }
    }

    let (off, size) = strings_region.ok_or(CoreError::UnknownFormat)?;
    let end = off.saturating_add(size).min(root.len());
    if off >= end {
        return Ok(Vec::new());
    }
    let blob = &root[off..end];

    let mut out = Vec::new();
    let mut cur = Vec::new();
    for &b in blob {
        if b == 0 {
            if let Ok(s) = std::str::from_utf8(&cur) {
                if is_symbol_like(s) {
                    out.push(s.to_string());
                }
            }
            cur.clear();
        } else {
            cur.push(b);
        }
    }
    out.sort();
    out.dedup();
    Ok(out)
}

/// Bir managed PE'nin (.NET) sembol adlarını (#Strings) dökümler.
pub fn dotnet_strings(bytes: &[u8]) -> Result<Vec<String>> {
    let pe = PE::parse(bytes)?;
    let oh = pe.header.optional_header.ok_or(CoreError::UnknownFormat)?;
    let clr = oh
        .data_directories
        .get_clr_runtime_header()
        .filter(|d| d.virtual_address != 0)
        .ok_or(CoreError::UnknownFormat)?;
    // CLI başlığı (IMAGE_COR20_HEADER): offset 8'de MetaData RVA(4)+Size(4).
    let cli_off = pe_rva_to_offset(bytes, clr.virtual_address as u64)?
        .ok_or(CoreError::UnknownFormat)? as usize;
    if cli_off + 16 > bytes.len() {
        return Err(CoreError::UnknownFormat);
    }
    let md_rva = u32::from_le_bytes([
        bytes[cli_off + 8],
        bytes[cli_off + 9],
        bytes[cli_off + 10],
        bytes[cli_off + 11],
    ]) as u64;
    let md_size = u32::from_le_bytes([
        bytes[cli_off + 12],
        bytes[cli_off + 13],
        bytes[cli_off + 14],
        bytes[cli_off + 15],
    ]) as usize;
    let md_off = pe_rva_to_offset(bytes, md_rva)?.ok_or(CoreError::UnknownFormat)? as usize;
    let end = md_off.saturating_add(md_size).min(bytes.len());
    if md_off >= end {
        return Err(CoreError::UnknownFormat);
    }
    parse_metadata_strings(&bytes[md_off..end])
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Minimal geçerli metadata kökü + #Strings heap kurar.
    fn synthetic_root(strings: &[&str]) -> Vec<u8> {
        // #Strings blob: baştan null (boş dize) + her ad null-sonlandırmalı.
        let mut heap = vec![0u8];
        for s in strings {
            heap.extend_from_slice(s.as_bytes());
            heap.push(0);
        }
        let version = b"v4.0.30319\0\0"; // 12 bayt (4-hizalı)
        let mut root = Vec::new();
        root.extend_from_slice(&METADATA_SIGNATURE.to_le_bytes()); // sig
        root.extend_from_slice(&[0, 0, 0, 0]); // major/minor
        root.extend_from_slice(&[0, 0, 0, 0]); // reserved
        root.extend_from_slice(&(version.len() as u32).to_le_bytes()); // length
        root.extend_from_slice(version); // version string
        root.extend_from_slice(&[0, 0]); // flags
        root.extend_from_slice(&1u16.to_le_bytes()); // streams = 1
                                                     // Stream header — offset hesaplanacak: header sonrası hemen heap.
                                                     // header: off(4)+size(4)+"#Strings\0"(9→12 hizalı)=20 bayt
        let header_len = 8 + 12;
        let heap_off = (root.len() + header_len) as u32;
        root.extend_from_slice(&heap_off.to_le_bytes()); // offset
        root.extend_from_slice(&(heap.len() as u32).to_le_bytes()); // size
        root.extend_from_slice(b"#Strings\0"); // 9 bayt
        root.extend_from_slice(&[0, 0, 0]); // 12'ye hizala
        root.extend_from_slice(&heap);
        root
    }

    #[test]
    fn extracts_strings_from_root() {
        let root = synthetic_root(&["System", "get_isPremium", "PlayerData", "x"]);
        let syms = parse_metadata_strings(&root).unwrap();
        assert!(syms.contains(&"System".to_string()));
        assert!(syms.contains(&"get_isPremium".to_string()));
        assert!(syms.contains(&"PlayerData".to_string()));
        // "x" tek karakter değil ama 1 uzunlukta → is_symbol_like eler (min 2).
        assert!(!syms.contains(&"x".to_string()));
    }

    #[test]
    fn rejects_bad_signature() {
        let mut root = synthetic_root(&["A"]);
        root[0] = 0;
        assert!(parse_metadata_strings(&root).is_err());
    }

    #[test]
    fn rejects_non_pe_for_dotnet() {
        assert!(dotnet_strings(b"not a pe").is_err());
    }

    #[test]
    fn native_dll_has_no_dotnet_metadata() {
        // forge.dll native → CLR başlığı yok.
        let bytes = include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/forge.dll"
        ));
        assert!(dotnet_strings(bytes).is_err());
    }
}
