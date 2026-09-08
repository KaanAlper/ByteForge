//! Windows implementasyonu: OpenProcess + ReadProcessMemory/WriteProcessMemory
//! + VirtualQueryEx (bölgeler) + Toolhelp32 (süreç listesi).
//!
//! Not: Bu ortam Linux olduğundan bu yol burada çalışma-zamanı test edilemedi;
//! standart Win32 API sözleşmesine göre yazıldı.

use super::{perms_string, ProcSummary, ProcessMemory};
use byteforge_core::memscan::MemRegion;
use std::io;
use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
use windows_sys::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
    TH32CS_SNAPPROCESS,
};
use windows_sys::Win32::System::Memory::{
    VirtualQueryEx, MEMORY_BASIC_INFORMATION, MEM_COMMIT, PAGE_EXECUTE, PAGE_EXECUTE_READ,
    PAGE_EXECUTE_READWRITE, PAGE_EXECUTE_WRITECOPY, PAGE_GUARD, PAGE_NOACCESS, PAGE_READONLY,
    PAGE_READWRITE, PAGE_WRITECOPY,
};
use windows_sys::Win32::System::Threading::{
    OpenProcess, PROCESS_QUERY_INFORMATION, PROCESS_VM_OPERATION, PROCESS_VM_READ,
    PROCESS_VM_WRITE,
};

/// Açık bir Windows süreç tutamacı. Drop'ta kapatılır.
pub struct WinMem {
    handle: HANDLE,
}

unsafe impl Send for WinMem {}

impl Drop for WinMem {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.handle);
        }
    }
}

/// Toolhelp anlık görüntüsüyle çalışan süreçleri listeler.
pub fn list_processes() -> io::Result<Vec<ProcSummary>> {
    let mut out = Vec::new();
    unsafe {
        let snap = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snap.is_null() {
            return Err(io::Error::last_os_error());
        }
        let mut entry: PROCESSENTRY32W = std::mem::zeroed();
        entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
        if Process32FirstW(snap, &mut entry) != 0 {
            loop {
                let name = String::from_utf16_lossy(
                    &entry.szExeFile[..entry
                        .szExeFile
                        .iter()
                        .position(|&c| c == 0)
                        .unwrap_or(entry.szExeFile.len())],
                );
                out.push(ProcSummary {
                    pid: entry.th32ProcessID,
                    name,
                });
                if Process32NextW(snap, &mut entry) == 0 {
                    break;
                }
            }
        }
        CloseHandle(snap);
    }
    out.sort_by_key(|p| p.name.to_lowercase());
    Ok(out)
}

/// Bir süreci okuma+yazma+sorgu erişimiyle açar.
pub fn open(pid: u32) -> io::Result<Box<dyn ProcessMemory>> {
    unsafe {
        let handle = OpenProcess(
            PROCESS_VM_READ | PROCESS_VM_WRITE | PROCESS_VM_OPERATION | PROCESS_QUERY_INFORMATION,
            0,
            pid,
        );
        if handle.is_null() {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                format!(
                    "OpenProcess({pid}) başarısız: {} — yönetici olarak çalıştırmayı deneyin",
                    io::Error::last_os_error()
                ),
            ));
        }
        Ok(Box::new(WinMem { handle }))
    }
}

impl ProcessMemory for WinMem {
    fn read_at(&self, addr: u64, buf: &mut [u8]) -> io::Result<usize> {
        use windows_sys::Win32::System::Diagnostics::Debug::ReadProcessMemory;
        let mut read: usize = 0;
        let ok = unsafe {
            ReadProcessMemory(
                self.handle,
                addr as *const _,
                buf.as_mut_ptr() as *mut _,
                buf.len(),
                &mut read,
            )
        };
        if ok == 0 && read == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(read)
    }

    fn write_at(&self, addr: u64, buf: &[u8]) -> io::Result<usize> {
        use windows_sys::Win32::System::Diagnostics::Debug::WriteProcessMemory;
        let mut written: usize = 0;
        let ok = unsafe {
            WriteProcessMemory(
                self.handle,
                addr as *mut _,
                buf.as_ptr() as *const _,
                buf.len(),
                &mut written,
            )
        };
        if ok == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(written)
    }

    fn regions(&self) -> io::Result<Vec<MemRegion>> {
        let mut out = Vec::new();
        let mut addr: usize = 0;
        unsafe {
            loop {
                let mut mbi: MEMORY_BASIC_INFORMATION = std::mem::zeroed();
                let n = VirtualQueryEx(
                    self.handle,
                    addr as *const _,
                    &mut mbi,
                    std::mem::size_of::<MEMORY_BASIC_INFORMATION>(),
                );
                if n == 0 {
                    break;
                }
                let base = mbi.BaseAddress as u64;
                let size = mbi.RegionSize as u64;
                if size == 0 {
                    break;
                }
                if mbi.State == MEM_COMMIT {
                    let p = mbi.Protect;
                    let guarded = p & PAGE_GUARD != 0 || p == PAGE_NOACCESS;
                    let readable = !guarded
                        && (p & (PAGE_READONLY
                            | PAGE_READWRITE
                            | PAGE_WRITECOPY
                            | PAGE_EXECUTE_READ
                            | PAGE_EXECUTE_READWRITE
                            | PAGE_EXECUTE_WRITECOPY)
                            != 0);
                    let writable = !guarded
                        && (p & (PAGE_READWRITE
                            | PAGE_WRITECOPY
                            | PAGE_EXECUTE_READWRITE
                            | PAGE_EXECUTE_WRITECOPY)
                            != 0);
                    let exec = p
                        & (PAGE_EXECUTE
                            | PAGE_EXECUTE_READ
                            | PAGE_EXECUTE_READWRITE
                            | PAGE_EXECUTE_WRITECOPY)
                        != 0;
                    out.push(MemRegion {
                        start: base,
                        end: base + size,
                        perms: perms_string(readable, writable, exec),
                        path: String::new(),
                    });
                }
                addr = base.saturating_add(size) as usize;
                if addr == 0 {
                    break;
                }
            }
        }
        Ok(out)
    }
}
