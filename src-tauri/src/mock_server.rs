use std::fs;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};
use std::thread;
use tauri::{AppHandle, Emitter};
use once_cell::sync::Lazy;

static MOCK_ACTIVE: Lazy<Arc<Mutex<bool>>> = Lazy::new(|| Arc::new(Mutex::new(false)));

#[derive(serde::Serialize, Clone)]
pub struct MockRequestInfo {
    pub ip: String,
    pub path: String,
    pub method: String,
    pub body_preview: String,
}

#[cfg(target_os = "linux")]
fn is_root() -> bool {
    unsafe { libc::geteuid() == 0 }
}

#[cfg(not(target_os = "linux"))]
fn is_root() -> bool {
    false
}

fn clean_hosts() {
    #[cfg(target_os = "linux")]
    {
        if let Ok(content) = fs::read_to_string("/etc/hosts") {
            let filtered: Vec<&str> = content.lines().filter(|l| !l.contains("# BYTEFORGE MOCK")).collect();
            let new_content = filtered.join("\n") + "\n";
            let _ = fs::write("/etc/hosts", new_content);
        }
    }
}

#[tauri::command]
pub fn start_mock_server(app: AppHandle, domain: String, port: u16, response: String) -> Result<(), String> {
    if !is_root() {
        return Err("Sahte sunucu ve Hosts yönlendirmesi için Root (Sudo) izni gereklidir!".to_string());
    }

    let mut active = MOCK_ACTIVE.lock().unwrap();
    if *active {
        return Err("Sunucu zaten çalışıyor. Önce durdurun.".to_string());
    }
    
    // Clean old hosts entries just in case
    clean_hosts();
    
    // Append to /etc/hosts
    #[cfg(target_os = "linux")]
    {
        if let Ok(mut file) = fs::OpenOptions::new().append(true).open("/etc/hosts") {
            let entry = format!("127.0.0.1 {} # BYTEFORGE MOCK\n", domain);
            if file.write_all(entry.as_bytes()).is_err() {
                return Err("/etc/hosts dosyasına yazılamadı!".to_string());
            }
        } else {
            return Err("/etc/hosts dosyası açılamadı!".to_string());
        }
    }

    *active = true;
    let active_clone = MOCK_ACTIVE.clone();
    let response_clone = response.clone();

    // Spawn server thread
    thread::spawn(move || {
        let listener_res = TcpListener::bind(format!("0.0.0.0:{}", port));
        if listener_res.is_err() {
            let _ = app.emit("mock-error", "Port kullanımda olabilir veya yetki yok.");
            *active_clone.lock().unwrap() = false;
            clean_hosts();
            return;
        }
        let listener = listener_res.unwrap();
        listener.set_nonblocking(true).unwrap();

        let http_response = format!(
            "HTTP/1.1 200 OK\r\n\
             Content-Type: application/json\r\n\
             Access-Control-Allow-Origin: *\r\n\
             Connection: close\r\n\
             Content-Length: {}\r\n\r\n\
             {}",
            response_clone.len(),
            response_clone
        );

        while *active_clone.lock().unwrap() {
            match listener.accept() {
                Ok((mut stream, addr)) => {
                    let mut buf = [0u8; 1024];
                    if let Ok(size) = stream.read(&mut buf) {
                        let request_str = String::from_utf8_lossy(&buf[..size]);
                        let mut method = "UNKNOWN".to_string();
                        let mut path = "/".to_string();
                        let mut body = "".to_string();

                        if let Some(first_line) = request_str.lines().next() {
                            let parts: Vec<&str> = first_line.split_whitespace().collect();
                            if parts.len() >= 2 {
                                method = parts[0].to_string();
                                path = parts[1].to_string();
                            }
                        }

                        if let Some(body_idx) = request_str.find("\r\n\r\n") {
                            body = request_str[body_idx+4..].chars().take(100).collect();
                        }

                        let info = MockRequestInfo {
                            ip: addr.ip().to_string(),
                            path,
                            method,
                            body_preview: body,
                        };
                        let _ = app.emit("mock-request", info);
                        
                        // Send fake response
                        let _ = stream.write_all(http_response.as_bytes());
                    }
                },
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(std::time::Duration::from_millis(50));
                },
                Err(_) => {}
            }
        }
    });

    Ok(())
}

#[tauri::command]
pub fn stop_mock_server() -> Result<(), String> {
    *MOCK_ACTIVE.lock().unwrap() = false;
    clean_hosts();
    Ok(())
}
