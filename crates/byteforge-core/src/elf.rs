use crate::Result;
use goblin::elf::{program_header::PT_LOAD, Elf};
use goblin::strtab::Strtab;
use serde::Serialize;

/// Bir `.so` içindeki tek bir sembol.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SoSymbol {
    pub name: String,
    /// Sembolün sanal adresi (RVA / virtual offset).
    pub rva: u64,
    /// `.so` içindeki gerçek dosya ofseti (patch hedefi); RVA bir LOAD
    /// segmentine düşmüyorsa None.
    pub file_offset: Option<u64>,
    pub size: u64,
    pub is_function: bool,
    /// Fonksiyonun başındaki baytlar bilinen bir ARM64 yama şablonuyla eşleşiyorsa
    /// şablon kimliği (ör. "return_true"); değilse None. "MODLU" rozeti için.
    pub patched: Option<String>,
}

/// Bir `.so` ELF baytlarındaki adlandırılmış sembolleri listeler
/// (hem dinamik hem tam sembol tablosu; stripped değilse ikincisi de dolar).
pub fn list_symbols(bytes: &[u8]) -> Result<Vec<SoSymbol>> {
    let elf = Elf::parse(bytes)?;
    let mut out = Vec::new();
    collect(&elf, bytes, &elf.dynsyms, &elf.dynstrtab, &mut out);
    collect(&elf, bytes, &elf.syms, &elf.strtab, &mut out);

    out.sort_by(|a, b| a.rva.cmp(&b.rva).then_with(|| a.name.cmp(&b.name)));
    out.dedup();
    Ok(out)
}

fn collect(
    elf: &Elf,
    bytes: &[u8],
    syms: &goblin::elf::Symtab,
    strtab: &Strtab,
    out: &mut Vec<SoSymbol>,
) {
    for sym in syms.iter() {
        let name = strtab.get_at(sym.st_name).unwrap_or("");
        if name.is_empty() {
            continue;
        }
        let file_offset = vaddr_to_offset(elf, sym.st_value);
        // Fonksiyon başındaki 12 baytı okuyup bilinen yamayla eşleştir.
        let patched = if sym.is_function() {
            file_offset
                .map(|o| o as usize)
                .and_then(|o| bytes.get(o..(o + 12).min(bytes.len())))
                .and_then(crate::patch::detect_arm64_patch)
                .map(|s| s.to_string())
        } else {
            None
        };
        out.push(SoSymbol {
            name: name.to_string(),
            rva: sym.st_value,
            file_offset,
            size: sym.st_size,
            is_function: sym.is_function(),
            patched,
        });
    }
}

/// Sanal adresi (RVA), onu içeren LOAD segmentini kullanarak dosya ofsetine çevirir.
fn vaddr_to_offset(elf: &Elf, vaddr: u64) -> Option<u64> {
    if vaddr == 0 {
        return None;
    }
    for ph in &elf.program_headers {
        if ph.p_type == PT_LOAD && vaddr >= ph.p_vaddr && vaddr < ph.p_vaddr + ph.p_memsz {
            return Some(vaddr - ph.p_vaddr + ph.p_offset);
        }
    }
    None
}

/// Bir yüklenebilir (PT_LOAD) ELF segmenti — RVA↔ofset dönüşümünün temeli.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ElfSegment {
    pub index: usize,
    /// Segmentin sanal adresi (RVA tabanı).
    pub vaddr: u64,
    /// Dosyadaki ofseti.
    pub file_offset: u64,
    /// Dosyadaki boyutu.
    pub file_size: u64,
    /// Bellekteki boyutu (file_size'dan büyükse fark .bss'tir — dosyada yoktur).
    pub mem_size: u64,
    /// İzinler, ör. "R-X" (okuma/yazma/çalıştırma).
    pub flags: String,
}

fn perm_flags(p_flags: u32) -> String {
    // PF_R=4, PF_W=2, PF_X=1
    let r = if p_flags & 0x4 != 0 { 'R' } else { '-' };
    let w = if p_flags & 0x2 != 0 { 'W' } else { '-' };
    let x = if p_flags & 0x1 != 0 { 'X' } else { '-' };
    format!("{r}{w}{x}")
}

/// Bir `.so`'nun yüklenebilir segmentlerini listeler (RVA↔ofset haritası).
pub fn load_segments(bytes: &[u8]) -> Result<Vec<ElfSegment>> {
    let elf = Elf::parse(bytes)?;
    let mut out = Vec::new();
    for (i, ph) in elf.program_headers.iter().enumerate() {
        if ph.p_type == PT_LOAD {
            out.push(ElfSegment {
                index: i,
                vaddr: ph.p_vaddr,
                file_offset: ph.p_offset,
                file_size: ph.p_filesz,
                mem_size: ph.p_memsz,
                flags: perm_flags(ph.p_flags),
            });
        }
    }
    Ok(out)
}

/// Bir sanal adresi (RVA) dosya ofsetine çevirir (yamalanacak baytın yeri).
/// RVA bir LOAD segmentine düşmüyorsa (ör. yalnızca .bss) None.
pub fn rva_to_file_offset(bytes: &[u8], rva: u64) -> Result<Option<u64>> {
    let elf = Elf::parse(bytes)?;
    Ok(vaddr_to_offset(&elf, rva))
}

/// Bir dosya ofsetini sanal adrese (RVA) çevirir — dumper/IDA ile eşleştirmek için.
pub fn file_offset_to_rva(bytes: &[u8], offset: u64) -> Result<Option<u64>> {
    let elf = Elf::parse(bytes)?;
    for ph in &elf.program_headers {
        if ph.p_type == PT_LOAD && offset >= ph.p_offset && offset < ph.p_offset + ph.p_filesz {
            return Ok(Some(offset - ph.p_offset + ph.p_vaddr));
        }
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> Vec<SoSymbol> {
        let bytes = include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/libforge.so"
        ));
        list_symbols(bytes).unwrap()
    }

    #[test]
    fn lists_exported_functions() {
        let syms = fixture();
        let names: Vec<&str> = syms.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"forge_add"));
        assert!(names.contains(&"forge_check"));
    }

    #[test]
    fn function_symbol_has_offset_and_flag() {
        let syms = fixture();
        let add = syms.iter().find(|s| s.name == "forge_add").unwrap();
        assert!(add.is_function);
        assert!(add.rva > 0);
        assert!(add.file_offset.is_some());
    }

    #[test]
    fn errors_on_non_elf() {
        assert!(list_symbols(b"definitely not an ELF file").is_err());
    }

    fn raw() -> &'static [u8] {
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/libforge.so"
        ))
    }

    #[test]
    fn lists_load_segments() {
        let segs = load_segments(raw()).unwrap();
        assert!(!segs.is_empty());
        // En az bir çalıştırılabilir (kod) segment olmalı.
        assert!(segs.iter().any(|s| s.flags.ends_with('X')));
    }

    #[test]
    fn rva_offset_roundtrip_matches_symbol() {
        // Bir fonksiyon sembolünün RVA'sı → ofset → RVA aynı olmalı.
        let add = fixture()
            .into_iter()
            .find(|s| s.name == "forge_add")
            .unwrap();
        let off = rva_to_file_offset(raw(), add.rva).unwrap().unwrap();
        assert_eq!(off, add.file_offset.unwrap());
        let back = file_offset_to_rva(raw(), off).unwrap().unwrap();
        assert_eq!(back, add.rva);
    }

    #[test]
    fn unmapped_rva_returns_none() {
        // Çok büyük, hiçbir segmente düşmeyen RVA.
        assert_eq!(rva_to_file_offset(raw(), 0xFFFF_FFFF_0000).unwrap(), None);
    }
}
