//! Çoklu platform süreç-bellek erişim katmanı (Cheat Engine tarzı).
//!
//! `ProcessMemory` trait'i bir süreçte okuma/yazma ve bellek bölgelerini
//! listelemeyi soyutlar. Her platform kendi implementasyonunu sağlar:
//! Linux `/proc/<pid>/mem`, Windows `ReadProcessMemory`/`VirtualQueryEx`,
//! macOS `mach_vm_read`/`mach_vm_region`. Üst katman (tarama/dondurma/pointer
//! scan) tamamen bu trait üzerinden çalışır, OS'a bağımlı değildir.

use byteforge_core::memscan::MemRegion;

/// Çalışan bir süreç özeti.
#[derive(serde::Serialize, Clone)]
pub struct ProcSummary {
    pub pid: u32,
    pub name: String,
}

/// Bir sürecin belleğine okuma/yazma ve bölge listeleme soyutlaması.
pub trait ProcessMemory: Send {
    /// `addr`'dan `buf.len()` bayt okur; okunan bayt sayısını döndürür.
    fn read_at(&self, addr: u64, buf: &mut [u8]) -> std::io::Result<usize>;
    /// `addr`'a `buf` yazar; yazılan bayt sayısını döndürür.
    fn write_at(&self, addr: u64, buf: &[u8]) -> std::io::Result<usize>;
    /// Sürecin bellek bölgelerini (izinlerle) döndürür.
    fn regions(&self) -> std::io::Result<Vec<MemRegion>>;
}

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
pub use linux::{list_processes, open};

#[cfg(target_os = "windows")]
mod windows;
#[cfg(target_os = "windows")]
pub use windows::{list_processes, open};

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
pub use macos::{list_processes, open};

/// Windows/macOS bölge izinlerini Linux "rwxp" biçimine çevirmek için ortak
/// yardımcı: okunabilir/yazılabilir/çalıştırılabilir bayraklarından perms üretir.
#[cfg(any(target_os = "windows", target_os = "macos"))]
pub(crate) fn perms_string(r: bool, w: bool, x: bool) -> String {
    let mut s = String::with_capacity(4);
    s.push(if r { 'r' } else { '-' });
    s.push(if w { 'w' } else { '-' });
    s.push(if x { 'x' } else { '-' });
    s.push('p');
    s
}
