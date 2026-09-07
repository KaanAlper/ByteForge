use byteforge_core::diff::ArchiveDiff;
use byteforge_core::elf::SoSymbol;
use byteforge_core::error::CoreError;
use byteforge_core::hexedit::{parse_hex, search_bytes};
use byteforge_core::patch::{
    hex_dump, template_bytes, template_bytes_arch, write_patched_file, Arch, PatchTemplate,
};
use byteforge_core::pe::PeProfile;
use byteforge_core::profile::AppProfile;
use byteforge_core::tools::ToolReport;
use serde::Serialize;
use std::io::{Read, Seek, SeekFrom};
use std::path::PathBuf;

/// IPC sınırını geçebilen, türü korunmuş hata. Frontend `kind`'e göre
/// davranabilir (ör. "dosya yok" vs "bozuk arşiv").
#[derive(Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ApiError {
    InvalidPath { message: String },
    NotAFile { message: String },
    TooLarge { message: String },
    BadExtension { message: String },
    Io { message: String },
    Archive { message: String },
    Manifest { message: String },
    EntryTooLarge { message: String },
    Elf { message: String },
    PatchOutOfBounds { message: String },
    ToolMissing { message: String },
    ProcessFailed { message: String },
    SmaliPatch { message: String },
    Hex { message: String },
    UnknownFormat { message: String },
}

impl From<CoreError> for ApiError {
    fn from(e: CoreError) -> Self {
        let message = e.to_string();
        match e {
            CoreError::Io(_) => ApiError::Io { message },
            CoreError::Archive(_) => ApiError::Archive { message },
            CoreError::Manifest(_) => ApiError::Manifest { message },
            CoreError::EntryTooLarge(_) => ApiError::EntryTooLarge { message },
            CoreError::Elf(_) => ApiError::Elf { message },
            CoreError::PatchOutOfBounds { .. } => ApiError::PatchOutOfBounds { message },
            CoreError::SmaliPatch(_) => ApiError::SmaliPatch { message },
            CoreError::Hex(_) => ApiError::Hex { message },
            CoreError::UnknownFormat => ApiError::UnknownFormat { message },
        }
    }
}

/// Makul üst sınır — bozuk/kötü niyetli girdilere karşı savunma-derinliği.
const MAX_FILE_BYTES: u64 = 4 * 1024 * 1024 * 1024; // 4 GiB

/// Webview IPC'den gelen bir yolu canonicalize + normal-dosya + boyut + uzantı
/// doğrulamasından geçirir. `allowed_exts` küçük harf uzantı listesidir.
pub(crate) fn validate_file(path: &str, allowed_exts: &[&str]) -> Result<PathBuf, ApiError> {
    let canonical = PathBuf::from(path)
        .canonicalize()
        .map_err(|e| ApiError::InvalidPath {
            message: format!("yol çözümlenemedi: {e}"),
        })?;

    let meta = canonical.metadata().map_err(|e| ApiError::InvalidPath {
        message: format!("dosya bilgisi okunamadı: {e}"),
    })?;

    if !meta.is_file() {
        return Err(ApiError::NotAFile {
            message: "yalnızca normal dosyalar işlenebilir (aygıt/FIFO değil)".into(),
        });
    }
    if meta.len() > MAX_FILE_BYTES {
        return Err(ApiError::TooLarge {
            message: format!("dosya çok büyük (> {MAX_FILE_BYTES} bayt)"),
        });
    }

    let ext_ok = canonical
        .extension()
        .and_then(|x| x.to_str())
        .map(|x| x.to_ascii_lowercase())
        .is_some_and(|x| allowed_exts.contains(&x.as_str()));
    if !ext_ok {
        return Err(ApiError::BadExtension {
            message: format!("beklenen uzantı: {}", allowed_exts.join(" / ")),
        });
    }

    Ok(canonical)
}

pub(crate) const APK_EXTS: &[&str] = &["apk", "xapk", "apks", "zip", "jar"];
const SO_EXTS: &[&str] = &["so"];
const PE_EXTS: &[&str] = &["exe", "dll", "sys", "ocx", "efi"];

/// Bir Windows PE (EXE/DLL) dosyasını analiz eder — mimari, .NET/native,
/// derleyici tahmini, import edilen DLL'ler ve önerilen müdahale hattı.
#[tauri::command]
pub fn profile_pe(path: String) -> Result<PeProfile, ApiError> {
    let canonical = validate_file(&path, PE_EXTS)?;
    let bytes = std::fs::read(&canonical).map_err(|e| ApiError::Io {
        message: e.to_string(),
    })?;
    byteforge_core::pe::analyze_pe(&bytes).map_err(ApiError::from)
}

/// .NET/Mono sembol dökümü sonucu.
#[derive(Debug, Serialize)]
pub struct MonoDump {
    pub count: usize,
    pub symbols: Vec<String>,
}

/// Managed bir PE'nin (.NET / Unity Mono) sembol adlarını (#Strings) dökümler.
#[tauri::command]
pub fn dotnet_symbols(path: String) -> Result<MonoDump, ApiError> {
    let canonical = validate_file(&path, PE_EXTS)?;
    let bytes = std::fs::read(&canonical).map_err(|e| ApiError::Io {
        message: e.to_string(),
    })?;
    let all = byteforge_core::mono::dotnet_strings(&bytes).map_err(ApiError::from)?;
    let count = all.len();
    Ok(MonoDump {
        symbols: all.into_iter().take(8000).collect(),
        count,
    })
}

/// Bir PE'nin anti-tamper/anti-debug göstergelerini tarar (import + section).
#[tauri::command]
pub fn antitamper_pe(path: String) -> Result<Vec<byteforge_core::antitamper::TamperHit>, ApiError> {
    let canonical = validate_file(&path, PE_EXTS)?;
    let bytes = std::fs::read(&canonical).map_err(|e| ApiError::Io {
        message: e.to_string(),
    })?;
    let mut tokens = byteforge_core::pe::pe_import_names(&bytes).unwrap_or_default();
    if let Ok(p) = byteforge_core::pe::analyze_pe(&bytes) {
        tokens.extend(p.imported_dlls);
        tokens.extend(p.sections);
    }
    Ok(byteforge_core::antitamper::scan_antitamper(&tokens))
}

/// Bir `.so`'nun anti-tamper/anti-debug göstergelerini tarar (semboller).
#[tauri::command]
pub fn antitamper_so(path: String) -> Result<Vec<byteforge_core::antitamper::TamperHit>, ApiError> {
    let canonical = validate_file(&path, SO_EXTS)?;
    let bytes = std::fs::read(&canonical).map_err(|e| ApiError::Io {
        message: e.to_string(),
    })?;
    let syms = byteforge_core::list_symbols(&bytes).map_err(ApiError::from)?;
    let tokens: Vec<String> = syms.into_iter().map(|s| s.name).collect();
    Ok(byteforge_core::antitamper::scan_antitamper(&tokens))
}

/// PE section'larını döndürür (RVA↔ofset haritası).
#[tauri::command]
pub fn pe_sections(path: String) -> Result<Vec<byteforge_core::pe::PeSection>, ApiError> {
    let canonical = validate_file(&path, PE_EXTS)?;
    let bytes = std::fs::read(&canonical).map_err(|e| ApiError::Io {
        message: e.to_string(),
    })?;
    byteforge_core::pe::pe_sections(&bytes).map_err(ApiError::from)
}

/// PE export tablosu (adlı fonksiyonlar) + yama tespiti (MODLU).
#[tauri::command]
pub fn pe_exports(path: String) -> Result<Vec<byteforge_core::pe::PeExport>, ApiError> {
    let canonical = validate_file(&path, PE_EXTS)?;
    let bytes = std::fs::read(&canonical).map_err(|e| ApiError::Io {
        message: e.to_string(),
    })?;
    byteforge_core::pe::pe_exports(&bytes).map_err(ApiError::from)
}

/// PE RVA'yı dosya ofsetine çevirir.
#[tauri::command]
pub fn pe_rva_to_offset(path: String, rva: u64) -> Result<Option<u64>, ApiError> {
    let canonical = validate_file(&path, PE_EXTS)?;
    let bytes = std::fs::read(&canonical).map_err(|e| ApiError::Io {
        message: e.to_string(),
    })?;
    byteforge_core::pe::pe_rva_to_offset(&bytes, rva).map_err(ApiError::from)
}

/// Dosya ofsetini PE RVA'sına çevirir.
#[tauri::command]
pub fn pe_offset_to_rva(path: String, offset: u64) -> Result<Option<u64>, ApiError> {
    let canonical = validate_file(&path, PE_EXTS)?;
    let bytes = std::fs::read(&canonical).map_err(|e| ApiError::Io {
        message: e.to_string(),
    })?;
    byteforge_core::pe::pe_offset_to_rva(&bytes, offset).map_err(ApiError::from)
}

/// PE için x86/x64 opcode yaması önizlemesi (mevcut → yeni baytlar).
#[tauri::command]
pub fn pe_patch_preview(
    path: String,
    offset: usize,
    template: PatchTemplate,
) -> Result<PatchPreview, ApiError> {
    let canonical = validate_file(&path, PE_EXTS)?;
    let bytes = std::fs::read(&canonical).map_err(|e| ApiError::Io {
        message: e.to_string(),
    })?;
    let repl = template_bytes_arch(template, Arch::X86);
    Ok(PatchPreview {
        offset,
        current: hex_dump(&bytes, offset, repl.len()),
        replacement: hex_dump(&repl, 0, repl.len()),
    })
}

/// PE'ye x86/x64 opcode yamasını `.bak` yedeğiyle uygular. Yedek yolunu döndürür.
#[tauri::command]
pub fn apply_pe_patch(
    path: String,
    offset: usize,
    template: PatchTemplate,
) -> Result<String, ApiError> {
    let canonical = validate_file(&path, PE_EXTS)?;
    let repl = template_bytes_arch(template, Arch::X86);
    let existing = std::fs::read(&canonical).map_err(|e| ApiError::Io {
        message: e.to_string(),
    })?;
    let old_hex = hex_dump(&existing, offset, repl.len());
    let new_hex = hex_dump(&repl, 0, repl.len());
    write_patched_file(&canonical, offset, &repl).map_err(ApiError::from)?;
    crate::history::append_record(
        "pe",
        &canonical.to_string_lossy(),
        offset,
        &old_hex,
        &new_hex,
        "x86/x64 opcode yaması",
    );
    Ok(format!("{}.bak", canonical.display()))
}

#[tauri::command]
pub fn analyze_apk(path: String) -> Result<AppProfile, ApiError> {
    let canonical = validate_file(&path, APK_EXTS)?;
    byteforge_core::analyze_archive(&canonical).map_err(ApiError::from)
}

#[tauri::command]
pub fn diff_apks(path_a: String, path_b: String) -> Result<ArchiveDiff, ApiError> {
    let a = validate_file(&path_a, APK_EXTS)?;
    let b = validate_file(&path_b, APK_EXTS)?;
    byteforge_core::diff_archives(&a, &b).map_err(ApiError::from)
}

/// İki APK'nın aynı native girişi (ör. libil2cpp.so) arasındaki tek bir baytsal
/// yama — "modcu bu ofseti şöyle değiştirmiş" reçetesi.
#[derive(Debug, Serialize)]
pub struct ModPatch {
    pub offset: usize,
    pub rva: Option<u64>,
    pub old_hex: String,
    pub new_hex: String,
    pub len: usize,
    /// Bilinen şablon (return_true/return_false/nop/ret) — yeni baytlardan.
    pub template: Option<String>,
    /// İnsan-okur yorum (ör. "ReturnTrue (kilit aç)").
    pub label: String,
    /// IL2CPP çözücüyle bulunan gerçek metod adı (ör. "AvailableBoard.get_IsOwned").
    /// Yalnızca arm64 libil2cpp.so + metadata mevcutsa dolar.
    pub symbol: Option<String>,
}

fn interpret_patch(new: &[u8], template: &Option<String>) -> String {
    if let Some(t) = template {
        return match t.as_str() {
            "return_true" => "ReturnTrue — kilit/erişim aç (MOV W0,#1; RET)".into(),
            "return_false" => "ReturnFalse — kapat (MOV W0,#0; RET)".into(),
            "return_max_int" => "MAX_INT döndür".into(),
            "nop" => "NOP — komutu etkisiz kıl".into(),
            "ret" => "RET — hemen dön".into(),
            _ => t.clone(),
        };
    }
    // RET ile biten kısa yama → sabit değer döndürme (ör. MAX_INT / özel).
    if new.len() >= 4 && new[new.len() - 4..] == [0xC0, 0x03, 0x5F, 0xD6] {
        return "Sabit değer döndür (RET ile — büyük değer/MAX_INT olası)".into();
    }
    "Özel bayt yaması".into()
}

/// İki APK'nın (temiz + modlu) aynı native girişini baytsal karşılaştırıp
/// modcunun yaptığı yerinde yamaları reçete olarak çıkarır (offset+RVA+yorum).
#[tauri::command]
pub fn native_mod_recipe(
    clean: String,
    modded: String,
    entry: String,
) -> Result<Vec<ModPatch>, ApiError> {
    let c = validate_file(&clean, APK_EXTS)?;
    let m = validate_file(&modded, APK_EXTS)?;
    let ca = byteforge_core::archive::read_entry(&c, &entry).map_err(ApiError::from)?;
    let ma = byteforge_core::archive::read_entry(&m, &entry).map_err(ApiError::from)?;
    if ca.len() != ma.len() {
        return Err(ApiError::UnknownFormat {
            message: format!(
                "'{entry}' boyutları farklı ({} vs {}) — yerinde yama değil, yeniden \
                 derlenmiş olabilir; baytsal reçete çıkarılamaz",
                ca.len(),
                ma.len()
            ),
        });
    }
    let regions = byteforge_core::diff::byte_diff_regions(&ca, &ma, 4, 500);
    let is_elf = ca.starts_with(b"\x7fELF");

    // IL2CPP isim çözümü: arm64 libil2cpp.so ise, temiz APK'nın metadata'sıyla
    // her yama RVA'sını gerçek metod adına bağla (reçetede "get_IsOwned" görünsün).
    let rva_names: std::collections::HashMap<u64, String> = if is_elf
        && entry.contains("arm64")
        && entry.contains("libil2cpp")
    {
        byteforge_core::archive::list_entries(&c)
            .ok()
            .and_then(|e| e.into_iter().find(|n| n.ends_with("global-metadata.dat")))
            .and_then(|meta| byteforge_core::archive::read_entry(&c, &meta).ok())
            .and_then(|md| byteforge_core::il2cpp_resolve::resolve_methods(&md, &ca).ok())
            .map(|res| res.methods.into_iter().map(|m| (m.rva, m.name)).collect())
            .unwrap_or_default()
    } else {
        std::collections::HashMap::new()
    };

    let mut out = Vec::new();
    for r in regions {
        let template = byteforge_core::patch::detect_arm64_patch(&r.new).map(|s| s.to_string());
        let label = interpret_patch(&r.new, &template);
        let rva = if is_elf {
            byteforge_core::elf::file_offset_to_rva(&ca, r.offset as u64)
                .ok()
                .flatten()
        } else {
            None
        };
        let symbol = rva.and_then(|a| rva_names.get(&a).cloned());
        out.push(ModPatch {
            offset: r.offset,
            rva,
            old_hex: hex_dump(&r.old, 0, r.old.len()),
            new_hex: hex_dump(&r.new, 0, r.new.len()),
            len: r.old.len(),
            template,
            label,
            symbol,
        });
    }
    Ok(out)
}

/// Bir arşiv farkını insan-okur adli bulgulara çevirir (yeniden diff yapmaz).
#[tauri::command]
pub fn forensic_report(diff: ArchiveDiff) -> Vec<byteforge_core::forensic::Finding> {
    byteforge_core::forensic::forensic_report(&diff)
}

#[tauri::command]
pub fn list_so_symbols(path: String) -> Result<Vec<SoSymbol>, ApiError> {
    let canonical = validate_file(&path, SO_EXTS)?;
    let bytes = std::fs::read(&canonical).map_err(|e| ApiError::Io {
        message: e.to_string(),
    })?;
    byteforge_core::list_symbols(&bytes).map_err(ApiError::from)
}

/// Reçete uygulamada tek bir bayt yaması (offset + yeni baytlar hex).
#[derive(Debug, serde::Deserialize)]
pub struct ApplyPatch {
    pub offset: usize,
    pub new_hex: String,
}

/// Bir mod reçetesini temiz APK'ya uygular: belirtilen girişi (native .so)
/// baytsal yamalar ve APK'yı yeniden paketler (imzasız). Diğer tüm girişler
/// aynen kopyalanır. Sonuç APK'yı Dağıt sekmesinde imzalayıp kurun.
#[tauri::command]
pub fn apply_native_mod(
    clean: String,
    entry: String,
    patches: Vec<ApplyPatch>,
    out_apk: String,
) -> Result<String, ApiError> {
    use std::io::Write;
    let c = validate_file(&clean, APK_EXTS)?;

    // 1) Hedef girişi oku ve baytsal yamala.
    let mut data = byteforge_core::archive::read_entry(&c, &entry).map_err(ApiError::from)?;
    for p in &patches {
        let bytes = parse_hex(&p.new_hex).map_err(ApiError::from)?;
        let end = p
            .offset
            .checked_add(bytes.len())
            .filter(|&e| e <= data.len())
            .ok_or_else(|| ApiError::PatchOutOfBounds {
                message: format!("yama sınır dışı @0x{:x}", p.offset),
            })?;
        data[p.offset..end].copy_from_slice(&bytes);
    }

    // 2) APK'yı yeniden paketle — hedef giriş yamalı, diğerleri aynen.
    let out_path = PathBuf::from(&out_apk);
    let reader = std::fs::File::open(&c).map_err(|e| ApiError::Io {
        message: e.to_string(),
    })?;
    let mut zin = zip::ZipArchive::new(reader).map_err(|e| ApiError::Archive {
        message: e.to_string(),
    })?;
    let writer = std::fs::File::create(&out_path).map_err(|e| ApiError::Io {
        message: e.to_string(),
    })?;
    let mut zout = zip::ZipWriter::new(writer);

    for i in 0..zin.len() {
        let file = zin.by_index(i).map_err(|e| ApiError::Archive {
            message: e.to_string(),
        })?;
        let name = file.name().to_string();
        if name == entry {
            // Yamalı giriş — native lib olduğu için STORED (zipalign hizalar).
            let opts: zip::write::FileOptions<()> =
                zip::write::FileOptions::default().compression_method(zip::CompressionMethod::Stored);
            zout.start_file(&name, opts).map_err(|e| ApiError::Archive {
                message: e.to_string(),
            })?;
            zout.write_all(&data).map_err(|e| ApiError::Io {
                message: e.to_string(),
            })?;
        } else {
            // Değişmeyen girişleri ham kopyala (yeniden sıkıştırma yok).
            zout.raw_copy_file(file).map_err(|e| ApiError::Archive {
                message: e.to_string(),
            })?;
        }
    }
    zout.finish().map_err(|e| ApiError::Archive {
        message: e.to_string(),
    })?;

    Ok(format!(
        "{} ({} yama, imzasız — Dağıt sekmesinde imzalayın)",
        out_path.display(),
        patches.len()
    ))
}

/// Bir `.so`'nun yüklenebilir segmentlerini döndürür (RVA↔ofset haritası).
#[tauri::command]
pub fn so_segments(path: String) -> Result<Vec<byteforge_core::elf::ElfSegment>, ApiError> {
    let canonical = validate_file(&path, SO_EXTS)?;
    let bytes = std::fs::read(&canonical).map_err(|e| ApiError::Io {
        message: e.to_string(),
    })?;
    byteforge_core::elf::load_segments(&bytes).map_err(ApiError::from)
}

/// Bir RVA'yı (dumper/IDA adresi) `.so` içindeki dosya ofsetine çevirir
/// (yamalanacak baytın yeri). Segmente düşmüyorsa None.
#[tauri::command]
pub fn rva_to_offset(path: String, rva: u64) -> Result<Option<u64>, ApiError> {
    let canonical = validate_file(&path, SO_EXTS)?;
    let bytes = std::fs::read(&canonical).map_err(|e| ApiError::Io {
        message: e.to_string(),
    })?;
    byteforge_core::elf::rva_to_file_offset(&bytes, rva).map_err(ApiError::from)
}

/// Bir dosya ofsetini RVA'ya çevirir (hex editördeki konumu dumper adresiyle eşleştirmek için).
#[tauri::command]
pub fn offset_to_rva(path: String, offset: u64) -> Result<Option<u64>, ApiError> {
    let canonical = validate_file(&path, SO_EXTS)?;
    let bytes = std::fs::read(&canonical).map_err(|e| ApiError::Io {
        message: e.to_string(),
    })?;
    byteforge_core::elf::file_offset_to_rva(&bytes, offset).map_err(ApiError::from)
}

/// Bir yamanın uygulanmadan önceki önizlemesi: mevcut baytlar → yeni baytlar.
#[derive(Debug, Serialize)]
pub struct PatchPreview {
    pub offset: usize,
    pub current: String,
    pub replacement: String,
}

#[tauri::command]
pub fn patch_preview(
    path: String,
    offset: usize,
    template: PatchTemplate,
) -> Result<PatchPreview, ApiError> {
    let canonical = validate_file(&path, SO_EXTS)?;
    let bytes = std::fs::read(&canonical).map_err(|e| ApiError::Io {
        message: e.to_string(),
    })?;
    let repl = template_bytes(template);
    Ok(PatchPreview {
        offset,
        current: hex_dump(&bytes, offset, repl.len()),
        replacement: hex_dump(&repl, 0, repl.len()),
    })
}

/// Yamayı `.bak` yedeği alarak `.so` dosyasına yazar. Yedek yolunu döndürür.
#[tauri::command]
pub fn apply_so_patch(
    path: String,
    offset: usize,
    template: PatchTemplate,
) -> Result<String, ApiError> {
    let canonical = validate_file(&path, SO_EXTS)?;
    let repl = template_bytes(template);
    let existing = std::fs::read(&canonical).map_err(|e| ApiError::Io {
        message: e.to_string(),
    })?;
    let old_hex = hex_dump(&existing, offset, repl.len());
    let new_hex = hex_dump(&repl, 0, repl.len());
    write_patched_file(&canonical, offset, &repl).map_err(ApiError::from)?;
    crate::history::append_record(
        "so",
        &canonical.to_string_lossy(),
        offset,
        &old_hex,
        &new_hex,
        "native opcode yaması",
    );
    Ok(format!("{}.bak", canonical.display()))
}

/// Uzantıdan bağımsız dosya doğrulaması (hex editör herhangi bir ikili dosyada çalışır).
pub(crate) fn validate_any_file(path: &str) -> Result<PathBuf, ApiError> {
    let canonical = PathBuf::from(path)
        .canonicalize()
        .map_err(|e| ApiError::InvalidPath {
            message: format!("yol çözümlenemedi: {e}"),
        })?;
    let meta = canonical.metadata().map_err(|e| ApiError::InvalidPath {
        message: format!("dosya bilgisi okunamadı: {e}"),
    })?;
    if !meta.is_file() {
        return Err(ApiError::NotAFile {
            message: "yalnızca normal dosyalar açılabilir".into(),
        });
    }
    if meta.len() > MAX_FILE_BYTES {
        return Err(ApiError::TooLarge {
            message: format!("dosya çok büyük (> {MAX_FILE_BYTES} bayt)"),
        });
    }
    Ok(canonical)
}

const HEX_CHUNK_MAX: usize = 64 * 1024;

#[derive(Debug, Serialize)]
pub struct HexChunk {
    pub total: u64,
    pub offset: usize,
    pub bytes: Vec<u8>,
}

/// Bir dosyadan `offset`'ten itibaren en fazla 64KB ham bayt okur (hex editör görünümü).
#[tauri::command]
pub fn read_hex(path: String, offset: usize, len: usize) -> Result<HexChunk, ApiError> {
    let canonical = validate_any_file(&path)?;
    let total = canonical.metadata().map(|m| m.len()).unwrap_or(0);
    let mut file = std::fs::File::open(&canonical).map_err(|e| ApiError::Io {
        message: e.to_string(),
    })?;
    file.seek(SeekFrom::Start(offset as u64))
        .map_err(|e| ApiError::Io {
            message: e.to_string(),
        })?;
    let mut buf = vec![0u8; len.min(HEX_CHUNK_MAX)];
    let n = file.read(&mut buf).map_err(|e| ApiError::Io {
        message: e.to_string(),
    })?;
    buf.truncate(n);
    Ok(HexChunk {
        total,
        offset,
        bytes: buf,
    })
}

/// Bir dosyada hex kalıbı arar; eşleşen ofsetleri döndürür (en fazla 1000).
#[tauri::command]
pub fn search_hex(path: String, pattern: String) -> Result<Vec<usize>, ApiError> {
    let canonical = validate_any_file(&path)?;
    let needle = parse_hex(&pattern).map_err(ApiError::from)?;
    let bytes = std::fs::read(&canonical).map_err(|e| ApiError::Io {
        message: e.to_string(),
    })?;
    let mut offsets = search_bytes(&bytes, &needle);
    offsets.truncate(1000);
    Ok(offsets)
}

/// Bir dosyaya `offset`'ten itibaren hex baytları yazar (.bak yedekli). Yedek yolunu döndürür.
#[tauri::command]
pub fn write_hex(path: String, offset: usize, hex: String) -> Result<String, ApiError> {
    let canonical = validate_any_file(&path)?;
    let bytes = parse_hex(&hex).map_err(ApiError::from)?;
    let existing = std::fs::read(&canonical).map_err(|e| ApiError::Io {
        message: e.to_string(),
    })?;
    let old_hex = hex_dump(&existing, offset, bytes.len());
    let new_hex = hex_dump(&bytes, 0, bytes.len());
    write_patched_file(&canonical, offset, &bytes).map_err(ApiError::from)?;
    crate::history::append_record(
        "hex",
        &canonical.to_string_lossy(),
        offset,
        &old_hex,
        &new_hex,
        "hex düzenleme",
    );
    Ok(format!("{}.bak", canonical.display()))
}

#[tauri::command]
pub fn check_tools() -> ToolReport {
    byteforge_core::tools::check_tools()
}

/// Bir ofsetin fonksiyon başlangıcı olup olmadığını denetler (çökme önleme).
/// `arch`: "arm64" (native .so) | "x86" (PE/ELF x86-64).
#[tauri::command]
pub fn check_prologue(
    path: String,
    offset: usize,
    arch: String,
) -> Result<byteforge_core::prologue::PrologueCheck, ApiError> {
    let canonical = validate_any_file(&path)?;
    let mut file = std::fs::File::open(&canonical).map_err(|e| ApiError::Io {
        message: e.to_string(),
    })?;
    file.seek(SeekFrom::Start(offset as u64))
        .map_err(|e| ApiError::Io {
            message: e.to_string(),
        })?;
    let mut head = [0u8; 12];
    let n = file.read(&mut head).map_err(|e| ApiError::Io {
        message: e.to_string(),
    })?;
    let head = &head[..n];
    Ok(match arch.as_str() {
        "x86" => byteforge_core::prologue::check_x86(head),
        _ => byteforge_core::prologue::check_arm64(head),
    })
}

/// Puanlanmış akıllı hedef adayı (IL2CPP/Mono isim listesi için).
#[tauri::command]
pub fn rank_symbols(names: Vec<String>, top: usize) -> Vec<byteforge_core::heuristic::Score> {
    byteforge_core::heuristic::rank_names(&names, top.min(200))
}

/// Bir APK'nın Markdown analiz raporunu üretir (profil + paketleyici + hat).
#[tauri::command]
pub fn report_apk(path: String) -> Result<String, ApiError> {
    let canonical = validate_file(&path, APK_EXTS)?;
    let profile = byteforge_core::analyze_archive(&canonical).map_err(ApiError::from)?;
    Ok(byteforge_core::report::app_report_md(&profile))
}

/// Bir PE'nin Markdown analiz raporunu üretir (profil + entropi + paketleyici).
#[tauri::command]
pub fn report_pe(path: String) -> Result<String, ApiError> {
    let canonical = validate_file(&path, PE_EXTS)?;
    let bytes = std::fs::read(&canonical).map_err(|e| ApiError::Io {
        message: e.to_string(),
    })?;
    let profile = byteforge_core::pe::analyze_pe(&bytes).map_err(ApiError::from)?;
    Ok(byteforge_core::report::pe_report_md(&profile))
}

/// Bir opcode şablonunun belirtilen mimarideki ham baytlarını onaltılık dize
/// olarak döndürür (UI'de gösterim/kopyalama için). Yama uygulamaz.
#[tauri::command]
pub fn opcode_hex(template: PatchTemplate, arch: Arch) -> String {
    let bytes = template_bytes_arch(template, arch);
    hex_dump(&bytes, 0, bytes.len())
}

/// Gizlenmiş bir dizeyi çözmeyi dener (Base64/hex/XOR/ROT13).
#[tauri::command]
pub fn deobfuscate_string(input: String) -> Vec<byteforge_core::deobf::Candidate> {
    byteforge_core::deobf::deobfuscate(&input)
}

/// Hazır Frida scriptleri (SSL/root bypass, imza izleyici).
#[tauri::command]
pub fn frida_scripts() -> Vec<byteforge_core::frida::FridaScript> {
    byteforge_core::frida::builtin_scripts()
}

/// Verilen sınıf.metod için parametrik Frida method tracer scripti üretir.
#[tauri::command]
pub fn frida_tracer(class: String, method: String) -> byteforge_core::frida::FridaScript {
    byteforge_core::frida::tracer_script(&class, &method)
}

/// Yamalanmış bir `.so` dosyasını (Atölye'de "Uygula" ile değiştirilmiş cache
/// dosyası) kaynak APK'ya geri paketler — o girişi yamalı `.so` ile değiştirir,
/// kalan her şeyi aynen kopyalar. Sonuç imzasızdır; Dağıt'ta imzalayıp kurun.
/// "Uygula" ile "Dağıt" arasındaki köprü: yamalar artık telefona giden APK'ya girer.
#[tauri::command]
pub fn package_patched_so(
    apk: String,
    entry: String,
    so_path: String,
    out_apk: String,
) -> Result<String, ApiError> {
    use std::io::Write;
    let a = validate_file(&apk, APK_EXTS)?;
    let so = validate_file(&so_path, SO_EXTS)?;
    let data = std::fs::read(&so).map_err(|e| ApiError::Io {
        message: e.to_string(),
    })?;

    // Girişin APK'da var olduğunu doğrula (yoksa yanlış ABI/isim).
    let entries = byteforge_core::archive::list_entries(&a).map_err(ApiError::from)?;
    if !entries.iter().any(|e| e == &entry) {
        return Err(ApiError::UnknownFormat {
            message: format!("APK'da '{entry}' girişi yok — ABI/yol yanlış olabilir"),
        });
    }

    let out_path = PathBuf::from(&out_apk);
    let reader = std::fs::File::open(&a).map_err(|e| ApiError::Io {
        message: e.to_string(),
    })?;
    let mut zin = zip::ZipArchive::new(reader).map_err(|e| ApiError::Archive {
        message: e.to_string(),
    })?;
    let writer = std::fs::File::create(&out_path).map_err(|e| ApiError::Io {
        message: e.to_string(),
    })?;
    let mut zout = zip::ZipWriter::new(writer);

    for i in 0..zin.len() {
        let file = zin.by_index(i).map_err(|e| ApiError::Archive {
            message: e.to_string(),
        })?;
        let name = file.name().to_string();
        if name == entry {
            // Native lib → STORED; Dağıt'ta zipalign -p 4 sayfa hizalar.
            let opts: zip::write::FileOptions<()> =
                zip::write::FileOptions::default().compression_method(zip::CompressionMethod::Stored);
            zout.start_file(&name, opts).map_err(|e| ApiError::Archive {
                message: e.to_string(),
            })?;
            zout.write_all(&data).map_err(|e| ApiError::Io {
                message: e.to_string(),
            })?;
        } else {
            zout.raw_copy_file(file).map_err(|e| ApiError::Archive {
                message: e.to_string(),
            })?;
        }
    }
    zout.finish().map_err(|e| ApiError::Archive {
        message: e.to_string(),
    })?;

    Ok(out_path.to_string_lossy().into_owned())
}

/// Bir APK'yı, verilen giriş→bayt eşlemesindeki girişleri değiştirerek yeniden
/// paketler; kalan tüm girişleri ham kopyalar. Native .so'lar STORED yazılır
/// (Dağıt zipalign -p 4 ile sayfa hizalar). Deploy'un mod-farkındalıklı imzası kullanır.
pub(crate) fn repack_replacing(
    apk: &std::path::Path,
    replace: &std::collections::HashMap<String, Vec<u8>>,
    out: &std::path::Path,
) -> Result<(), ApiError> {
    use std::io::Write;
    let reader = std::fs::File::open(apk).map_err(|e| ApiError::Io {
        message: e.to_string(),
    })?;
    let mut zin = zip::ZipArchive::new(reader).map_err(|e| ApiError::Archive {
        message: e.to_string(),
    })?;
    let writer = std::fs::File::create(out).map_err(|e| ApiError::Io {
        message: e.to_string(),
    })?;
    let mut zout = zip::ZipWriter::new(writer);
    for i in 0..zin.len() {
        let file = zin.by_index(i).map_err(|e| ApiError::Archive {
            message: e.to_string(),
        })?;
        let name = file.name().to_string();
        if let Some(data) = replace.get(&name) {
            let opts: zip::write::FileOptions<()> =
                zip::write::FileOptions::default().compression_method(zip::CompressionMethod::Stored);
            zout.start_file(&name, opts).map_err(|e| ApiError::Archive {
                message: e.to_string(),
            })?;
            zout.write_all(data).map_err(|e| ApiError::Io {
                message: e.to_string(),
            })?;
        } else {
            zout.raw_copy_file(file).map_err(|e| ApiError::Archive {
                message: e.to_string(),
            })?;
        }
    }
    zout.finish().map_err(|e| ApiError::Archive {
        message: e.to_string(),
    })?;
    Ok(())
}

/// Bir ikilinin (`.so`/`.exe`/`.dll`) belirtilen ofsetinden itibaren `len` baytı
/// disassemble eder (native Rust, yaxpeax). `base` gösterilecek başlangıç adresi
/// (genelde RVA). arch: "arm64" | "x86".
#[tauri::command]
pub fn disassemble_range(
    path: String,
    offset: usize,
    len: usize,
    base: u64,
    arch: String,
) -> Result<Vec<byteforge_core::disasm::DisasmLine>, ApiError> {
    // .so veya PE — ikisini de kabul et.
    let canonical = validate_file(&path, SO_EXTS)
        .or_else(|_| validate_file(&path, PE_EXTS))?;
    let bytes = std::fs::read(&canonical).map_err(|e| ApiError::Io {
        message: e.to_string(),
    })?;
    let end = offset.saturating_add(len).min(bytes.len());
    if offset >= bytes.len() {
        return Ok(Vec::new());
    }
    Ok(byteforge_core::disasm::disassemble(&bytes[offset..end], base, &arch))
}

/// Bir ikiliyi (apk/so/exe/dll/dex…) gömülü Yara kural setiyle tarar
/// (native Rust yara-x). Packer/kripto/anti-debug/root/emülatör/Frida imzaları.
#[tauri::command]
pub fn yara_scan(path: String) -> Result<Vec<byteforge_core::yara::YaraMatch>, ApiError> {
    let canonical = validate_file(&path, APK_EXTS)
        .or_else(|_| validate_file(&path, SO_EXTS))
        .or_else(|_| validate_file(&path, PE_EXTS))?;
    let bytes = std::fs::read(&canonical).map_err(|e| ApiError::Io {
        message: e.to_string(),
    })?;
    byteforge_core::yara::scan(&bytes).map_err(|e| ApiError::ProcessFailed { message: e })
}

/// Bir `.so`'daki fonksiyonu (ofsetten itibaren) native mini-decompiler ile
/// C benzeri psödokoda çevirir (Ghidra-modeli, basit fonksiyonlar).
#[tauri::command]
pub fn decompile_function(
    path: String,
    offset: usize,
    name: String,
) -> Result<byteforge_core::decompile::Decompiled, ApiError> {
    let canonical = validate_file(&path, SO_EXTS)?;
    let bytes = std::fs::read(&canonical).map_err(|e| ApiError::Io {
        message: e.to_string(),
    })?;
    let end = offset.saturating_add(160).min(bytes.len());
    if offset >= bytes.len() {
        return Err(ApiError::PatchOutOfBounds {
            message: "ofset dosya dışında".into(),
        });
    }
    Ok(byteforge_core::decompile::decompile_arm64(&bytes[offset..end], &name))
}
