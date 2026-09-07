//! Analiz raporu üretimi (Markdown) — APK ve PE profillerini paylaşılabilir
//! özet belgeye dönüştürür. Saf Rust, test edilebilir.

use crate::pe::PeProfile;
use crate::profile::AppProfile;
use crate::runtime::RuntimeKind;

fn runtime_label(r: RuntimeKind) -> &'static str {
    match r {
        RuntimeKind::UnityIl2cpp => "Unity (IL2CPP)",
        RuntimeKind::UnityMono => "Unity (Mono)",
        RuntimeKind::Flutter => "Flutter / Dart AOT",
        RuntimeKind::ReactNative => "React Native",
        RuntimeKind::Unreal => "Unreal Engine",
        RuntimeKind::Godot => "Godot",
        RuntimeKind::JavaKotlin => "Java / Kotlin",
        RuntimeKind::Unknown => "Bilinmeyen",
    }
}

/// Bir APK profilini Markdown raporuna dönüştürür.
pub fn app_report_md(p: &AppProfile) -> String {
    let mut s = String::new();
    s.push_str(&format!("# APKForge Analiz Raporu — {}\n\n", p.file_name));
    s.push_str("## Genel\n\n");
    s.push_str(&format!("- **Motor:** {}\n", runtime_label(p.runtime)));
    s.push_str(&format!(
        "- **Mimari (ABI):** {}\n",
        if p.abis.is_empty() {
            "—".to_string()
        } else {
            p.abis.join(", ")
        }
    ));
    s.push_str(&format!("- **Giriş sayısı:** {}\n", p.entry_count));
    let sig = &p.signatures;
    s.push_str(&format!(
        "- **İmza:** v1={} v2={} v3={}\n",
        yn(sig.v1),
        yn(sig.v2),
        yn(sig.v3)
    ));

    if let Some(m) = &p.manifest {
        s.push_str("\n## Manifest\n\n");
        s.push_str(&format!(
            "- **Paket:** {}\n",
            m.package.as_deref().unwrap_or("—")
        ));
        s.push_str(&format!(
            "- **Sürüm:** {} ({})\n",
            m.version_name.as_deref().unwrap_or("—"),
            m.version_code.as_deref().unwrap_or("—")
        ));
        s.push_str(&format!(
            "- **SDK:** min {} / target {}\n",
            m.min_sdk.as_deref().unwrap_or("—"),
            m.target_sdk.as_deref().unwrap_or("—")
        ));
        s.push_str(&format!("- **debuggable:** {}\n", yn(m.debuggable)));
        s.push_str(&format!("- **İzin sayısı:** {}\n", m.permissions.len()));
    }

    push_packers(&mut s, &p.packers);
    s.push_str(&format!(
        "\n## Önerilen Hat\n\n{}\n",
        p.recommended_pipeline
    ));
    s
}

/// Bir PE profilini Markdown raporuna dönüştürür.
pub fn pe_report_md(p: &PeProfile) -> String {
    let mut s = String::new();
    s.push_str("# APKForge PE Analiz Raporu\n\n");
    s.push_str("## Genel\n\n");
    s.push_str(&format!(
        "- **Tür:** {} · {}\n",
        if p.is_dll { "DLL" } else { "EXE" },
        if p.is_64bit { "64-bit" } else { "32-bit" }
    ));
    s.push_str(&format!("- **Mimari:** {}\n", p.machine));
    s.push_str(&format!("- **Subsystem:** {}\n", p.subsystem));
    s.push_str(&format!("- **Derleyici:** {}\n", p.compiler));
    s.push_str(&format!("- **.NET:** {}\n", yn(p.is_dotnet)));
    s.push_str(&format!("- **Entry (RVA):** 0x{:X}\n", p.entry_point));
    s.push_str(&format!(
        "- **Paketlenmiş olası:** {}\n",
        yn(p.likely_packed)
    ));

    if !p.section_entropy.is_empty() {
        s.push_str("\n## Section Entropisi\n\n");
        for se in &p.section_entropy {
            s.push_str(&format!("- `{}` — {:.2} bit/bayt\n", se.name, se.bits));
        }
    }
    if !p.imported_dlls.is_empty() {
        s.push_str(&format!(
            "\n## Import edilen DLL'ler ({})\n\n",
            p.imported_dlls.len()
        ));
        for d in &p.imported_dlls {
            s.push_str(&format!("- {d}\n"));
        }
    }

    push_packers(&mut s, &p.packers);
    s.push_str(&format!(
        "\n## Önerilen Hat\n\n{}\n",
        p.recommended_pipeline
    ));
    s
}

fn push_packers(s: &mut String, packers: &[crate::packer::PackerHit]) {
    s.push_str("\n## Koruma / Paketleyici\n\n");
    if packers.is_empty() {
        s.push_str("- Bilinen bir paketleyici/koruyucu imzası bulunamadı.\n");
    } else {
        for p in packers {
            s.push_str(&format!(
                "- **{}** ({}) — {} · kanıt: `{}`\n",
                p.name, p.vendor, p.confidence, p.evidence
            ));
        }
    }
}

fn yn(b: bool) -> &'static str {
    if b {
        "evet"
    } else {
        "hayır"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pe::analyze_pe;

    #[test]
    fn pe_report_contains_sections() {
        let bytes = include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/forge.dll"
        ));
        let p = analyze_pe(bytes).unwrap();
        let md = pe_report_md(&p);
        assert!(md.contains("# APKForge PE Analiz Raporu"));
        assert!(md.contains("Derleyici:"));
        assert!(md.contains("Section Entropisi"));
        assert!(md.contains("Koruma / Paketleyici"));
    }

    #[test]
    fn app_report_from_real_apk_shape() {
        // AppProfile'ı elle kurmak yerine minimal alanlarla doğrula.
        use crate::profile::AppProfile;
        use crate::signature::SignatureSchemes;
        let p = AppProfile {
            file_name: "test.apk".into(),
            entry_count: 42,
            runtime: RuntimeKind::UnityIl2cpp,
            abis: vec!["arm64-v8a".into()],
            signatures: SignatureSchemes {
                v1: true,
                v2: false,
                v3: false,
            },
            manifest: None,
            recommended_pipeline: "test hattı".into(),
            packers: vec![],
        };
        let md = app_report_md(&p);
        assert!(md.contains("test.apk"));
        assert!(md.contains("Unity (IL2CPP)"));
        assert!(md.contains("arm64-v8a"));
        assert!(md.contains("test hattı"));
        assert!(md.contains("bulunamadı")); // packer yok
    }
}
