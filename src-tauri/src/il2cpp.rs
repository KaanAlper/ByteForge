use crate::commands::{validate_file, ApiError, APK_EXTS};
use byteforge_core::archive::{list_entries, read_entry};
use byteforge_core::il2cpp::{dump_metadata_symbols, parse_metadata_header};
use byteforge_core::il2cpp_resolve::{resolve_methods, ResolvedMethod};
use serde::Serialize;
use std::path::{Path, PathBuf};

#[derive(Serialize)]
pub struct Il2CppResult {
    pub out_dir: String,
    pub metadata_valid: bool,
    pub metadata_version: i32,
    pub metadata_size: u64,
    pub has_binary: bool,
    pub binary_arch: Vec<String>,
    pub note: String,
}

/// Bir IL2CPP APK'sından global-metadata.dat ve libil2cpp.so dosyalarını
/// `~/.cache/byteforge/il2cpp/<ad>` altına çıkarır ve metadata başlığını analiz eder.
/// (Tam dump.cs üretimi harici Il2CppDumper gerektirir.)
#[tauri::command]
pub fn extract_il2cpp(path: String) -> Result<Il2CppResult, ApiError> {
    let apk = validate_file(&path, APK_EXTS)?;
    let entries = list_entries(&apk).map_err(ApiError::from)?;

    let meta_entry = entries
        .iter()
        .find(|e| e.ends_with("global-metadata.dat"))
        .cloned();
    let so_entries: Vec<String> = entries
        .iter()
        .filter(|e| e.ends_with("libil2cpp.so"))
        .cloned()
        .collect();

    let home = std::env::var("HOME").map_err(|_| ApiError::ProcessFailed {
        message: "HOME tanımlı değil".into(),
    })?;
    let stem = apk
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "app".into());
    let out_dir = Path::new(&home)
        .join(".cache")
        .join("byteforge")
        .join("il2cpp")
        .join(&stem);
    let _ = std::fs::remove_dir_all(&out_dir);
    std::fs::create_dir_all(&out_dir).map_err(|e| ApiError::Io {
        message: e.to_string(),
    })?;

    let (valid, version, size) = if let Some(m) = &meta_entry {
        let bytes = read_entry(&apk, m).map_err(ApiError::from)?;
        let info = parse_metadata_header(&bytes);
        std::fs::write(out_dir.join("global-metadata.dat"), &bytes).map_err(|e| ApiError::Io {
            message: e.to_string(),
        })?;
        (info.valid, info.version, info.size)
    } else {
        (false, 0, 0)
    };

    let mut arch = Vec::new();
    for so in &so_entries {
        let bytes = read_entry(&apk, so).map_err(ApiError::from)?;
        let abi = so
            .strip_prefix("lib/")
            .and_then(|r| r.split('/').next())
            .unwrap_or("unknown")
            .to_string();
        std::fs::write(out_dir.join(format!("libil2cpp-{abi}.so")), &bytes).map_err(|e| {
            ApiError::Io {
                message: e.to_string(),
            }
        })?;
        arch.push(abi);
    }

    Ok(Il2CppResult {
        out_dir: out_dir.to_string_lossy().into_owned(),
        metadata_valid: valid,
        metadata_version: version,
        metadata_size: size,
        has_binary: !so_entries.is_empty(),
        binary_arch: arch,
        note: "Tam dump.cs / script.json için bu dosyaları Il2CppDumper'a verin (harici araç).".into(),
    })
}

#[derive(Serialize)]
pub struct DumpResult {
    pub count: usize,
    pub symbols: Vec<String>,
    pub out_file: String,
    pub metadata_version: i32,
}

/// global-metadata.dat'tan sembol adlarını saf Rust ile çözümler (harici araç yok).
/// Aşama 1: isim çıkarma. (RVA eşleştirme = libil2cpp.so CodeRegistration parse'ı
/// sonraki aşama.)
#[tauri::command]
pub fn dump_il2cpp_symbols(path: String) -> Result<DumpResult, ApiError> {
    let apk = validate_file(&path, APK_EXTS)?;
    let entries = list_entries(&apk).map_err(ApiError::from)?;
    let meta_entry = entries
        .iter()
        .find(|e| e.ends_with("global-metadata.dat"))
        .ok_or_else(|| ApiError::UnknownFormat {
            message: "APK içinde global-metadata.dat yok (IL2CPP değil?)".into(),
        })?;
    let bytes = read_entry(&apk, meta_entry).map_err(ApiError::from)?;

    let info = parse_metadata_header(&bytes);
    if !info.valid {
        return Err(ApiError::UnknownFormat {
            message: "geçersiz global-metadata.dat başlığı".into(),
        });
    }
    let symbols = dump_metadata_symbols(&bytes);

    let home = std::env::var("HOME").map_err(|_| ApiError::ProcessFailed {
        message: "HOME tanımlı değil".into(),
    })?;
    let stem = apk
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "app".into());
    let out_dir = Path::new(&home)
        .join(".cache")
        .join("byteforge")
        .join("il2cpp")
        .join(&stem);
    std::fs::create_dir_all(&out_dir).map_err(|e| ApiError::Io {
        message: e.to_string(),
    })?;
    let out_file = out_dir.join("dump-symbols.txt");
    std::fs::write(&out_file, symbols.join("\n")).map_err(|e| ApiError::Io {
        message: e.to_string(),
    })?;

    let total_count = symbols.len();
    Ok(DumpResult {
        count: total_count,
        symbols,
        out_file: out_file.to_string_lossy().into_owned(),
        metadata_version: info.version,
    })
}

/// Ad→RVA çözümünün özeti (UI'da "otomatik çöz" durumu için).
#[derive(Serialize)]
pub struct ResolveSummary {
    pub resolved: usize,
    pub total_methods: usize,
    pub images_resolved: usize,
    pub metadata_version: i32,
    pub out_file: String,
    pub note: String,
}

/// Çıkarılmış IL2CPP dizinindeki (extract_il2cpp çıktısı) arm64 `.so`'yu bulur.
fn find_arm64_so(dir: &Path) -> Option<PathBuf> {
    let read = std::fs::read_dir(dir).ok()?;
    let mut cand: Option<PathBuf> = None;
    for e in read.flatten() {
        let p = e.path();
        let name = p.file_name()?.to_string_lossy().to_string();
        if name.starts_with("libil2cpp") && name.ends_with(".so") {
            if name.contains("arm64") {
                return Some(p);
            }
            cand.get_or_insert(p);
        }
    }
    cand
}

/// global-metadata.dat + libil2cpp(arm64).so'dan her metodu RVA'ya bağlar
/// (saf Rust, Il2CppDumper gerektirmez) ve `resolved-methods.tsv` yazar.
/// Böylece kullanıcı bir isim arayıp doğrudan yamalanacak adresi bulur.
#[tauri::command]
pub fn il2cpp_resolve(out_dir: String) -> Result<ResolveSummary, ApiError> {
    let dir = Path::new(&out_dir);
    let md_path = dir.join("global-metadata.dat");
    let md = std::fs::read(&md_path).map_err(|e| ApiError::Io {
        message: format!("global-metadata.dat okunamadı: {e}"),
    })?;
    let so_path = find_arm64_so(dir).ok_or_else(|| ApiError::UnknownFormat {
        message: "arm64-v8a libil2cpp.so bulunamadı (yalnızca arm64 desteklenir)".into(),
    })?;
    let so = std::fs::read(&so_path).map_err(|e| ApiError::Io {
        message: e.to_string(),
    })?;

    let result = resolve_methods(&md, &so).map_err(|e| ApiError::UnknownFormat { message: e })?;

    // İsme göre sıralı TSV: rva \t type \t method \t image
    let mut sorted = result.methods.clone();
    sorted.sort_by(|a, b| a.name.cmp(&b.name));
    let mut tsv = String::with_capacity(sorted.len() * 48);
    for m in &sorted {
        tsv.push_str(&format!(
            "{:x}\t{}\t{}\t{}\n",
            m.rva, m.type_name, m.method_name, m.image
        ));
    }
    let out_file = dir.join("resolved-methods.tsv");
    std::fs::write(&out_file, &tsv).map_err(|e| ApiError::Io {
        message: e.to_string(),
    })?;

    Ok(ResolveSummary {
        resolved: result.resolved,
        total_methods: result.total_methods,
        images_resolved: result.images_resolved,
        metadata_version: result.metadata_version,
        out_file: out_file.to_string_lossy().into_owned(),
        note: result.note,
    })
}

/// resolved-methods.tsv içinde isim araması (alt dize, büyük/küçük harf duyarsız).
/// UI: bir sembol yaz → RVA'lı sonuçlar → tıkla → detay panelinde adres dolu gelir.
#[tauri::command]
pub fn il2cpp_resolve_search(
    out_dir: String,
    query: String,
    limit: usize,
) -> Result<Vec<ResolvedMethod>, ApiError> {
    let tsv_path = Path::new(&out_dir).join("resolved-methods.tsv");
    let text = std::fs::read_to_string(&tsv_path).map_err(|e| ApiError::Io {
        message: format!("önce çözümleyin (resolved-methods.tsv yok): {e}"),
    })?;
    let q = query.trim().to_lowercase();
    let cap = limit.clamp(1, 2000);
    let mut out = Vec::new();
    for line in text.lines() {
        let mut it = line.split('\t');
        let (Some(rva_s), Some(ty), Some(mn), Some(img)) =
            (it.next(), it.next(), it.next(), it.next())
        else {
            continue;
        };
        let full = format!("{ty}.{mn}");
        if !q.is_empty() && !full.to_lowercase().contains(&q) {
            continue;
        }
        let rva = u64::from_str_radix(rva_s, 16).unwrap_or(0);
        out.push(ResolvedMethod {
            name: full,
            type_name: ty.to_string(),
            method_name: mn.to_string(),
            rva,
            image: img.to_string(),
        });
        if out.len() >= cap {
            break;
        }
    }
    Ok(out)
}

/// Puanlanmış + adreslenmiş hedef: sezgisel skor + çözülmüş RVA bir arada.
/// Böylece "Sezgisel Hedefler" listesi tıklanınca RVA elle girilmeden gelir.
#[derive(Serialize)]
pub struct RankedTarget {
    pub name: String, // "AvailableBoard.get_IsOwned"
    pub type_name: String,
    pub method_name: String,
    pub image: String,
    pub rva: u64,
    pub score: i32,
    pub reasons: Vec<String>,
    pub confidence: String,
    pub category: String,
    pub suggested_template: String,
    pub risky: bool,
}

/// resolved-methods.tsv'yi okur, her metodu adına göre sezgisel puanlar ve
/// en yüksek puanlıları RVA'larıyla döndürür — tek liste: skor + adres.
#[tauri::command]
pub fn il2cpp_rank_resolved(out_dir: String, top: usize) -> Result<Vec<RankedTarget>, ApiError> {
    let tsv_path = Path::new(&out_dir).join("resolved-methods.tsv");
    let text = std::fs::read_to_string(&tsv_path).map_err(|e| ApiError::Io {
        message: format!("önce çözümleyin (resolved-methods.tsv yok): {e}"),
    })?;

    let mut out: Vec<RankedTarget> = Vec::new();
    for line in text.lines() {
        let mut it = line.split('\t');
        let (Some(rva_s), Some(ty), Some(mn), Some(img)) =
            (it.next(), it.next(), it.next(), it.next())
        else {
            continue;
        };
        // İsimden bool-getter çıkarımı (rank_names ile aynı sezgi).
        let low = mn.to_lowercase();
        let inferred_bool = low.starts_with("get_is")
            || low.starts_with("get_has")
            || low.starts_with("is")
            || low.starts_with("has")
            || low.starts_with("can");
        let s = byteforge_core::heuristic::score_symbol(
            mn,
            if inferred_bool { Some(true) } else { None },
            None,
        );
        if s.excluded || s.score < 20 {
            continue;
        }
        let rva = u64::from_str_radix(rva_s, 16).unwrap_or(0);
        if rva == 0 {
            continue;
        }
        out.push(RankedTarget {
            name: format!("{ty}.{mn}"),
            type_name: ty.to_string(),
            method_name: mn.to_string(),
            image: img.to_string(),
            rva,
            score: s.score,
            reasons: s.reasons,
            confidence: s.confidence,
            category: s.category,
            suggested_template: s.suggested_template,
            risky: s.risky,
        });
    }
    // Skora göre azalan, sonra ada göre.
    out.sort_by(|a, b| b.score.cmp(&a.score).then_with(|| a.name.cmp(&b.name)));
    out.truncate(top.clamp(1, 1000));
    Ok(out)
}

/// APK yolundan otomatik: IL2CPP cache dizinini bulur, gerekiyorsa çözer ve
/// isimle arama yapar. Mod Menü editörü bunu kullanır — kullanıcı elle RVA
/// yazmak yerine metod adını arar, RVA otomatik gelir.
#[tauri::command]
pub fn il2cpp_lookup(
    apk: String,
    query: String,
    limit: usize,
) -> Result<Vec<ResolvedMethod>, ApiError> {
    let a = validate_file(&apk, APK_EXTS)?;
    let home = std::env::var("HOME").map_err(|_| ApiError::ProcessFailed {
        message: "HOME tanımlı değil".into(),
    })?;
    let stem = a
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "app".into());
    let out_dir = Path::new(&home)
        .join(".cache/byteforge/il2cpp")
        .join(&stem);
    let out_str = out_dir.to_string_lossy().into_owned();
    let tsv = out_dir.join("resolved-methods.tsv");

    // TSV yoksa: metadata + arm64 .so varsa çöz; yoksa boş dön (henüz çıkarılmamış).
    if !tsv.exists() {
        let has_md = out_dir.join("global-metadata.dat").exists();
        if has_md && find_arm64_so(&out_dir).is_some() {
            il2cpp_resolve(out_str.clone())?;
        } else {
            return Ok(Vec::new());
        }
    }
    il2cpp_resolve_search(out_str, query, limit)
}
