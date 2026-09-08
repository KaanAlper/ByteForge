//! Pointer scan (Cheat Engine "pointer scan") — dinamik bir adrese ulaşan
//! statik pointer zincirlerini bulur. Derinlik ve ofset sınırlı, bounded.
//!
//! Yaklaşım: taranabilir/okunabilir bölgelerdeki tüm pointer-boyu (8B) hizalı
//! değerleri topla (bir başka bölgeye işaret edenler aday pointer). Hedeften
//! geriye doğru BFS: bir düğüm `node` için, değeri `[node - max_offset, node]`
//! aralığında olan pointer'lar bir sonraki seviyedir (ofset = node - değer).
//! Dosya-destekli (statik) bölgede duran pointer'lar zincir kökü sayılır.

use crate::commands::ApiError;
use crate::mem::{self, ProcessMemory};
use byteforge_core::memscan::{is_scannable, MemRegion};
use serde::Serialize;

const PTR: usize = 8;
const CHUNK: usize = 4 * 1024 * 1024;
/// Toplanacak en fazla aday pointer (zaman/bellek sınırı).
const MAX_POINTERS: usize = 8_000_000;
/// Döndürülecek en fazla zincir.
const MAX_CHAINS: usize = 200;

/// Bulunan bir pointer zinciri: statik kök + ofset dizisi.
#[derive(Serialize)]
pub struct PointerChain {
    /// Kök pointer'ın bulunduğu modül/yol (varsa) — statik taban.
    pub base_module: String,
    /// Kök pointer adresi (mutlak).
    pub base_address: u64,
    /// Kök modülün başlangıcına göre ofset (statik olarak yeniden bulmak için).
    pub base_offset: u64,
    /// Uygulanacak ofset dizisi (kökten hedefe).
    pub offsets: Vec<u64>,
    /// Zincir derinliği.
    pub depth: usize,
}

fn regions_of(m: &dyn ProcessMemory) -> Result<Vec<MemRegion>, ApiError> {
    m.regions().map_err(|e| ApiError::ProcessFailed {
        message: e.to_string(),
    })
}

/// Bir adresin hangi bölgeye düştüğünü bulur.
fn region_of(regions: &[MemRegion], addr: u64) -> Option<&MemRegion> {
    regions
        .iter()
        .find(|r| addr >= r.start && addr < r.end)
}

/// Bir bölgenin statik (dosya-destekli, kalıcı taban) sayılıp sayılmadığı.
fn is_static(r: &MemRegion) -> bool {
    r.path.starts_with('/')
        || (r.path.contains('.') && !r.path.starts_with('['))
        || r.path.ends_with(".exe")
        || r.path.ends_with(".dll")
        || r.path.ends_with(".so")
}

/// Hedefe ulaşan pointer zincirlerini arar.
#[tauri::command]
pub fn pointer_scan(
    pid: u32,
    target: u64,
    max_offset: u64,
    max_depth: u32,
) -> Result<Vec<PointerChain>, ApiError> {
    let depth = max_depth.clamp(1, 4) as usize;
    let m = mem::open(pid).map_err(|e| ApiError::ProcessFailed {
        message: e.to_string(),
    })?;
    let regions = regions_of(m.as_ref())?;

    // Bellekteki tüm hizalı 8B pointer'ları topla: (konum, değer).
    // Yalnızca başka bir haritalı bölgeye işaret edenler ilgi çekicidir.
    let min_a = regions.iter().map(|r| r.start).min().unwrap_or(0);
    let max_a = regions.iter().map(|r| r.end).max().unwrap_or(0);
    let mut pointers: Vec<(u64, u64)> = Vec::new(); // (konum_adresi, işaret_ettiği)
    'collect: for r in regions.iter().filter(|r| is_scannable(r) || is_static(r)) {
        if !r.is_readable() {
            continue;
        }
        let mut base = r.start;
        while base < r.end {
            let want = ((r.end - base) as usize).min(CHUNK);
            let mut buf = vec![0u8; want];
            match m.read_at(base, &mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    let usable = (n / PTR) * PTR;
                    let mut off = 0;
                    while off + PTR <= usable {
                        let val = u64::from_le_bytes(buf[off..off + PTR].try_into().unwrap());
                        if val >= min_a && val < max_a {
                            pointers.push((base + off as u64, val));
                            if pointers.len() >= MAX_POINTERS {
                                truncate_warn();
                                break 'collect;
                            }
                        }
                        off += PTR;
                    }
                    base += usable.max(PTR) as u64;
                }
                Err(_) => break,
            }
        }
    }

    // Değere göre sırala → aralık sorgusu için ikili arama.
    pointers.sort_unstable_by_key(|&(_, val)| val);
    let values: Vec<u64> = pointers.iter().map(|&(_, v)| v).collect();

    // Hedeften geriye BFS. Her düğüm: (adres, o ana kadarki ofsetler).
    let mut chains = Vec::new();
    let mut frontier: Vec<(u64, Vec<u64>)> = vec![(target, Vec::new())];
    for _lvl in 0..depth {
        let mut next: Vec<(u64, Vec<u64>)> = Vec::new();
        for (node, offs) in &frontier {
            // değeri [node - max_offset, node] olan pointer'ları bul.
            let lo = node.saturating_sub(max_offset);
            let start = values.partition_point(|&v| v < lo);
            let mut i = start;
            while i < values.len() && values[i] <= *node {
                let (loc, val) = pointers[i];
                let this_off = node - val;
                let mut new_offs = vec![this_off];
                new_offs.extend(offs.iter().copied());
                // Kök statik bölgede mi? → zincir tamam.
                if let Some(r) = region_of(&regions, loc) {
                    if is_static(r) {
                        chains.push(PointerChain {
                            base_module: module_name(&r.path),
                            base_address: loc,
                            base_offset: loc - r.start,
                            offsets: new_offs.clone(),
                            depth: new_offs.len(),
                        });
                        if chains.len() >= MAX_CHAINS {
                            return Ok(chains);
                        }
                    }
                }
                // Daha derine: bu pointer konumu yeni düğüm.
                next.push((loc, new_offs));
                i += 1;
            }
        }
        if next.is_empty() {
            break;
        }
        // Frontier'ı sınırla (patlamayı önle).
        next.truncate(50_000);
        frontier = next;
    }

    Ok(chains)
}

fn module_name(path: &str) -> String {
    if path.is_empty() {
        return "(anonim)".into();
    }
    path.rsplit(['/', '\\']).next().unwrap_or(path).to_string()
}

fn truncate_warn() {
    crate::console::log(
        "pointer scan: pointer üst sınırına ulaşıldı, sonuçlar kısmi olabilir".into(),
    );
}
