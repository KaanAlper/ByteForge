use std::sync::{Arc, Mutex};
use std::thread;
use tauri::{AppHandle, Emitter};
use once_cell::sync::Lazy;

#[derive(serde::Serialize, Clone)]
pub struct PacketInfo {
    pub id: usize,
    pub protocol: String,
    pub src: String,
    pub dst: String,
    pub length: usize,
    pub payload_preview: String,
}

static SNIFFER_ACTIVE: Lazy<Arc<Mutex<bool>>> = Lazy::new(|| Arc::new(Mutex::new(false)));
static PACKET_ID: Lazy<Mutex<usize>> = Lazy::new(|| Mutex::new(0));

#[tauri::command]
#[cfg(target_os = "linux")]
pub fn is_root() -> bool {
    unsafe { libc::geteuid() == 0 }
}

#[tauri::command]
#[cfg(not(target_os = "linux"))]
pub fn is_root() -> bool {
    false // Only supported on Linux for now
}

#[tauri::command]
#[cfg(target_os = "linux")]
pub fn start_sniffer(app: AppHandle) -> Result<(), String> {
    if !is_root() {
        return Err("Root izni gerekli! Lütfen 'sudo' ile başlatın.".to_string());
    }
    
    let mut active = SNIFFER_ACTIVE.lock().unwrap();
    if *active { return Ok(()); }
    *active = true;
    
    let active_clone = SNIFFER_ACTIVE.clone();
    
    thread::spawn(move || {
        // Use raw sockets using libc to avoid pcap crate compile issues or missing libpcap.
        // It's much safer to compile in rust natively.
        unsafe {
            let fd = libc::socket(libc::AF_PACKET, libc::SOCK_RAW, 768); // 768 = ETH_P_ALL (0x0300) in network byte order on LE
            if fd < 0 {
                let _ = app.emit("sniffer-error", "Raw socket açılamadı. Sudo emin misiniz?");
                *active_clone.lock().unwrap() = false;
                return;
            }
            
            let mut buf = [0u8; 65536];
            loop {
                if !*active_clone.lock().unwrap() {
                    libc::close(fd);
                    break;
                }
                
                let res = libc::recv(fd, buf.as_mut_ptr() as *mut libc::c_void, buf.len(), libc::MSG_DONTWAIT);
                if res > 0 {
                    let size = res as usize;
                    if size > 34 { // Min ethernet + IPv4
                        let eth_type = ((buf[12] as u16) << 8) | (buf[13] as u16);
                        if eth_type == 0x0800 { // IPv4
                            let ihl = (buf[14] & 0x0F) as usize * 4;
                            let protocol = buf[23];
                            let src = format!("{}.{}.{}.{}", buf[26], buf[27], buf[28], buf[29]);
                            let dst = format!("{}.{}.{}.{}", buf[30], buf[31], buf[32], buf[33]);
                            
                            let mut proto_name = "Unknown".to_string();
                            let mut payload = String::new();
                            
                            if protocol == 6 { // TCP
                                proto_name = "TCP".to_string();
                                let tcp_offset = 14 + ihl;
                                if size > tcp_offset + 20 {
                                    let data_offset = ((buf[tcp_offset + 12] >> 4) as usize) * 4;
                                    let payload_offset = tcp_offset + data_offset;
                                    if size > payload_offset {
                                        payload = String::from_utf8_lossy(&buf[payload_offset..std::cmp::min(payload_offset + 64, size)]).into_owned();
                                    }
                                }
                            } else if protocol == 17 { // UDP
                                proto_name = "UDP".to_string();
                                let payload_offset = 14 + ihl + 8;
                                if size > payload_offset {
                                    payload = String::from_utf8_lossy(&buf[payload_offset..std::cmp::min(payload_offset + 64, size)]).into_owned();
                                }
                            }
                            
                            // Only emit TCP or UDP to avoid flooding with low level noise
                            if protocol == 6 || protocol == 17 {
                                let mut id_lock = PACKET_ID.lock().unwrap();
                                *id_lock += 1;
                                let pkt = PacketInfo {
                                    id: *id_lock,
                                    protocol: proto_name,
                                    src,
                                    dst,
                                    length: size,
                                    payload_preview: payload.chars().filter(|c| c.is_ascii_graphic() || c.is_ascii_whitespace()).collect(),
                                };
                                let _ = app.emit("sniffer-packet", pkt);
                            }
                        }
                    }
                } else {
                    thread::sleep(std::time::Duration::from_millis(10));
                }
            }
        }
    });
    
    Ok(())
}

#[tauri::command]
pub fn stop_sniffer() {
    *SNIFFER_ACTIVE.lock().unwrap() = false;
}

#[tauri::command]
pub fn clear_sniffer() {
    *PACKET_ID.lock().unwrap() = 0;
}

#[cfg(not(target_os = "linux"))]
#[tauri::command]
pub fn start_sniffer(_app: AppHandle) -> Result<(), String> {
    Err("Ağ Dinleyici şu anda sadece Linux üzerinde desteklenmektedir.".to_string())
}
