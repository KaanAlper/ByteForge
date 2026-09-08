//! Bellek tarayıcı (Cheat Engine tarzı) — çoklu platform (`crate::mem`).
//!
//! Süreç listeleme, kesin + fuzzy/bilinmeyen tarama (oturum durumu backend'de
//! tutulur), daraltma (next scan), okuma ve yazma. Tüm OS etkileşimi
//! `crate::mem::ProcessMemory` trait'i üzerinden — Linux/Windows/macOS.

use crate::commands::ApiError;
use crate::mem::{self, ProcessMemory};
use byteforge_core::memscan::{
    decode_num, encode_value, format_value, is_scannable, matches, Compare, ValueType,
};
use serde::Serialize;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Mutex, OnceLock};

/// UI'ye önizlemede döndürülen en fazla adres.
const PREVIEW: usize = 300;
/// Açık liste olarak tutulacak en fazla aday (bunun üstü kırpılır).
const MATERIALIZE_MAX: usize = 400_000;
/// Bilinmeyen (snapshot) tarama için en fazla toplam bayt (~512 MB).
const SNAPSHOT_MAX: u64 = 512 * 1024 * 1024;
/// Bölge okuma parça boyutu (8'in katı — hizalı slot'lar bölünmez).
const CHUNK: usize = 4 * 1024 * 1024;

/// Çalışan bir süreç (UI için).
#[derive(Serialize)]
pub struct ProcInfo {
    pub pid: u32,
    pub name: String,
}

/// Çalışan süreçleri listeler (platforma göre).
#[tauri::command]
pub fn list_processes() -> Result<Vec<ProcInfo>, ApiError> {
    let procs = mem::list_processes().map_err(|e| ApiError::Io {
        message: e.to_string(),
    })?;
    Ok(procs
        .into_iter()
        .map(|p| ProcInfo {
            pid: p.pid,
            name: p.name,
        })
        .collect())
}

/// Bir tarama oturumunun aday verisi.
enum SessionData {
    /// Materyalize aday kümesi: her adres için son okunan baytlar.
    List {
        addrs: Vec<u64>,
        last: Vec<Vec<u8>>,
    },
    /// Bilinmeyen (fuzzy) küme: taranan bölgelerin anlık kopyaları (base, bayt).
    Snapshot { regions: Vec<(u64, Vec<u8>)> },
}

/// Backend'de tutulan tarama oturumu.
struct Session {
    pid: u32,
    ty: ValueType,
    data: SessionData,
    count: usize,
}

fn sessions() -> &'static Mutex<HashMap<u32, Session>> {
    static S: OnceLock<Mutex<HashMap<u32, Session>>> = OnceLock::new();
    S.get_or_init(|| Mutex::new(HashMap::new()))
}

fn next_session_id() -> u32 {
    static N: AtomicU32 = AtomicU32::new(1);
    N.fetch_add(1, Ordering::Relaxed)
}

/// Adres + insan-okunur değer (önizleme satırı).
#[derive(Serialize)]
pub struct AddrValue {
    pub address: u64,
    pub value: String,
}

/// Tarama özeti (yeni + daraltma ortak dönüşü).
#[derive(Serialize)]
pub struct ScanSummary {
    pub session: u32,
    pub total: usize,
    pub truncated: bool,
    pub scanned_regions: usize,
    pub preview: Vec<AddrValue>,
}

fn open(pid: u32) -> Result<Box<dyn ProcessMemory>, ApiError> {
    mem::open(pid).map_err(|e| ApiError::ProcessFailed {
        message: e.to_string(),
    })
}

/// Bir bölgeyi canlı okur (parça parça), hizalı slot'lar bölünmeden tek Vec'e.
fn read_region(mem: &dyn ProcessMemory, start: u64, end: u64, width: usize) -> Vec<u8> {
    let mut out = Vec::new();
    let mut base = start;
    while base < end {
        let want = ((end - base) as usize).min(CHUNK);
        let mut buf = vec![0u8; want];
        match mem.read_at(base, &mut buf) {
            Ok(0) => break,
            Ok(n) => {
                // Slot bölünmesin: width'in katına yuvarla.
                let usable = (n / width) * width;
                if usable == 0 {
                    break;
                }
                out.extend_from_slice(&buf[..usable]);
                base += usable as u64;
            }
            Err(_) => break, // bu bölge okunamadı
        }
    }
    out
}

/// Değeri (metin) ilgili türe göre bayt + sayı olarak çözer.
fn parse_operand(value: &str, ty: ValueType) -> Option<(Vec<u8>, f64)> {
    let bytes = encode_value(value, ty)?;
    let num = decode_num(&bytes, ty)?;
    Some((bytes, num))
}

fn build_preview(addrs: &[u64], vals: &[Vec<u8>], ty: ValueType) -> Vec<AddrValue> {
    addrs
        .iter()
        .zip(vals.iter())
        .take(PREVIEW)
        .map(|(&address, b)| AddrValue {
            address,
            value: format_value(b, ty),
        })
        .collect()
}

/// Yeni tarama başlatır. `compare`: exact/greater/less/between/not_equal →
/// materyalize liste; unknown → bilinmeyen (snapshot). Göreli türler
/// (increased vb.) yalnızca daraltmada geçerlidir; ilk taramada unknown kullanın.
#[tauri::command]
pub fn scan_new(
    pid: u32,
    ty: ValueType,
    compare: Compare,
    value: String,
    value2: Option<String>,
) -> Result<ScanSummary, ApiError> {
    if compare.needs_previous() {
        return Err(ApiError::InvalidPath {
            message: "bu karşılaştırma önceki değeri gerektirir — ilk tarama için \
                      'bilinmeyen' seçin, sonra daraltın"
                .into(),
        });
    }
    let m = open(pid)?;
    let regions: Vec<_> = m
        .regions()
        .map_err(|e| ApiError::ProcessFailed {
            message: e.to_string(),
        })?
        .into_iter()
        .filter(is_scannable)
        .collect();
    let width = ty.width();

    // Operand(lar)ı çöz (unknown hariç).
    let (needle, operand) = if compare == Compare::Unknown {
        (Vec::new(), 0.0)
    } else {
        parse_operand(&value, ty).ok_or_else(|| ApiError::InvalidPath {
            message: "değer bu tür için ayrıştırılamadı".into(),
        })?
    };
    let operand2 = value2
        .as_deref()
        .and_then(|v| parse_operand(v, ty))
        .map(|(_, n)| n)
        .unwrap_or(0.0);

    let scanned_regions = regions.len();

    if compare == Compare::Unknown {
        // Bilinmeyen: bölgeleri anlık görüntüle (toplam bayt sınırlı).
        let total_bytes: u64 = regions.iter().map(|r| r.size()).sum();
        if total_bytes > SNAPSHOT_MAX {
            return Err(ApiError::ProcessFailed {
                message: format!(
                    "süreç çok büyük ({} MB taranabilir) — bilinmeyen tarama yerine \
                     önce kesin bir değerle tarayın",
                    total_bytes / (1024 * 1024)
                ),
            });
        }
        let mut snaps = Vec::new();
        let mut count = 0usize;
        for r in &regions {
            let bytes = read_region(m.as_ref(), r.start, r.end, width);
            count += bytes.len() / width;
            if !bytes.is_empty() {
                snaps.push((r.start, bytes));
            }
        }
        let id = next_session_id();
        // Önizleme: ilk birkaç slot'un mevcut değeri.
        let mut preview = Vec::new();
        'p: for (base, bytes) in &snaps {
            let mut off = 0;
            while off + width <= bytes.len() {
                preview.push(AddrValue {
                    address: base + off as u64,
                    value: format_value(&bytes[off..off + width], ty),
                });
                if preview.len() >= PREVIEW {
                    break 'p;
                }
                off += width;
            }
        }
        sessions().lock().unwrap().insert(
            id,
            Session {
                pid,
                ty,
                data: SessionData::Snapshot { regions: snaps },
                count,
            },
        );
        return Ok(ScanSummary {
            session: id,
            total: count,
            truncated: false,
            scanned_regions,
            preview,
        });
    }

    // Kesin/eşitsizlik: hizalı slot'ları tara, eşleşenleri materyalize et.
    let mut addrs = Vec::new();
    let mut last = Vec::new();
    let mut total = 0usize;
    let mut truncated = false;
    'outer: for r in &regions {
        let bytes = read_region(m.as_ref(), r.start, r.end, width);
        let mut off = 0;
        while off + width <= bytes.len() {
            let slot = &bytes[off..off + width];
            let hit = if compare == Compare::Exact {
                slot == needle.as_slice()
            } else {
                matches(slot, None, ty, compare, operand, operand2)
            };
            if hit {
                total += 1;
                if addrs.len() < MATERIALIZE_MAX {
                    addrs.push(r.start + off as u64);
                    last.push(slot.to_vec());
                } else {
                    truncated = true;
                    break 'outer;
                }
            }
            off += width;
        }
    }
    let preview = build_preview(&addrs, &last, ty);
    let id = next_session_id();
    sessions().lock().unwrap().insert(
        id,
        Session {
            pid,
            ty,
            data: SessionData::List { addrs, last },
            count: total,
        },
    );
    Ok(ScanSummary {
        session: id,
        total,
        truncated,
        scanned_regions,
        preview,
    })
}

/// Mevcut oturumu yeni karşılaştırmaya göre daraltır (next scan).
#[tauri::command]
pub fn scan_next(
    session: u32,
    compare: Compare,
    value: Option<String>,
    value2: Option<String>,
) -> Result<ScanSummary, ApiError> {
    let mut guard = sessions().lock().unwrap();
    let s = guard.get_mut(&session).ok_or_else(|| ApiError::InvalidPath {
        message: "tarama oturumu bulunamadı (yeniden tarayın)".into(),
    })?;
    let ty = s.ty;
    let width = ty.width();
    let pid = s.pid;
    drop(guard); // aç sırasında kilidi bırak
    let m = open(pid)?;

    let (operand, needle) = match value.as_deref().and_then(|v| parse_operand(v, ty)) {
        Some((b, n)) => (n, Some(b)),
        None => (0.0, None),
    };
    let operand2 = value2
        .as_deref()
        .and_then(|v| parse_operand(v, ty))
        .map(|(_, n)| n)
        .unwrap_or(0.0);
    if compare == Compare::Exact && needle.is_none() {
        return Err(ApiError::InvalidPath {
            message: "kesin karşılaştırma için geçerli bir değer girin".into(),
        });
    }

    let mut guard = sessions().lock().unwrap();
    let s = guard.get_mut(&session).ok_or_else(|| ApiError::InvalidPath {
        message: "tarama oturumu bulunamadı".into(),
    })?;

    let mut new_addrs = Vec::new();
    let mut new_last = Vec::new();
    let mut truncated = false;

    match &s.data {
        SessionData::List { addrs, last } => {
            let mut cur = vec![0u8; width];
            for (i, &addr) in addrs.iter().enumerate() {
                if m.read_at(addr, &mut cur).map(|n| n == width).unwrap_or(false) {
                    let old = last.get(i).map(|v| v.as_slice());
                    let hit = if compare == Compare::Exact {
                        Some(cur.as_slice()) == needle.as_deref()
                    } else {
                        matches(&cur, old, ty, compare, operand, operand2)
                    };
                    if hit {
                        new_addrs.push(addr);
                        new_last.push(cur.clone());
                    }
                }
            }
        }
        SessionData::Snapshot { regions } => {
            // Anlık kopyayla canlı okumayı slot-slot karşılaştır.
            'outer: for (base, old_bytes) in regions {
                let live = read_region(m.as_ref(), *base, *base + old_bytes.len() as u64, width);
                let n = old_bytes.len().min(live.len());
                let mut off = 0;
                while off + width <= n {
                    let old = &old_bytes[off..off + width];
                    let new = &live[off..off + width];
                    let hit = if compare == Compare::Exact {
                        Some(new) == needle.as_deref()
                    } else {
                        matches(new, Some(old), ty, compare, operand, operand2)
                    };
                    if hit {
                        if new_addrs.len() >= MATERIALIZE_MAX {
                            truncated = true;
                            break 'outer;
                        }
                        new_addrs.push(base + off as u64);
                        new_last.push(new.to_vec());
                    }
                    off += width;
                }
            }
        }
    }

    let total = new_addrs.len();
    let preview = build_preview(&new_addrs, &new_last, ty);
    s.data = SessionData::List {
        addrs: new_addrs,
        last: new_last,
    };
    s.count = total;
    Ok(ScanSummary {
        session,
        total,
        truncated,
        scanned_regions: 0,
        preview,
    })
}

/// Oturumun ilk PREVIEW adresinin güncel değerlerini yeniden okur.
#[tauri::command]
pub fn scan_read(session: u32) -> Result<Vec<AddrValue>, ApiError> {
    let guard = sessions().lock().unwrap();
    let s = guard.get(&session).ok_or_else(|| ApiError::InvalidPath {
        message: "tarama oturumu bulunamadı".into(),
    })?;
    let ty = s.ty;
    let width = ty.width();
    let pid = s.pid;
    let addrs: Vec<u64> = match &s.data {
        SessionData::List { addrs, .. } => addrs.iter().take(PREVIEW).copied().collect(),
        SessionData::Snapshot { regions } => {
            let mut out = Vec::new();
            'p: for (base, bytes) in regions {
                let mut off = 0;
                while off + width <= bytes.len() {
                    out.push(base + off as u64);
                    if out.len() >= PREVIEW {
                        break 'p;
                    }
                    off += width;
                }
            }
            out
        }
    };
    drop(guard);
    let m = open(pid)?;
    let mut out = Vec::new();
    let mut buf = vec![0u8; width];
    for a in addrs {
        let v = if m.read_at(a, &mut buf).map(|n| n == width).unwrap_or(false) {
            format_value(&buf, ty)
        } else {
            "?".into()
        };
        out.push(AddrValue { address: a, value: v });
    }
    Ok(out)
}

/// Bir oturumu serbest bırakır.
#[tauri::command]
pub fn scan_clear(session: u32) {
    sessions().lock().unwrap().remove(&session);
}

/// Bir adrese bir değeri yazar (canlı düzenleme).
#[tauri::command]
pub fn write_memory(pid: u32, address: u64, value: String, ty: ValueType) -> Result<(), ApiError> {
    let bytes = encode_value(&value, ty).ok_or_else(|| ApiError::InvalidPath {
        message: "değer bu tür için ayrıştırılamadı".into(),
    })?;
    let m = open(pid)?;
    let n = m.write_at(address, &bytes).map_err(|e| ApiError::ProcessFailed {
        message: format!("bellek yazılamadı @0x{address:x}: {e}"),
    })?;
    if n != bytes.len() {
        return Err(ApiError::ProcessFailed {
            message: format!("kısmi yazma: {n}/{} bayt", bytes.len()),
        });
    }
    crate::console::log(format!(
        "bellek yazıldı: pid={pid} @0x{address:x} = {value} ({} bayt)",
        bytes.len()
    ));
    Ok(())
}

/// Bir adresteki mevcut değeri okur.
#[tauri::command]
pub fn read_memory(pid: u32, address: u64, ty: ValueType) -> Result<String, ApiError> {
    let m = open(pid)?;
    let width = ty.width();
    let mut buf = vec![0u8; width];
    let n = m.read_at(address, &mut buf).map_err(|e| ApiError::ProcessFailed {
        message: format!("bellek okunamadı @0x{address:x}: {e}"),
    })?;
    if n != width {
        return Err(ApiError::ProcessFailed {
            message: "eksik okuma".into(),
        });
    }
    Ok(format_value(&buf, ty))
}

/// ptrace_scope durumunu döndürür (Linux; diğer platformlarda bilgilendirme).
#[tauri::command]
pub fn ptrace_scope() -> String {
    #[cfg(target_os = "linux")]
    {
        std::fs::read_to_string("/proc/sys/kernel/yama/ptrace_scope")
            .map(|s| s.trim().to_string())
            .unwrap_or_else(|_| "bilinmiyor".into())
    }
    #[cfg(not(target_os = "linux"))]
    {
        "yok".into()
    }
}

#[cfg(all(test, target_os = "linux"))]
mod live_tests {
    //! Canlı uçtan-uca test: bir alt süreçte bilinen bir i64 değeri tutulur,
    //! `crate::mem` üzerinden taranır (hizalı), bulunur, üzerine yazılır ve
    //! geri okunarak doğrulanır. ptrace izni yoksa test atlanır (panik değil).

    use super::*;
    use std::io::BufRead;
    use std::process::{Command, Stdio};

    const MAGIC: i64 = 0x0000_07BA_DF00_D5A7; // ayırt edici değer

    #[test]
    fn live_scan_and_write_child() {
        // Bilinen değeri bellekte tutan bir alt süreç başlat.
        let py = format!(
            "import ctypes,sys,time\n\
             v=ctypes.c_int64({MAGIC})\n\
             sys.stdout.write('READY\\n'); sys.stdout.flush()\n\
             time.sleep(30)\n"
        );
        let mut child = match Command::new("python3")
            .arg("-c")
            .arg(&py)
            .stdout(Stdio::piped())
            .spawn()
        {
            Ok(c) => c,
            Err(_) => {
                eprintln!("python3 yok — canlı test atlandı");
                return;
            }
        };
        // READY bekle
        {
            let out = child.stdout.take().unwrap();
            let mut r = std::io::BufReader::new(out);
            let mut line = String::new();
            let _ = r.read_line(&mut line);
            assert!(line.contains("READY"), "alt süreç hazır olmadı");
        }
        let pid = child.id();

        let result = (|| -> Result<(), String> {
            let m = mem::open(pid).map_err(|e| format!("open: {e:?}"))?;
            let needle = encode_value(&MAGIC.to_string(), ValueType::I64).unwrap();
            let regions: Vec<_> = m
                .regions()
                .map_err(|e| format!("regions: {e:?}"))?
                .into_iter()
                .filter(is_scannable)
                .collect();
            // Hizalı tara: MAGIC'i içeren en az bir adres bul.
            let mut found = None;
            'outer: for r in &regions {
                let bytes = read_region(m.as_ref(), r.start, r.end, 8);
                let mut off = 0;
                while off + 8 <= bytes.len() {
                    if &bytes[off..off + 8] == needle.as_slice() {
                        found = Some(r.start + off as u64);
                        break 'outer;
                    }
                    off += 8;
                }
            }
            let addr = found.ok_or("MAGIC bellekte bulunamadı")?;

            // Üzerine yaz ve geri oku.
            let newv = encode_value("42", ValueType::I64).unwrap();
            m.write_at(addr, &newv).map_err(|e| format!("write: {e:?}"))?;
            let mut back = vec![0u8; 8];
            m.read_at(addr, &mut back).map_err(|e| format!("read: {e:?}"))?;
            if back != newv {
                return Err(format!("yazma doğrulanamadı: {back:?}"));
            }
            Ok(())
        })();

        let _ = child.kill();
        let _ = child.wait();

        match result {
            Ok(()) => { /* başarı */ }
            Err(e) if e.contains("ptrace") || e.contains("PermissionDenied") || e.contains("open:") => {
                eprintln!("canlı test ptrace izni yok — atlandı: {e}");
            }
            Err(e) => panic!("canlı bellek testi başarısız: {e}"),
        }
    }
}
