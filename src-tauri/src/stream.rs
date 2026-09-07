//! Alt-süreçleri (jadx/apktool) canlı akışla çalıştırma: stdout/stderr satırlarını
//! anlık okur, jadx ilerleme satırlarını yakalar ve bir `Channel`'a event gönderir.
//! Ana thread'i bloklamaz (tüm iş arka plan thread'inde).

use byteforge_core::progress::parse_jadx_progress;
use serde::Serialize;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use tauri::ipc::Channel;

/// Akış sırasında UI'a gönderilen olay.
#[derive(Serialize, Clone)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum StreamEvent {
    /// Sıradan bir log satırı.
    Line { text: String },
    /// İlerleme güncellemesi.
    Progress { percent: u32, current: u64, total: u64 },
    /// İşlem başarıyla bitti (çıktı üretildi).
    Done { out_dir: String, files: usize },
    /// İşlem başarısız (hiç çıktı üretilmedi).
    Error { message: String },
}

/// Bir okuyucuyu ayrı thread'de satır satır (hem `\n` hem `\r`) tüketir.
fn pump<R: Read + Send + 'static>(
    mut reader: R,
    ch: Channel<StreamEvent>,
) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        let mut buf = [0u8; 4096];
        let mut line: Vec<u8> = Vec::new();
        loop {
            match reader.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    for &b in &buf[..n] {
                        if b == b'\n' || b == b'\r' {
                            flush_line(&mut line, &ch);
                        } else {
                            line.push(b);
                        }
                    }
                }
                Err(_) => break,
            }
        }
        flush_line(&mut line, &ch);
    })
}

fn flush_line(line: &mut Vec<u8>, ch: &Channel<StreamEvent>) {
    if line.is_empty() {
        return;
    }
    let text = String::from_utf8_lossy(line).trim().to_string();
    line.clear();
    if text.is_empty() {
        return;
    }
    if let Some((percent, current, total)) = parse_jadx_progress(&text) {
        let _ = ch.send(StreamEvent::Progress {
            percent,
            current,
            total,
        });
    } else {
        crate::console::log(text.clone());
        let _ = ch.send(StreamEvent::Line { text });
    }
}

/// Belirtilen uzantıdaki dosyaları özyinelemeli sayar.
fn count_ext(dir: &Path, ext: &str) -> usize {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return 0;
    };
    let mut n = 0;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            n += count_ext(&path, ext);
        } else if path.extension().and_then(|e| e.to_str()) == Some(ext) {
            n += 1;
        }
    }
    n
}

/// Bir aracı canlı akışla çalıştırır. Komut hemen döner; ilerleme ve sonuç
/// `on_event` kanalıyla gönderilir. Çıkış kodu non-zero olsa bile hedef dizinde
/// `ext` dosyası varsa **başarı** kabul edilir (kısmi decompile toleransı).
pub fn run_stream(
    bin: String,
    args: Vec<String>,
    out_dir: PathBuf,
    ext: &'static str,
    on_event: Channel<StreamEvent>,
) {
    std::thread::spawn(move || {
        crate::console::log(format!("$ {} {}", bin, args.join(" ")));
        let mut child = match Command::new(&bin)
            .args(&args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
        {
            Ok(c) => c,
            Err(e) => {
                let _ = on_event.send(StreamEvent::Error {
                    message: format!("{bin} başlatılamadı: {e}"),
                });
                return;
            }
        };

        let mut handles = Vec::new();
        if let Some(out) = child.stdout.take() {
            handles.push(pump(out, on_event.clone()));
        }
        if let Some(err) = child.stderr.take() {
            handles.push(pump(err, on_event.clone()));
        }
        let _ = child.wait();
        for h in handles {
            let _ = h.join();
        }

        let files = count_ext(&out_dir, ext);
        if files == 0 {
            let _ = on_event.send(StreamEvent::Error {
                message: format!("{bin} hiç .{ext} dosyası üretemedi"),
            });
        } else {
            let _ = on_event.send(StreamEvent::Done {
                out_dir: out_dir.to_string_lossy().into_owned(),
                files,
            });
        }
    });
}
