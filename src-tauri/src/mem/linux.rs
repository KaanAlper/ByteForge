//! Linux implementasyonu: `/proc/<pid>/maps` + `/proc/<pid>/mem`.

use super::{ProcSummary, ProcessMemory};
use byteforge_core::memscan::{parse_maps, MemRegion};
use std::fs::{File, OpenOptions};
use std::io;
use std::os::unix::fs::FileExt;

/// Açık bir Linux süreç-bellek tutamacı. Okuma için kalıcı bir RO handle
/// tutar; yazma anında RW handle açar (yazma seyrek).
pub struct LinuxMem {
    pid: u32,
    mem: File,
}

/// Çalışan süreçleri `/proc` taranarak listeler.
pub fn list_processes() -> io::Result<Vec<ProcSummary>> {
    let mut out = Vec::new();
    for entry in std::fs::read_dir("/proc")?.flatten() {
        let name = entry.file_name();
        let Some(pid_str) = name.to_str() else { continue };
        let Ok(pid) = pid_str.parse::<u32>() else { continue };
        let Ok(comm) = std::fs::read_to_string(format!("/proc/{pid}/comm")) else {
            continue;
        };
        let mut comm = comm.trim().to_string();
        
        // Linux comm limit is 15 chars. If it's exactly 15, read cmdline for the full binary name
        if comm.len() == 15 {
            if let Ok(cmd) = std::fs::read_to_string(format!("/proc/{pid}/cmdline")) {
                if let Some(first) = cmd.split('\0').next() {
                    if let Some(basename) = first.split('/').last() {
                        if !basename.is_empty() && basename.starts_with(&comm[0..10]) {
                            comm = basename.to_string();
                        }
                    }
                }
            }
        }
        if !comm.is_empty() {
            out.push(ProcSummary { pid, name: comm });
        }
    }
    out.sort_by_key(|p| p.name.to_lowercase());
    Ok(out)
}

/// Bir süreci okuma için açar.
pub fn open(pid: u32) -> io::Result<Box<dyn ProcessMemory>> {
    let mem = OpenOptions::new()
        .read(true)
        .open(format!("/proc/{pid}/mem"))
        .map_err(|e| {
            io::Error::new(
                e.kind(),
                format!(
                    "/proc/{pid}/mem açılamadı: {e} — ptrace izni gerekli \
                     (sudo ile çalıştırın veya ptrace_scope=0)"
                ),
            )
        })?;
    Ok(Box::new(LinuxMem { pid, mem }))
}

impl ProcessMemory for LinuxMem {
    fn read_at(&self, addr: u64, buf: &mut [u8]) -> io::Result<usize> {
        self.mem.read_at(buf, addr)
    }

    fn write_at(&self, addr: u64, buf: &[u8]) -> io::Result<usize> {
        let f = OpenOptions::new()
            .write(true)
            .open(format!("/proc/{}/mem", self.pid))
            .map_err(|e| {
                io::Error::new(
                    e.kind(),
                    format!("/proc/{}/mem yazma için açılamadı: {e}", self.pid),
                )
            })?;
        f.write_at(buf, addr)
    }

    fn regions(&self) -> io::Result<Vec<MemRegion>> {
        let text = std::fs::read_to_string(format!("/proc/{}/maps", self.pid))?;
        Ok(parse_maps(&text))
    }
}
