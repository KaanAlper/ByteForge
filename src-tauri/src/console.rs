use std::sync::{Mutex, OnceLock};

/// Uygulama genelinde çalıştırılan komutların çıktı günlüğü (bellek içi).
fn buffer() -> &'static Mutex<Vec<String>> {
    static C: OnceLock<Mutex<Vec<String>>> = OnceLock::new();
    C.get_or_init(|| Mutex::new(Vec::new()))
}

/// Konsola bir satır ekler (subprocess sarmalayıcıları tarafından çağrılır).
pub(crate) fn log(line: String) {
    if let Ok(mut c) = buffer().lock() {
        if c.len() >= 600 {
            c.drain(0..200);
        }
        c.push(line);
    }
}

/// Konsol satırlarını döndürür.
#[tauri::command]
pub fn read_console() -> Vec<String> {
    buffer().lock().map(|c| c.clone()).unwrap_or_default()
}

/// Konsolu temizler.
#[tauri::command]
pub fn clear_console() {
    if let Ok(mut c) = buffer().lock() {
        c.clear();
    }
}
