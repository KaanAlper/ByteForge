//! macOS implementasyonu: task_for_pid + mach_vm_read_overwrite/mach_vm_write
//! + mach_vm_region (bölgeler) + libproc (süreç listesi).
//!
//! Not: task_for_pid çoğu durumda root veya `com.apple.security.cs.debugger`
//! yetkilendirmesi ister. Bu ortam Linux olduğundan burada çalışma-zamanı test
//! edilemedi; mach/libproc API sözleşmesine göre yazıldı.

use super::{perms_string, ProcSummary, ProcessMemory};
use byteforge_core::memscan::MemRegion;
use mach2::kern_return::KERN_SUCCESS;
use mach2::port::mach_port_t;
use mach2::traps::{mach_task_self, task_for_pid};
use mach2::vm::{mach_vm_read_overwrite, mach_vm_region, mach_vm_write};
use mach2::vm_prot::{VM_PROT_EXECUTE, VM_PROT_READ, VM_PROT_WRITE};
use mach2::vm_region::{vm_region_basic_info_data_64_t, VM_REGION_BASIC_INFO_64};
use std::io;

const PROC_ALL_PIDS: u32 = 1;

extern "C" {
    fn proc_listpids(t: u32, typeinfo: u32, buffer: *mut libc::c_void, buffersize: i32) -> i32;
    fn proc_name(pid: i32, buffer: *mut libc::c_void, buffersize: u32) -> i32;
}

/// Açık bir mach task tutamacı.
pub struct MacMem {
    task: mach_port_t,
}

unsafe impl Send for MacMem {}

/// libproc ile çalışan süreçleri listeler.
pub fn list_processes() -> io::Result<Vec<ProcSummary>> {
    let mut out = Vec::new();
    unsafe {
        let cap = proc_listpids(PROC_ALL_PIDS, 0, std::ptr::null_mut(), 0);
        if cap <= 0 {
            return Err(io::Error::last_os_error());
        }
        let count = cap as usize / std::mem::size_of::<i32>();
        let mut pids = vec![0i32; count];
        let n = proc_listpids(
            PROC_ALL_PIDS,
            0,
            pids.as_mut_ptr() as *mut libc::c_void,
            cap,
        );
        if n <= 0 {
            return Err(io::Error::last_os_error());
        }
        let got = n as usize / std::mem::size_of::<i32>();
        for &pid in pids.iter().take(got) {
            if pid <= 0 {
                continue;
            }
            let mut name_buf = [0u8; 256];
            let len = proc_name(
                pid,
                name_buf.as_mut_ptr() as *mut libc::c_void,
                name_buf.len() as u32,
            );
            let name = if len > 0 {
                String::from_utf8_lossy(&name_buf[..len as usize]).to_string()
            } else {
                continue;
            };
            out.push(ProcSummary {
                pid: pid as u32,
                name,
            });
        }
    }
    out.sort_by_key(|p| p.name.to_lowercase());
    Ok(out)
}

/// Bir süreç için mach task portunu alır.
pub fn open(pid: u32) -> io::Result<Box<dyn ProcessMemory>> {
    unsafe {
        let mut task: mach_port_t = 0;
        let kr = task_for_pid(mach_task_self(), pid as i32, &mut task);
        if kr != KERN_SUCCESS {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                format!(
                    "task_for_pid({pid}) başarısız (kr={kr}) — sudo ile çalıştırın \
                     veya debugger yetkilendirmesi gerekir"
                ),
            ));
        }
        Ok(Box::new(MacMem { task }))
    }
}

impl ProcessMemory for MacMem {
    fn read_at(&self, addr: u64, buf: &mut [u8]) -> io::Result<usize> {
        let mut out_size: u64 = 0;
        let kr = unsafe {
            mach_vm_read_overwrite(
                self.task,
                addr,
                buf.len() as u64,
                buf.as_mut_ptr() as usize as _,
                &mut out_size,
            )
        };
        if kr != KERN_SUCCESS {
            return Err(io::Error::new(
                io::ErrorKind::Other,
                format!("mach_vm_read_overwrite başarısız (kr={kr})"),
            ));
        }
        Ok(out_size as usize)
    }

    fn write_at(&self, addr: u64, buf: &[u8]) -> io::Result<usize> {
        let kr = unsafe {
            mach_vm_write(
                self.task,
                addr,
                buf.as_ptr() as usize as _,
                buf.len() as u32,
            )
        };
        if kr != KERN_SUCCESS {
            return Err(io::Error::new(
                io::ErrorKind::Other,
                format!("mach_vm_write başarısız (kr={kr})"),
            ));
        }
        Ok(buf.len())
    }

    fn regions(&self) -> io::Result<Vec<MemRegion>> {
        let mut out = Vec::new();
        let mut addr: u64 = 1;
        loop {
            let mut size: u64 = 0;
            let mut info: vm_region_basic_info_data_64_t = unsafe { std::mem::zeroed() };
            let mut count = mach2::vm_region::VM_REGION_BASIC_INFO_COUNT_64;
            let mut object_name: mach_port_t = 0;
            let kr = unsafe {
                mach_vm_region(
                    self.task,
                    &mut addr,
                    &mut size,
                    VM_REGION_BASIC_INFO_64,
                    &mut info as *mut _ as *mut i32,
                    &mut count,
                    &mut object_name,
                )
            };
            if kr != KERN_SUCCESS {
                break;
            }
            let r = info.protection & VM_PROT_READ != 0;
            let w = info.protection & VM_PROT_WRITE != 0;
            let x = info.protection & VM_PROT_EXECUTE != 0;
            out.push(MemRegion {
                start: addr,
                end: addr + size,
                perms: perms_string(r, w, x),
                path: String::new(),
            });
            addr = addr.saturating_add(size);
            if addr == 0 {
                break;
            }
        }
        Ok(out)
    }
}
