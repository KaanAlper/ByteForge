use crate::abi::detect_abis;
use crate::archive::{list_entries, read_entry};
use crate::manifest::{parse_manifest, ManifestInfo};
use crate::runtime::{detect_runtime, RuntimeKind};
use crate::signature::{detect_v1, detect_v2_v3, SignatureSchemes};
use crate::Result;
use serde::Serialize;
use std::path::Path;

#[derive(Debug, Clone, Serialize)]
pub struct AppProfile {
    pub file_name: String,
    pub entry_count: usize,
    pub runtime: RuntimeKind,
    pub abis: Vec<String>,
    pub signatures: SignatureSchemes,
    /// AndroidManifest.xml varsa çıkarılan özet (best-effort; parse hatası → None).
    pub manifest: Option<ManifestInfo>,
    /// Tespit edilen runtime'a göre önerilen müdahale hattı (insan-okur metin).
    pub recommended_pipeline: String,
    /// Tespit edilen koruyucu/paketleyiciler (boş → korumasız/temiz görünüyor).
    pub packers: Vec<crate::packer::PackerHit>,
}

/// Bir APK yolunu analiz edip birleşik profili döndürür.
pub fn analyze_archive(path: &Path) -> Result<AppProfile> {
    let entries = list_entries(path)?;
    let runtime = detect_runtime(&entries);
    let abis = detect_abis(&entries);
    let v1 = detect_v1(&entries);
    let (v2, v3) = detect_v2_v3(path)?;
    // Manifest best-effort: bozuk/eksik manifest tüm analizi bozmamalı.
    let manifest = if entries.iter().any(|e| e == "AndroidManifest.xml") {
        read_entry(path, "AndroidManifest.xml")
            .ok()
            .and_then(|bytes| parse_manifest(&bytes).ok())
    } else {
        None
    };
    let file_name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    Ok(AppProfile {
        file_name,
        entry_count: entries.len(),
        runtime,
        abis,
        signatures: SignatureSchemes { v1, v2, v3 },
        manifest,
        recommended_pipeline: recommend_pipeline(runtime),
        packers: crate::packer::detect_android_packers(&entries),
    })
}

/// Runtime'a göre önerilen pipeline metni.
pub fn recommend_pipeline(runtime: RuntimeKind) -> String {
    match runtime {
        RuntimeKind::UnityIl2cpp => {
            "IL2CPP: global-metadata.dat + libil2cpp.so → sembol dump → native .so patch".into()
        }
        RuntimeKind::UnityMono => {
            "Unity Mono: Assembly-CSharp.dll → IL düzenleme (dnSpy tarzı)".into()
        }
        RuntimeKind::Flutter => {
            "Flutter/Dart AOT: libapp.so analizi (sınırlı); ağ/asset düzeyi müdahale".into()
        }
        RuntimeKind::ReactNative => "React Native: index.android.bundle (JS) düzenleme".into(),
        RuntimeKind::Unreal => {
            "Unreal Engine: libUE4.so native patch; .pak asset düzeyi müdahale".into()
        }
        RuntimeKind::Godot => "Godot: libgodot .pck paketi; GDScript/sahne düzenleme".into(),
        RuntimeKind::JavaKotlin => {
            "Java/Kotlin: apktool/baksmali → Smali patch → yeniden derle".into()
        }
        RuntimeKind::Unknown => "Bilinmeyen: manuel inceleme önerilir".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn make_apk(entries: &[&str]) -> tempfile::NamedTempFile {
        let file = tempfile::NamedTempFile::new().unwrap();
        let mut zip = zip::ZipWriter::new(file.reopen().unwrap());
        let opts: zip::write::FileOptions<()> = zip::write::FileOptions::default();
        for name in entries {
            zip.start_file(*name, opts).unwrap();
            zip.write_all(b"x").unwrap();
        }
        zip.finish().unwrap();
        file
    }

    #[test]
    fn builds_java_kotlin_profile() {
        let apk = make_apk(&[
            "classes.dex",
            "lib/arm64-v8a/libfoo.so",
            "META-INF/CERT.RSA",
            "AndroidManifest.xml",
        ]);
        let p = analyze_archive(apk.path()).unwrap();
        assert_eq!(p.runtime, RuntimeKind::JavaKotlin);
        assert_eq!(p.abis, vec!["arm64-v8a".to_string()]);
        assert!(p.signatures.v1);
        assert_eq!(p.entry_count, 4);
        assert!(p.recommended_pipeline.to_lowercase().contains("smali"));
    }

    #[test]
    fn il2cpp_pipeline_mentions_native() {
        let apk = make_apk(&["lib/arm64-v8a/libil2cpp.so", "classes.dex"]);
        let p = analyze_archive(apk.path()).unwrap();
        assert_eq!(p.runtime, RuntimeKind::UnityIl2cpp);
        assert!(p.recommended_pipeline.to_lowercase().contains("il2cpp"));
    }

    #[test]
    fn attaches_manifest_from_real_axml() {
        let file = tempfile::NamedTempFile::new().unwrap();
        {
            let mut zip = zip::ZipWriter::new(file.reopen().unwrap());
            let opts: zip::write::FileOptions<()> = zip::write::FileOptions::default();
            zip.start_file("AndroidManifest.xml", opts).unwrap();
            zip.write_all(include_bytes!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/fixtures/AndroidManifest.xml"
            )))
            .unwrap();
            zip.start_file("classes.dex", opts).unwrap();
            zip.write_all(b"x").unwrap();
            zip.finish().unwrap();
        }
        let p = analyze_archive(file.path()).unwrap();
        let m = p.manifest.expect("manifest bekleniyordu");
        assert_eq!(m.package.as_deref(), Some("org.t0t0.androguard.TC"));
        assert!(m.debuggable);
    }

    #[test]
    fn manifest_none_when_absent() {
        let apk = make_apk(&["classes.dex", "lib/arm64-v8a/libil2cpp.so"]);
        assert!(analyze_archive(apk.path()).unwrap().manifest.is_none());
    }
}
