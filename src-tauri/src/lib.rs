mod commands;
mod console;
mod decompile;
mod deploy;
mod history;
mod il2cpp;
mod jadx;
mod memscan;
mod modmenu;
mod splits;
mod stream;

use std::sync::OnceLock;

/// Uygulama açılışındaki --import argümanı (frontend hazır olunca okunur).
static INITIAL_FILE: OnceLock<Option<String>> = OnceLock::new();

/// argv'den içe aktarılacak dosyayı çıkarır: `--import <yol>`, `-i <yol>` veya ilk dosya.
fn parse_file_arg(args: &[String]) -> Option<String> {
    let mut iter = args.iter().skip(1);
    while let Some(a) = iter.next() {
        if a == "--import" || a == "-import" || a == "-i" {
            return iter.next().cloned();
        }
        if !a.starts_with('-') {
            let low = a.to_lowercase();
            if low.ends_with(".apk")
                || low.ends_with(".xapk")
                || low.ends_with(".apks")
                || low.ends_with(".so")
                || low.ends_with(".zip")
            {
                return Some(a.clone());
            }
        }
    }
    None
}

/// Açılış argümanı olarak verilen dosyayı döndürür (frontend mount'ta çağırır).
#[tauri::command]
fn get_cli_file() -> Option<String> {
    INITIAL_FILE.get().cloned().flatten()
}

fn cli_help() {
    println!(
        "APKForge — Android RE iş istasyonu\n\n\
         Kullanım:\n  \
         byteforge [komut] [argümanlar]\n\n\
         Headless komutlar (GUI açmadan):\n  \
         profile <apk>          statik profil (JSON)\n  \
         resign <apk>           zipalign + debug imza\n  \
         decode <apk>           smali'ye aç (apktool)\n  \
         build <dizin>          smali dizinini apk'ya derle\n  \
         install <apk> [seri]   cihaza kur (adb)\n  \
         jadx <apk>             java kaynağına decompile\n  \
         il2cpp <apk>           IL2CPP sembol dökümü\n  \
         targets <apk>          Sezgisel Hedef Bulucu (akıllı lisans/kilit analizi)\n  \
         pe <exe|dll>           Windows PE profili (JSON)\n  \
         devices                bağlı adb cihazları\n\n\
         GUI:\n  \
         byteforge --import <dosya>   açık pencerede içe aktar\n  \
         byteforge <dosya>            aynısı (kısa)\n  \
         byteforge                    boş GUI"
    );
}

fn cli_arg1(rest: &[String]) -> Result<String, commands::ApiError> {
    rest.first()
        .cloned()
        .ok_or(commands::ApiError::InvalidPath {
            message: "dosya/dizin argümanı gerekli".into(),
        })
}

/// Headless alt-komutları işler. Bir alt-komut çalıştıysa çıkış kodunu, aksi
/// halde None (GUI ile devam) döndürür.
fn try_headless_cli(args: &[String]) -> Option<i32> {
    let cmd = args.get(1)?.as_str();
    let rest = &args[2..];
    let result: Result<String, commands::ApiError> = match cmd {
        "--help" | "-h" | "help" => {
            cli_help();
            return Some(0);
        }
        "profile" => cli_arg1(rest).and_then(|a| {
            byteforge_core::analyze_archive(&std::path::PathBuf::from(a))
                .map_err(commands::ApiError::from)
                .and_then(|p| {
                    serde_json::to_string_pretty(&p).map_err(|e| commands::ApiError::Io {
                        message: e.to_string(),
                    })
                })
        }),
        "resign" => cli_arg1(rest).and_then(deploy::resign_apk),
        "decode" => cli_arg1(rest).and_then(|a| {
            decompile::decode_apk(a)
                .map(|r| format!("Decode: {} ({} smali dosyası)", r.out_dir, r.smali_files))
        }),
        "build" => cli_arg1(rest).and_then(decompile::build_apk),
        "install" => {
            cli_arg1(rest).and_then(|a| deploy::install_apk(rest.get(1).cloned(), a))
        }
        "jadx" => cli_arg1(rest).and_then(|a| {
            jadx::jadx_decompile(a).map(|r| format!("Java: {} ({} dosya)", r.out_dir, r.java_files))
        }),
        "il2cpp" => cli_arg1(rest).and_then(|a| {
            il2cpp::dump_il2cpp_symbols(a).map(|r| {
                format!(
                    "{} sembol (metadata v{}) → {}",
                    r.count, r.metadata_version, r.out_file
                )
            })
        }),
        "targets" => cli_arg1(rest).and_then(|a| {
            il2cpp::dump_il2cpp_symbols(a).map(|r| {
                let targets = byteforge_core::heuristic::rank_names(&r.symbols, 25);
                format!(
                    "{} sembol arasından bulunan ilk {} akıllı hedef:\n{}",
                    r.count,
                    targets.len(),
                    targets
                        .iter()
                        .map(|t| format!("  [{:>2}] {:<40} ({})", t.score, t.name, t.confidence))
                        .collect::<Vec<_>>()
                        .join("\n")
                )
            })
        }),
        "pe" => cli_arg1(rest).and_then(|a| {
            commands::profile_pe(a).and_then(|p| {
                serde_json::to_string_pretty(&p).map_err(|e| commands::ApiError::Io {
                    message: e.to_string(),
                })
            })
        }),
        "devices" => deploy::list_adb_devices().map(|d| {
            if d.is_empty() {
                "bağlı cihaz yok".into()
            } else {
                d.iter()
                    .map(|x| format!("{} ({})", x.serial, x.state))
                    .collect::<Vec<_>>()
                    .join("\n")
            }
        }),
        // --import, tek dosya, boş ya da bilinmeyen → GUI'ye bırak
        _ => return None,
    };

    match result {
        Ok(msg) => {
            println!("{msg}");
            Some(0)
        }
        Err(e) => {
            eprintln!("Hata: {e:?}");
            Some(1)
        }
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let args: Vec<String> = std::env::args().collect();
    if let Some(code) = try_headless_cli(&args) {
        std::process::exit(code);
    }
    let _ = INITIAL_FILE.set(parse_file_arg(&args));

    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, argv, _cwd| {
            use tauri::{Emitter, Manager};
            if let Some(f) = parse_file_arg(&argv) {
                let _ = app.emit("open-file", f);
            }
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.set_focus();
            }
        }))
        .plugin(tauri_plugin_opener::init())
        .manage(deploy::LogcatState::default())
        .invoke_handler(tauri::generate_handler![
            commands::analyze_apk,
            commands::diff_apks,
            commands::forensic_report,
            commands::native_mod_recipe,
            commands::apply_native_mod,
            commands::package_patched_so,
            commands::disassemble_range,
            commands::yara_scan,
            commands::list_so_symbols,
            commands::so_segments,
            commands::rva_to_offset,
            commands::offset_to_rva,
            commands::patch_preview,
            commands::apply_so_patch,
            commands::read_hex,
            commands::search_hex,
            commands::write_hex,
            commands::check_tools,
            commands::check_prologue,
            commands::rank_symbols,
            commands::report_apk,
            commands::report_pe,
            commands::profile_pe,
            commands::opcode_hex,
            commands::dotnet_symbols,
            commands::antitamper_pe,
            commands::antitamper_so,
            commands::pe_sections,
            commands::pe_exports,
            commands::pe_rva_to_offset,
            commands::pe_offset_to_rva,
            commands::pe_patch_preview,
            commands::apply_pe_patch,
            commands::deobfuscate_string,
            commands::frida_scripts,
            commands::frida_tracer,
            deploy::resign_apk,
            deploy::resign_apk_with_mods,
            deploy::list_adb_devices,
            deploy::install_apk,
            decompile::decode_apk,
            decompile::decode_apk_stream,
            decompile::smali_cache_status,
            decompile::search_smali,
            decompile::scan_smali_rules,
            decompile::smali_smart_targets,
            decompile::list_smali_methods,
            decompile::patch_smali_method,
            decompile::build_apk,
            decompile::create_checkpoint,
            decompile::list_checkpoints,
            decompile::restore_checkpoint,
            decompile::list_manifest_permissions,
            decompile::remove_manifest_permissions,
            decompile::read_smali_file,
            decompile::write_smali_file,
            history::patch_history,
            history::revert_patch,
            history::clear_patch_history,
            history::revert_patch_at,
            history::has_revertible_patch,
            history::export_recipe,
            history::import_recipe,
            history::verify_recipe,
            deploy::start_logcat,
            deploy::stop_logcat,
            deploy::launch_app,
            splits::extract_splits,
            splits::install_splits,
            il2cpp::extract_il2cpp,
            il2cpp::dump_il2cpp_symbols,
            il2cpp::il2cpp_resolve,
            il2cpp::il2cpp_resolve_search,
            il2cpp::il2cpp_rank_resolved,
            il2cpp::il2cpp_lookup,
            jadx::jadx_decompile,
            jadx::jadx_decompile_stream,
            jadx::jadx_cache_status,
            modmenu::inject_mod_menu,
            memscan::list_processes,
            memscan::scan_process,
            memscan::refine_scan,
            memscan::write_memory,
            memscan::read_memory,
            memscan::ptrace_scope,
            jadx::search_java,
            jadx::read_java,
            console::read_console,
            console::clear_console,
            get_cli_file
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
