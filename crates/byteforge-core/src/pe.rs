use crate::Result;
use goblin::pe::PE;
use serde::Serialize;

/// Bir Windows PE (EXE/DLL) dosyasının profili.
#[derive(Debug, Clone, Serialize)]
pub struct PeProfile {
    pub is_64bit: bool,
    pub is_dll: bool,
    pub machine: String,
    pub subsystem: String,
    pub is_dotnet: bool,
    /// Derleyici/çalışma zamanı tahmini (MSVC / MinGW / .NET / ...).
    pub compiler: String,
    pub sections: Vec<String>,
    pub imported_dlls: Vec<String>,
    pub entry_point: u64,
    /// Önerilen müdahale hattı (insan-okur).
    pub recommended_pipeline: String,
    /// Tespit edilen paketleyici/koruyucular (UPX, VMProtect, Themida...).
    pub packers: Vec<crate::packer::PackerHit>,
    /// Section başına entropi (yüksek → sıkıştırılmış/şifreli/paketlenmiş).
    pub section_entropy: Vec<SectionEntropy>,
    /// Herhangi bir section yüksek entropili mi (paketlenmişlik göstergesi).
    pub likely_packed: bool,
}

/// Bir PE section'ının adı + entropisi.
#[derive(Debug, Clone, Serialize)]
pub struct SectionEntropy {
    pub name: String,
    pub bits: f64,
    pub class: crate::entropy::EntropyClass,
}

const IMAGE_FILE_DLL: u16 = 0x2000;

fn machine_name(m: u16) -> &'static str {
    match m {
        0x8664 => "x86-64",
        0x014c => "x86 (32-bit)",
        0xAA64 => "ARM64",
        0x01c0 | 0x01c4 => "ARM",
        _ => "bilinmeyen",
    }
}

fn subsystem_name(s: u16) -> &'static str {
    match s {
        2 => "Windows GUI",
        3 => "Windows Console",
        9 => "Windows CE GUI",
        _ => "diğer",
    }
}

fn detect_compiler(is_dotnet: bool, sections: &[String], dlls: &[String]) -> String {
    if is_dotnet {
        return ".NET / Mono (managed)".into();
    }
    let low: Vec<String> = dlls.iter().map(|d| d.to_lowercase()).collect();
    let has = |n: &str| low.iter().any(|d| d.contains(n));
    if sections.iter().any(|s| s.contains("rustc")) {
        return "Rust".into();
    }
    // msvcrt.dll (rakamsız) MinGW/GCC'nin klasik CRT'sidir; MSVC msvcr###/vcruntime kullanır.
    if low.iter().any(|d| d == "msvcrt.dll") {
        return "MinGW / GCC".into();
    }
    if has("vcruntime") || has("msvcp") || has("ucrtbase") || has("msvcr") {
        return "MSVC (Visual C++)".into();
    }
    "bilinmeyen (native)".into()
}

/// Bir PE (EXE/DLL) baytlarını analiz eder.
pub fn analyze_pe(bytes: &[u8]) -> Result<PeProfile> {
    let pe = PE::parse(bytes)?;
    let is_64bit = pe.is_64;
    let is_dll = pe.header.coff_header.characteristics & IMAGE_FILE_DLL != 0;
    let machine = machine_name(pe.header.coff_header.machine).to_string();

    let (subsystem, is_dotnet) = match &pe.header.optional_header {
        Some(oh) => {
            let sub = subsystem_name(oh.windows_fields.subsystem).to_string();
            let dotnet = oh
                .data_directories
                .get_clr_runtime_header()
                .map(|d| d.virtual_address != 0)
                .unwrap_or(false);
            (sub, dotnet)
        }
        None => ("bilinmeyen".to_string(), false),
    };

    let sections: Vec<String> = pe
        .sections
        .iter()
        .map(|s| s.name().unwrap_or("?").to_string())
        .collect();
    let imported_dlls: Vec<String> = pe.libraries.iter().map(|s| s.to_string()).collect();
    let compiler = detect_compiler(is_dotnet, &sections, &imported_dlls);
    let packers = crate::packer::detect_pe_packers(&sections);

    // Section başına entropi — ham veriyi dosya baytlarından okur.
    let section_entropy: Vec<SectionEntropy> = pe
        .sections
        .iter()
        .map(|s| {
            let start = s.pointer_to_raw_data as usize;
            let size = s.size_of_raw_data as usize;
            let end = start.saturating_add(size).min(bytes.len());
            let data = if start < end {
                &bytes[start..end]
            } else {
                &[][..]
            };
            let info = crate::entropy::analyze(data);
            SectionEntropy {
                name: s.name().unwrap_or("?").to_string(),
                bits: info.bits,
                class: info.class,
            }
        })
        .collect();
    let likely_packed = section_entropy
        .iter()
        .any(|s| s.class == crate::entropy::EntropyClass::High);

    let recommended_pipeline = if is_dotnet {
        "PC Unity/.NET: Assembly-CSharp.dll → dnSpy/IL; GameAssembly.dll varsa IL2CPP".into()
    } else {
        "Native PE: x86/x64 hex patch (opcode enjeksiyonu); import/section analizi".into()
    };

    Ok(PeProfile {
        is_64bit,
        is_dll,
        machine,
        subsystem,
        is_dotnet,
        compiler,
        sections,
        imported_dlls,
        entry_point: pe.entry as u64,
        recommended_pipeline,
        packers,
        section_entropy,
        likely_packed,
    })
}

/// Bir PE section'ı (RVA↔ofset dönüşümünün temeli).
#[derive(Debug, Clone, Serialize)]
pub struct PeSection {
    pub name: String,
    /// Sanal adres (RVA tabanı, ImageBase'e göreli).
    pub virtual_address: u64,
    pub virtual_size: u64,
    /// Dosyadaki ham veri ofseti.
    pub raw_pointer: u64,
    pub raw_size: u64,
    /// İzinler, ör. "R-X".
    pub flags: String,
}

const IMAGE_SCN_MEM_EXECUTE: u32 = 0x2000_0000;
const IMAGE_SCN_MEM_READ: u32 = 0x4000_0000;
const IMAGE_SCN_MEM_WRITE: u32 = 0x8000_0000;

fn sect_flags(ch: u32) -> String {
    let r = if ch & IMAGE_SCN_MEM_READ != 0 {
        'R'
    } else {
        '-'
    };
    let w = if ch & IMAGE_SCN_MEM_WRITE != 0 {
        'W'
    } else {
        '-'
    };
    let x = if ch & IMAGE_SCN_MEM_EXECUTE != 0 {
        'X'
    } else {
        '-'
    };
    format!("{r}{w}{x}")
}

/// Bir PE'nin section'larını listeler (RVA↔ofset haritası).
pub fn pe_sections(bytes: &[u8]) -> Result<Vec<PeSection>> {
    let pe = PE::parse(bytes)?;
    Ok(pe
        .sections
        .iter()
        .map(|s| PeSection {
            name: s.name().unwrap_or("?").to_string(),
            virtual_address: s.virtual_address as u64,
            virtual_size: s.virtual_size as u64,
            raw_pointer: s.pointer_to_raw_data as u64,
            raw_size: s.size_of_raw_data as u64,
            flags: sect_flags(s.characteristics),
        })
        .collect())
}

/// PE RVA'yı (ImageBase'e göreli) dosya ofsetine çevirir. Section'ın sanal
/// alanına düşüp dosyada karşılığı yoksa (ör. .bss dolgusu) None.
pub fn pe_rva_to_offset(bytes: &[u8], rva: u64) -> Result<Option<u64>> {
    let sections = pe_sections(bytes)?;
    // Header bölgesi (ilk section'dan önce) 1:1 eşlenir.
    let first_va = sections
        .iter()
        .map(|s| s.virtual_address)
        .min()
        .unwrap_or(0);
    if rva < first_va {
        return Ok(Some(rva));
    }
    for s in &sections {
        if rva >= s.virtual_address && rva < s.virtual_address + s.virtual_size {
            let delta = rva - s.virtual_address;
            if delta < s.raw_size {
                return Ok(Some(s.raw_pointer + delta));
            }
            return Ok(None); // sanal alan var ama dosyada yok
        }
    }
    Ok(None)
}

/// Dosya ofsetini PE RVA'sına çevirir (dumper/IDA ile eşleştirmek için).
pub fn pe_offset_to_rva(bytes: &[u8], offset: u64) -> Result<Option<u64>> {
    let sections = pe_sections(bytes)?;
    let first_raw = sections
        .iter()
        .filter(|s| s.raw_size > 0)
        .map(|s| s.raw_pointer)
        .min()
        .unwrap_or(0);
    if offset < first_raw {
        return Ok(Some(offset)); // header bölgesi
    }
    for s in &sections {
        if s.raw_size > 0 && offset >= s.raw_pointer && offset < s.raw_pointer + s.raw_size {
            return Ok(Some(offset - s.raw_pointer + s.virtual_address));
        }
    }
    Ok(None)
}

/// Bir PE'nin dışa aktardığı (export) bir fonksiyon.
#[derive(Debug, Clone, Serialize)]
pub struct PeExport {
    pub name: String,
    pub rva: u64,
    pub file_offset: Option<u64>,
    /// Bilinen bir x86 yama şablonuyla eşleşiyorsa kimliği (MODLU rozeti).
    pub patched: Option<String>,
}

/// Bir PE'nin import ettiği fonksiyon adlarını döndürür (anti-tamper taraması için).
pub fn pe_import_names(bytes: &[u8]) -> Result<Vec<String>> {
    let pe = PE::parse(bytes)?;
    Ok(pe.imports.iter().map(|i| i.name.to_string()).collect())
}

/// Bir PE'nin export tablosunu (adlı fonksiyonlar) döndürür, yama tespitiyle.
pub fn pe_exports(bytes: &[u8]) -> Result<Vec<PeExport>> {
    let pe = PE::parse(bytes)?;
    let mut out = Vec::new();
    for e in &pe.exports {
        let Some(name) = e.name else { continue };
        let rva = e.rva as u64;
        let file_offset = e.offset.map(|o| o as u64);
        let patched = file_offset
            .map(|o| o as usize)
            .and_then(|o| bytes.get(o..(o + 6).min(bytes.len())))
            .and_then(crate::patch::detect_x86_patch)
            .map(|s| s.to_string());
        out.push(PeExport {
            name: name.to_string(),
            rva,
            file_offset,
            patched,
        });
    }
    out.sort_by(|a, b| a.rva.cmp(&b.rva).then_with(|| a.name.cmp(&b.name)));
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> &'static [u8] {
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/forge.dll"
        ))
    }

    #[test]
    fn errors_on_non_pe() {
        assert!(analyze_pe(b"not a PE file at all").is_err());
        assert!(analyze_pe(&[0u8; 8]).is_err());
    }

    #[test]
    fn analyzes_real_dll() {
        let p = analyze_pe(fixture()).unwrap();
        assert!(p.is_dll);
        assert!(p.is_64bit);
        assert!(!p.is_dotnet);
        assert!(p.sections.iter().any(|s| s == ".text"));
    }

    #[test]
    fn lists_sections_with_exec_flag() {
        let secs = pe_sections(fixture()).unwrap();
        assert!(!secs.is_empty());
        let text = secs.iter().find(|s| s.name == ".text").unwrap();
        assert!(text.flags.ends_with('X'));
        assert!(text.raw_size > 0);
    }

    #[test]
    fn lists_exports_and_detects_return_one() {
        let ex = pe_exports(fixture()).unwrap();
        // forge_true = `return 1;` → mov eax,1; ret = ReturnTrue şablonuyla aynı.
        let t = ex.iter().find(|e| e.name == "forge_true").unwrap();
        assert_eq!(t.patched.as_deref(), Some("return_true"));
        // forge_add gerçek mantık → yama değil.
        let a = ex.iter().find(|e| e.name == "forge_add").unwrap();
        assert_eq!(a.patched, None);
        assert!(a.file_offset.is_some());
    }

    #[test]
    fn pe_rva_offset_roundtrip() {
        let ex = pe_exports(fixture()).unwrap();
        let f = ex.iter().find(|e| e.name == "forge_add").unwrap();
        let off = pe_rva_to_offset(fixture(), f.rva).unwrap().unwrap();
        assert_eq!(off, f.file_offset.unwrap());
        let back = pe_offset_to_rva(fixture(), off).unwrap().unwrap();
        assert_eq!(back, f.rva);
    }

    #[test]
    fn machine_and_subsystem_names() {
        assert_eq!(machine_name(0x8664), "x86-64");
        assert_eq!(machine_name(0x014c), "x86 (32-bit)");
        assert_eq!(subsystem_name(2), "Windows GUI");
        assert_eq!(subsystem_name(3), "Windows Console");
    }

    #[test]
    fn compiler_detection() {
        assert_eq!(detect_compiler(true, &[], &[]), ".NET / Mono (managed)");
        assert_eq!(
            detect_compiler(false, &[], &["VCRUNTIME140.dll".into()]),
            "MSVC (Visual C++)"
        );
        assert_eq!(
            detect_compiler(false, &[".CRT$XCU".into()], &["msvcrt.dll".into()]),
            "MinGW / GCC"
        );
    }
}
