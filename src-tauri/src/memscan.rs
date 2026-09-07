//! Bellek tarayıcı (Cheat Engine tarzı) — Linux /proc tabanlı.
//!
//! Süreç listeleme, canlı bellek tarama (ilk + daraltma), okuma ve yazma.
//! /proc/<pid>/mem erişimi ptrace izni ister; izin yoksa net hata döner
//! (sudo ile çalıştırın ya da `ptrace_scope`'u gevşetin).

use crate::commands::ApiError;
use byteforge_core::memscan::{encode_value, find_all, is_scannable, parse_maps, ValueType};
use serde::Serialize;
use std::os::unix::fs::FileExt;

/// Tarama başına en fazla adres (UI'yi boğmamak için).
const MAX_MATCHES: usize = 5000;
/// Bölge okuma parça boyutu.
const CHUNK: usize = 4 * 1024 * 1024;

/// Çalışan bir süreç.
#[derive(Serialize)]
pub struct ProcInfo {
    pub pid: u32,
    pub name: String,
}

/// Çalışan süreçleri listeler (/proc taranarak).
#[tauri::command]
pub fn list_processes() -> Result<Vec<ProcInfo>, ApiError> {
    let mut out = Vec::new();
    let entries = std::fs::read_dir("/proc").map_err(|e| ApiError::Io {
        message: e.to_string(),
    })?;
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(pid_str) = name.to_str() else {
            continue;
        };
        let Ok(pid) = pid_str.parse::<u32>() else {
            continue;
        };
        let comm_path = format!("/proc/{pid}/comm");
        let Ok(comm) = std::fs::read_to_string(&comm_path) else {
            continue;
        };
        let comm = comm.trim().to_string();
        if comm.is_empty() {
            continue;
        }
        out.push(ProcInfo { pid, name: comm });
    }
    out.sort_by_key(|p| p.name.to_lowercase());
    Ok(out)
}

/// Tarama sonucu.
#[derive(Serialize)]
pub struct ScanResult {
    pub addresses: Vec<u64>,
    pub total: usize,
    pub truncated: bool,
    pub scanned_regions: usize,
}

fn open_mem(pid: u32, write: bool) -> Result<std::fs::File, ApiError> {
    let path = format!("/proc/{pid}/mem");
    let mut opts = std::fs::OpenOptions::new();
    opts.read(true).write(write);
    opts.open(&path).map_err(|e| ApiError::ProcessFailed {
        message: format!(
            "/proc/{pid}/mem açılamadı: {e} — ptrace izni gerekli \
             (sudo ile çalıştırın veya ptrace_scope=0)"
        ),
    })
}

fn read_maps(pid: u32) -> Result<Vec<byteforge_core::memscan::MemRegion>, ApiError> {
    let text =
        std::fs::read_to_string(format!("/proc/{pid}/maps")).map_err(|e| ApiError::Io {
            message: format!("/proc/{pid}/maps okunamadı: {e}"),
        })?;
    Ok(parse_maps(&text))
}

/// Bir süreçte bir değerin ilk taramasını yapar; eşleşen adresleri döndürür.
#[tauri::command]
pub fn scan_process(pid: u32, value: String, ty: ValueType) -> Result<ScanResult, ApiError> {
    let needle = encode_value(&value, ty).ok_or_else(|| ApiError::InvalidPath {
        message: "değer bu tür için ayrıştırılamadı".into(),
    })?;
    let regions = read_maps(pid)?;
    let mem = open_mem(pid, false)?;

    let mut addresses = Vec::new();
    let mut total = 0usize;
    let mut scanned = 0usize;
    let mut truncated = false;

    'outer: for r in regions.iter().filter(|r| is_scannable(r)) {
        scanned += 1;
        let mut base = r.start;
        let overlap = needle.len().saturating_sub(1) as u64;
        while base < r.end {
            let want = ((r.end - base) as usize).min(CHUNK);
            let mut buf = vec![0u8; want];
            match mem.read_at(&mut buf, base) {
                Ok(0) => break,
                Ok(n) => {
                    buf.truncate(n);
                    for off in find_all(&buf, &needle) {
                        total += 1;
                        if addresses.len() < MAX_MATCHES {
                            addresses.push(base + off as u64);
                        } else {
                            truncated = true;
                        }
                    }
                    // Parça sınırındaki değeri kaçırmamak için overlap kadar geri.
                    let advance = (n as u64).saturating_sub(overlap).max(1);
                    base += advance;
                }
                Err(_) => break, // bu bölge okunamadı, atla
            }
            if total > MAX_MATCHES * 4 {
                truncated = true;
                break 'outer;
            }
        }
    }

    Ok(ScanResult {
        addresses,
        total,
        truncated,
        scanned_regions: scanned,
    })
}

/// Önceki tarama adreslerini yeni değere göre daraltır (next scan).
#[tauri::command]
pub fn refine_scan(
    pid: u32,
    addresses: Vec<u64>,
    value: String,
    ty: ValueType,
) -> Result<ScanResult, ApiError> {
    let needle = encode_value(&value, ty).ok_or_else(|| ApiError::InvalidPath {
        message: "değer bu tür için ayrıştırılamadı".into(),
    })?;
    let mem = open_mem(pid, false)?;
    let w = ty.width();
    let mut kept = Vec::new();
    for addr in addresses {
        let mut buf = vec![0u8; w];
        if mem.read_at(&mut buf, addr).map(|n| n == w).unwrap_or(false) && buf == needle {
            kept.push(addr);
        }
        if kept.len() >= MAX_MATCHES {
            break;
        }
    }
    let total = kept.len();
    Ok(ScanResult {
        addresses: kept,
        total,
        truncated: false,
        scanned_regions: 0,
    })
}

/// Bir adrese bir değeri yazar (canlı düzenleme).
#[tauri::command]
pub fn write_memory(pid: u32, address: u64, value: String, ty: ValueType) -> Result<(), ApiError> {
    let bytes = encode_value(&value, ty).ok_or_else(|| ApiError::InvalidPath {
        message: "değer bu tür için ayrıştırılamadı".into(),
    })?;
    let mem = open_mem(pid, true)?;
    let n = mem.write_at(&bytes, address).map_err(|e| ApiError::ProcessFailed {
        message: format!("bellek yazılamadı @0x{address:x}: {e} (ptrace/yazma izni gerekli)"),
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

/// Bir adresteki mevcut değeri okur (izleme/doğrulama için).
#[tauri::command]
pub fn read_memory(pid: u32, address: u64, ty: ValueType) -> Result<String, ApiError> {
    let mem = open_mem(pid, false)?;
    let w = ty.width();
    let mut buf = vec![0u8; w];
    let n = mem.read_at(&mut buf, address).map_err(|e| ApiError::ProcessFailed {
        message: format!("bellek okunamadı @0x{address:x}: {e}"),
    })?;
    if n != w {
        return Err(ApiError::ProcessFailed {
            message: "eksik okuma".into(),
        });
    }
    let s = match ty {
        ValueType::U8 => buf[0].to_string(),
        ValueType::I32 => i32::from_le_bytes(buf[..4].try_into().unwrap()).to_string(),
        ValueType::I64 => i64::from_le_bytes(buf[..8].try_into().unwrap()).to_string(),
        ValueType::F32 => f32::from_le_bytes(buf[..4].try_into().unwrap()).to_string(),
        ValueType::F64 => f64::from_le_bytes(buf[..8].try_into().unwrap()).to_string(),
    };
    Ok(s)
}

/// ptrace_scope durumunu döndürür (UI'de kullanıcıyı bilgilendirmek için).
#[tauri::command]
pub fn ptrace_scope() -> String {
    std::fs::read_to_string("/proc/sys/kernel/yama/ptrace_scope")
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|_| "bilinmiyor".into())
}
