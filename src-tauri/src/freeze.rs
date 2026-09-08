//! Değer dondurma (Cheat Engine "freeze") — arka plan bir thread, dondurulmuş
//! adreslere periyodik olarak istenen değeri yazar. Çoklu platform: yazma
//! `crate::mem` üzerinden yapılır.

use crate::commands::ApiError;
use crate::mem;
use byteforge_core::memscan::{encode_value, ValueType};
use serde::Serialize;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

/// Dondurulmuş tek bir giriş.
#[derive(Clone)]
struct Frozen {
    pid: u32,
    address: u64,
    bytes: Vec<u8>,
    ty: ValueType,
    value: String,
    label: String,
}

/// UI'ye dönen dondurma girişi.
#[derive(Serialize)]
pub struct FrozenInfo {
    pub pid: u32,
    pub address: u64,
    pub value: String,
    pub label: String,
    pub ty: ValueType,
}

fn table() -> &'static Mutex<Vec<Frozen>> {
    static T: OnceLock<Mutex<Vec<Frozen>>> = OnceLock::new();
    T.get_or_init(|| Mutex::new(Vec::new()))
}

fn ensure_thread() {
    static STARTED: AtomicBool = AtomicBool::new(false);
    if STARTED.swap(true, Ordering::SeqCst) {
        return;
    }
    std::thread::spawn(|| loop {
        std::thread::sleep(Duration::from_millis(50));
        let entries = { table().lock().unwrap().clone() };
        if entries.is_empty() {
            continue;
        }
        // pid başına tek handle: aynı süreçteki girişleri grupla.
        let mut by_pid: HashMap<u32, Vec<&Frozen>> = HashMap::new();
        for e in &entries {
            by_pid.entry(e.pid).or_default().push(e);
        }
        for (pid, list) in by_pid {
            if let Ok(m) = mem::open(pid) {
                for e in list {
                    let _ = m.write_at(e.address, &e.bytes);
                }
            }
        }
    });
}

/// Bir adresi dondurur (verilen değere sabitler). Aynı (pid,address) varsa günceller.
#[tauri::command]
pub fn freeze_add(
    pid: u32,
    address: u64,
    value: String,
    ty: ValueType,
    label: Option<String>,
) -> Result<(), ApiError> {
    let bytes = encode_value(&value, ty).ok_or_else(|| ApiError::InvalidPath {
        message: "değer bu tür için ayrıştırılamadı".into(),
    })?;
    ensure_thread();
    let mut t = table().lock().unwrap();
    t.retain(|e| !(e.pid == pid && e.address == address));
    t.push(Frozen {
        pid,
        address,
        bytes,
        ty,
        value: value.clone(),
        label: label.unwrap_or_default(),
    });
    crate::console::log(format!("donduruldu: pid={pid} @0x{address:x} = {value}"));
    Ok(())
}

/// Bir dondurmayı kaldırır.
#[tauri::command]
pub fn freeze_remove(pid: u32, address: u64) {
    table()
        .lock()
        .unwrap()
        .retain(|e| !(e.pid == pid && e.address == address));
}

/// Tüm dondurmaları listeler.
#[tauri::command]
pub fn freeze_list() -> Vec<FrozenInfo> {
    table()
        .lock()
        .unwrap()
        .iter()
        .map(|e| FrozenInfo {
            pid: e.pid,
            address: e.address,
            value: e.value.clone(),
            label: e.label.clone(),
            ty: e.ty,
        })
        .collect()
}

/// Tüm dondurmaları temizler.
#[tauri::command]
pub fn freeze_clear() {
    table().lock().unwrap().clear();
}
