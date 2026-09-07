pub mod abi;
pub mod antitamper;
pub mod archive;
pub mod deobf;
pub mod diff;
pub mod disasm;
pub mod elf;
pub mod entropy;
pub mod error;
pub mod forensic;
pub mod frida;
pub mod heuristic;
pub mod hexedit;
pub mod il2cpp;
pub mod il2cpp_resolve;
pub mod manifest;
pub mod memscan;
pub mod modmenu;
pub mod mono;
pub mod packer;
pub mod patch;
pub mod pe;
pub mod profile;
pub mod progress;
pub mod prologue;
pub mod recipe;
pub mod report;
pub mod runtime;
pub mod sign;
pub mod signature;
pub mod smali;
pub mod yara;
pub mod smali_rules;
pub mod tools;

pub use antitamper::{scan_antitamper, TamperHit};
pub use deobf::{deobfuscate, Candidate};
pub use diff::{byte_diff_regions, diff_archives, ArchiveDiff, ByteDiff, DiffRegion};
pub use disasm::{disassemble, DisasmLine};
pub use elf::{
    file_offset_to_rva, list_symbols, load_segments, rva_to_file_offset, ElfSegment, SoSymbol,
};
pub use entropy::{analyze as analyze_entropy, shannon_entropy, EntropyClass, EntropyInfo};
pub use error::{CoreError, Result};
pub use forensic::{forensic_report, Finding};
pub use frida::{builtin_scripts, tracer_script, FridaScript};
pub use heuristic::{is_library_path, rank_names, score_symbol, Score};
pub use il2cpp_resolve::{is_resolvable, resolve_methods, ResolveResult, ResolvedMethod};
pub use manifest::ManifestInfo;
pub use memscan::{encode_value, find_all, is_scannable, parse_maps, MemRegion, ValueType};
pub use modmenu::{
    ensure_permission, insert_oncreate_hook, menu_loader_smali, Feature, FeatureTarget, MenuConfig,
};
pub use mono::{dotnet_strings, parse_metadata_strings};
pub use packer::{detect_android_packers, detect_pe_packers, PackerHit};
pub use patch::{
    detect_arm64_patch, detect_x86_patch, template_bytes, template_bytes_arch, Arch, PatchTemplate,
};
pub use pe::{
    analyze_pe, pe_exports, pe_offset_to_rva, pe_rva_to_offset, pe_sections, PeExport, PeProfile,
    PeSection,
};
pub use profile::{analyze_archive, AppProfile};
pub use progress::parse_jadx_progress;
pub use prologue::{check_arm64, check_x86, PrologueCheck};
pub use recipe::{
    parse_recipe, to_json, verify_step, Recipe, RecipeStep, StepStatus, RECIPE_VERSION,
};
pub use report::{app_report_md, pe_report_md};
pub use smali_rules::{scan_rules, RuleHit};
