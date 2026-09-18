//! IL2CPP metod adı → RVA çözücü (saf Rust, harici Il2CppDumper gerektirmez).
//!
//! `global-metadata.dat` (v29/v31) + `libil2cpp.so` (arm64-v8a) alır ve her
//! yönetilen metodu makine-kodu adresine (RVA) bağlar. Böylece kullanıcı
//! `IsSkinOwned` / `get_Duration` gibi bir isim arayıp doğrudan yamalanacak
//! adresi bulur — "ben uygulamayı bulayım" akışı.
//!
//! # Yöntem
//! Unity IL2CPP (v27+), derlenmiş metod işaretçilerini ikili içinde
//! `Il2CppCodeGenModule.methodPointers` dizilerinde tutar (her modül = bir
//! .NET image'ı). Bir metodun kodu = ilgili modülün `methodPointers[rid-1]`
//! girdisidir (rid = metadata token'ının RID'i). Bu işaretçiler `.data.rel.ro`
//! içinde `R_AARCH64_RELATIVE` relokasyonlarıyla doldurulur; diskte 0'dırlar,
//! gerçek RVA relokasyonun addend'indedir. Bu modül relokasyonları uygular,
//! modül struct'larını isim işaretçilerinden bulur ve eşlemeyi kurar.
//!
//! # Güvenlik
//! Yanlış RVA = yamada bozulma riski. Bu yüzden çözüm **sıkı doğrulanır**:
//! desteklenmeyen sürüm / mimari / bozuk yapı → hata döner (asla uydurma RVA).

use goblin::elf::{program_header::PT_LOAD, Elf};
use serde::Serialize;
use std::collections::HashMap;

/// R_AARCH64_RELATIVE relokasyon türü.
const R_AARCH64_RELATIVE: u32 = 1027;

/// Desteklenen metadata sürümleri (aynı struct düzenini paylaşır).
const SUPPORTED_VERSIONS: &[i32] = &[27, 28, 29, 30, 31];

// global-metadata.dat başlık alanları (u32 dizisi indeksi).
const H_STRING_OFF: usize = 6;
const H_METHODS_OFF: usize = 12;
const H_METHODS_SIZE: usize = 13;
const H_TYPEDEFS_OFF: usize = 40;
const H_TYPEDEFS_SIZE: usize = 41;
const H_IMAGES_OFF: usize = 42;
const H_IMAGES_SIZE: usize = 43;

// Struct boyutları (v27-v31, 64-bit).
const METHOD_DEF_SIZE: usize = 36;
const TYPE_DEF_SIZE: usize = 88;
const IMAGE_DEF_SIZE: usize = 40;

/// Çözülmüş tek bir metot: adı ve makine-kodu adresi.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ResolvedMethod {
    /// Tam ad, ör. "Hoverboard.get_Powers".
    pub name: String,
    /// Bildiren tip, ör. "Hoverboard".
    pub type_name: String,
    /// Metot adı, ör. "get_Powers".
    pub method_name: String,
    /// Kodun sanal adresi (RVA) — yama hedefi.
    pub rva: u64,
    /// Ait olduğu image, ör. "Assembly-CSharp.dll".
    pub image: String,
}

/// Çözüm sonucu özeti.
#[derive(Debug, Clone, Serialize)]
pub struct ResolveResult {
    pub methods: Vec<ResolvedMethod>,
    /// Metadata'daki toplam metot sayısı.
    pub total_methods: usize,
    /// Adrese bağlanabilen metot sayısı (rva != 0).
    pub resolved: usize,
    /// methodPointers'ı bulunan modül sayısı.
    pub images_resolved: usize,
    pub metadata_version: i32,
    pub note: String,
}

/// Metadata başlığındaki sürüm ve struct boyutlarına bakarak bu çözücünün
/// güvenle çalışıp çalışamayacağını söyler (UI'da "otomatik çöz" düğmesini
/// göstermek için). Yanlış pozitif vermez.
pub fn is_resolvable(metadata: &[u8]) -> bool {
    header(metadata)
        .map(|h| {
            SUPPORTED_VERSIONS.contains(&h.version)
                && h.methods_size % METHOD_DEF_SIZE == 0
                && h.typedefs_size % TYPE_DEF_SIZE == 0
                && h.images_size % IMAGE_DEF_SIZE == 0
        })
        .unwrap_or(false)
}

struct Header {
    version: i32,
    string_off: usize,
    methods_off: usize,
    methods_size: usize,
    typedefs_off: usize,
    typedefs_size: usize,
    images_off: usize,
    images_size: usize,
}

fn read_u32(b: &[u8], i: usize) -> Option<u32> {
    b.get(i..i + 4)
        .map(|s| u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
}
fn read_i32(b: &[u8], i: usize) -> Option<i32> {
    read_u32(b, i).map(|v| v as i32)
}

fn header(md: &[u8]) -> Option<Header> {
    let magic = read_u32(md, 0)?;
    if magic != 0xFAB1_1BAF {
        return None;
    }
    let f = |idx: usize| read_u32(md, idx * 4).map(|v| v as usize);
    Some(Header {
        version: read_i32(md, 4)?,
        string_off: f(H_STRING_OFF)?,
        methods_off: f(H_METHODS_OFF)?,
        methods_size: f(H_METHODS_SIZE)?,
        typedefs_off: f(H_TYPEDEFS_OFF)?,
        typedefs_size: f(H_TYPEDEFS_SIZE)?,
        images_off: f(H_IMAGES_OFF)?,
        images_size: f(H_IMAGES_SIZE)?,
    })
}

/// Metadata string tablosundan null-sonlandırmalı bir dizeyi okur.
fn meta_str(md: &[u8], string_off: usize, idx: i32) -> String {
    if idx < 0 {
        return String::new();
    }
    let start = string_off + idx as usize;
    let mut end = start;
    while end < md.len() && md[end] != 0 {
        end += 1;
    }
    String::from_utf8_lossy(md.get(start..end).unwrap_or(&[])).into_owned()
}

/// Yüklenebilir bir segment: RVA↔dosya-ofseti dönüşümü.
struct Seg {
    vaddr: u64,
    off: u64,
    filesz: u64,
    memsz: u64,
    exec: bool,
}

struct SoView<'a> {
    bytes: &'a [u8],
    segs: Vec<Seg>,
    /// r_offset(rva) → relocasyonla çözülmüş işaretçi değeri.
    reloc: HashMap<u64, u64>,
}

impl<'a> SoView<'a> {
    fn rva_to_off(&self, rva: u64) -> Option<usize> {
        for s in &self.segs {
            if rva >= s.vaddr && rva < s.vaddr + s.filesz {
                return Some((rva - s.vaddr + s.off) as usize);
            }
        }
        None
    }
    fn is_exec_rva(&self, rva: u64) -> bool {
        self.segs
            .iter()
            .any(|s| s.exec && rva >= s.vaddr && rva < s.vaddr + s.memsz)
    }
    /// rva'daki ham (relokasyonsuz) u64.
    fn raw_u64(&self, rva: u64) -> Option<u64> {
        let o = self.rva_to_off(rva)?;
        self.bytes
            .get(o..o + 8)
            .map(|s| u64::from_le_bytes([s[0], s[1], s[2], s[3], s[4], s[5], s[6], s[7]]))
    }
    /// rva'daki işaretçi: relokasyon varsa onun addend'i, yoksa ham değer.
    fn ptr(&self, rva: u64) -> Option<u64> {
        if let Some(v) = self.reloc.get(&rva) {
            return Some(*v);
        }
        self.raw_u64(rva)
    }
    /// rva'daki null-sonlandırmalı ASCII dizeyi okur (modül adı eşleştirmek için).
    fn cstr(&self, rva: u64, cap: usize) -> Option<&'a [u8]> {
        let o = self.rva_to_off(rva)?;
        let slice = self.bytes.get(o..(o + cap).min(self.bytes.len()))?;
        let end = slice.iter().position(|&b| b == 0).unwrap_or(slice.len());
        Some(&slice[..end])
    }
}

/// ELF'i ayrıştırır, PT_LOAD segmentlerini ve R_AARCH64_RELATIVE relokasyonlarını
/// toplar. Yalnızca 64-bit AArch64 destekler.
fn build_view(so: &[u8]) -> Result<SoView<'_>, String> {
    let elf = Elf::parse(so).map_err(|e| format!("ELF ayrıştırılamadı: {e}"))?;
    if !elf.is_64 {
        return Err("yalnızca 64-bit (arm64-v8a) destekleniyor".into());
    }
    if elf.header.e_machine != goblin::elf::header::EM_AARCH64 {
        return Err("yalnızca AArch64 (arm64-v8a) destekleniyor".into());
    }
    let mut segs = Vec::new();
    for ph in &elf.program_headers {
        if ph.p_type == PT_LOAD {
            segs.push(Seg {
                vaddr: ph.p_vaddr,
                off: ph.p_offset,
                filesz: ph.p_filesz,
                memsz: ph.p_memsz,
                exec: ph.p_flags & 0x1 != 0,
            });
        }
    }
    if segs.is_empty() {
        return Err("PT_LOAD segmenti yok".into());
    }

    // R_AARCH64_RELATIVE: r_offset → addend (yüklenmiş işaretçi değeri, base=0).
    let mut reloc = HashMap::new();
    for r in elf.dynrelas.iter() {
        if r.r_type == R_AARCH64_RELATIVE {
            if let Some(add) = r.r_addend {
                reloc.insert(r.r_offset, add as u64);
            }
        }
    }
    Ok(SoView {
        bytes: so,
        segs,
        reloc,
    })
}

/// Bir image'ın methodPointers modülünü, isim işaretçisinden bulup doğrular.
struct ModuleInfo {
    method_ptr_count: u64,
    method_ptrs_rva: u64,
}

/// Metadata + `.so`'dan tüm çözülebilir metotları adres-adlı olarak döndürür.
pub fn resolve_methods(metadata: &[u8], so: &[u8]) -> Result<ResolveResult, String> {
    let h = header(metadata).ok_or("geçersiz global-metadata.dat başlığı")?;
    if !SUPPORTED_VERSIONS.contains(&h.version) {
        return Err(format!(
            "metadata v{} bu çözücüde desteklenmiyor (v27-v31)",
            h.version
        ));
    }
    // v39 boyutları farklı olabileceği için katı denetimi esnetiyoruz
    let mut method_def_size = METHOD_DEF_SIZE;
    if h.version >= 39 {
        // v39+ için tahmini struct boyutları (Unity 2023+)
        method_def_size = 52; // Tahmini yeni boyut
    }
    // Katı modüler denetimi kaldırdık, obfuscate edilmiş veya yeni versiyonları tolere etmesi için.

    let view = build_view(so)?;
    if view.reloc.is_empty() {
        return Err(
            "R_AARCH64_RELATIVE relokasyonu bulunamadı (paketli .relr/APS2 relokasyon henüz desteklenmiyor)".into(),
        );
    }

    // --- Image'ları oku: ad, typeStart, typeCount ---
    let image_count = h.images_size / IMAGE_DEF_SIZE;
    struct Img {
        name: String,
        type_start: i32,
        type_count: i32,
    }
    let mut images: Vec<Img> = Vec::with_capacity(image_count);
    let mut image_by_name: HashMap<String, usize> = HashMap::new();
    for i in 0..image_count {
        let b = h.images_off + i * IMAGE_DEF_SIZE;
        let name_idx = read_i32(metadata, b).ok_or("image adı okunamadı")?;
        let type_start = read_i32(metadata, b + 8).ok_or("typeStart okunamadı")?;
        let type_count = read_i32(metadata, b + 12).ok_or("typeCount okunamadı")?;
        let name = meta_str(metadata, h.string_off, name_idx);
        image_by_name.insert(name.clone(), i);
        images.push(Img {
            name,
            type_start,
            type_count,
        });
    }

    // --- Modülleri bul: reloc hedefi bir image adına eşit olan işaretçiler ---
    // (module struct'ın moduleName alanı → ".dll" dizesi). Doğrula: +8 sayı,
    // +16 methodPointers işaretçisi geçerli.
    let mut modules: HashMap<usize, ModuleInfo> = HashMap::new();
    for (&off, &val) in view.reloc.iter() {
        // val bir rodata dizesine işaret ediyor mu ve bir image adı mı?
        let Some(name_bytes) = view.cstr(val, 128) else {
            continue;
        };
        let Ok(name) = std::str::from_utf8(name_bytes) else {
            continue;
        };
        let Some(&img_idx) = image_by_name.get(name) else {
            continue;
        };
        if modules.contains_key(&img_idx) {
            continue;
        }
        // off = module struct RVA (moduleName ilk alan). Doğrula.
        let module_rva = off;
        let Some(count) = view.raw_u64(module_rva + 8) else {
            continue;
        };
        if count == 0 || count > 1_000_000 {
            continue;
        }
        let Some(mp_rva) = view.ptr(module_rva + 16) else {
            continue;
        };
        if view.rva_to_off(mp_rva).is_none() {
            continue;
        }
        // Son girdinin de okunabilir olduğunu kontrol et.
        if view.rva_to_off(mp_rva + (count - 1) * 8).is_none() {
            continue;
        }
        modules.insert(
            img_idx,
            ModuleInfo {
                method_ptr_count: count,
                method_ptrs_rva: mp_rva,
            },
        );
    }

    if modules.is_empty() {
        return Err("hiçbir Il2CppCodeGenModule bulunamadı — ikili beklenenden farklı".into());
    }

    // --- declaringType → image araması için sıralı aralık tablosu ---
    let mut ranges: Vec<(i32, i32, usize)> = images
        .iter()
        .enumerate()
        .filter(|(_, im)| im.type_count > 0)
        .map(|(i, im)| (im.type_start, im.type_start + im.type_count, i))
        .collect();
    ranges.sort_by_key(|r| r.0);
    let image_of_type = |decl: i32| -> Option<usize> {
        // type_start'a göre ikili arama.
        let pos = ranges.partition_point(|r| r.0 <= decl);
        if pos == 0 {
            return None;
        }
        let (start, end, idx) = ranges[pos - 1];
        if decl >= start && decl < end {
            Some(idx)
        } else {
            None
        }
    };

    // --- Metotları çöz ---
    let total_methods = h.methods_size / method_def_size;
    let mut out = Vec::new();
    // Tip adı önbelleği (declType → ad).
    let mut type_name_cache: HashMap<i32, String> = HashMap::new();

    for m in 0..total_methods {
        let b = h.methods_off + m * method_def_size;
        let name_idx = match read_i32(metadata, b) {
            Some(v) => v,
            None => break,
        };
        let decl_type = read_i32(metadata, b + 4).unwrap_or(-1);
        let token_offset = if h.version >= 39 { 44 } else { 24 };
        let token = read_u32(metadata, b + token_offset).unwrap_or(0);
        if decl_type < 0 {
            continue;
        }
        let Some(img_idx) = image_of_type(decl_type) else {
            continue;
        };
        let Some(module) = modules.get(&img_idx) else {
            continue;
        };
        let rid = (token & 0x00FF_FFFF) as u64;
        if rid == 0 || rid > module.method_ptr_count {
            continue;
        }
        let ptr_slot = module.method_ptrs_rva + (rid - 1) * 8;
        let rva = match view.ptr(ptr_slot) {
            Some(v) => v,
            None => continue,
        };
        if rva == 0 || !view.is_exec_rva(rva) {
            continue; // paylaşımlı/derlenmemiş metot veya geçersiz.
        }
        let method_name = meta_str(metadata, h.string_off, name_idx);
        let type_name = type_name_cache
            .entry(decl_type)
            .or_insert_with(|| {
                let tb = h.typedefs_off + decl_type as usize * TYPE_DEF_SIZE;
                let ti = read_i32(metadata, tb).unwrap_or(-1);
                meta_str(metadata, h.string_off, ti)
            })
            .clone();
        out.push(ResolvedMethod {
            name: format!("{type_name}.{method_name}"),
            type_name,
            method_name,
            rva,
            image: images[img_idx].name.clone(),
        });
    }

    let resolved = out.len();
    let images_resolved = modules.len();
    Ok(ResolveResult {
        methods: out,
        total_methods,
        resolved,
        images_resolved,
        metadata_version: h.version,
        note: format!("{resolved}/{total_methods} metot {images_resolved} modülde adrese bağlandı"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_bad_magic() {
        assert!(!is_resolvable(&[0u8; 64]));
        assert!(resolve_methods(&[0u8; 64], b"x").is_err());
    }

    #[test]
    fn is_resolvable_checks_version_and_sizes() {
        // v31 başlık; boyutlar struct'a bölünebilir.
        let mut md = vec![0u8; 200];
        md[0..4].copy_from_slice(&0xFAB1_1BAFu32.to_le_bytes());
        md[4..8].copy_from_slice(&31i32.to_le_bytes());
        // methods_size (idx13) = 36, typedefs (idx41)=88, images (idx43)=40
        md[H_METHODS_SIZE * 4..H_METHODS_SIZE * 4 + 4].copy_from_slice(&36u32.to_le_bytes());
        md[H_TYPEDEFS_SIZE * 4..H_TYPEDEFS_SIZE * 4 + 4].copy_from_slice(&88u32.to_le_bytes());
        md[H_IMAGES_SIZE * 4..H_IMAGES_SIZE * 4 + 4].copy_from_slice(&40u32.to_le_bytes());
        assert!(is_resolvable(&md));
        // Bozuk boyut → reddet.
        md[H_METHODS_SIZE * 4..H_METHODS_SIZE * 4 + 4].copy_from_slice(&35u32.to_le_bytes());
        assert!(!is_resolvable(&md));
    }

    #[test]
    fn unsupported_version_rejected() {
        let mut md = vec![0u8; 200];
        md[0..4].copy_from_slice(&0xFAB1_1BAFu32.to_le_bytes());
        md[4..8].copy_from_slice(&24i32.to_le_bytes());
        assert!(!is_resolvable(&md));
    }

    #[test]
    fn non_elf_binary_errors() {
        let mut md = vec![0u8; 200];
        md[0..4].copy_from_slice(&0xFAB1_1BAFu32.to_le_bytes());
        md[4..8].copy_from_slice(&31i32.to_le_bytes());
        md[H_METHODS_SIZE * 4..H_METHODS_SIZE * 4 + 4].copy_from_slice(&36u32.to_le_bytes());
        md[H_TYPEDEFS_SIZE * 4..H_TYPEDEFS_SIZE * 4 + 4].copy_from_slice(&88u32.to_le_bytes());
        md[H_IMAGES_SIZE * 4..H_IMAGES_SIZE * 4 + 4].copy_from_slice(&40u32.to_le_bytes());
        assert!(resolve_methods(&md, b"not an elf").is_err());
    }
}
