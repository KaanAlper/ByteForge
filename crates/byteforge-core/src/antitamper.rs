//! Anti-tamper / anti-debug tespiti — bir ikili dosyanın koruma/algılama
//! mekanizmalarını (debugger tespiti, imza kontrolü, bütünlük) imzayla bulur.
//! Saf Rust; import/symbol adları listesi üzerinde çalışır.

use serde::Serialize;

/// Tespit edilen bir koruma göstergesi.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TamperHit {
    /// Mekanizma adı, ör. "IsDebuggerPresent".
    pub name: String,
    /// Kategori: anti_debug / signature / integrity / anti_emulator.
    pub category: String,
    /// İnsan-okur açıklama / nötrleme önerisi.
    pub note: String,
}

struct Sig {
    needle: &'static str,
    name: &'static str,
    category: &'static str,
    note: &'static str,
}

const SIGS: &[Sig] = &[
    // Windows anti-debug (crackme'lerde yaygın)
    Sig {
        needle: "isdebuggerpresent",
        name: "IsDebuggerPresent",
        category: "anti_debug",
        note: "Debugger tespiti — dönüşü 0 (false) yapılmalı",
    },
    Sig {
        needle: "checkremotedebuggerpresent",
        name: "CheckRemoteDebuggerPresent",
        category: "anti_debug",
        note: "Uzak debugger tespiti — nötrle",
    },
    Sig {
        needle: "ntqueryinformationprocess",
        name: "NtQueryInformationProcess",
        category: "anti_debug",
        note: "ProcessDebugPort/Flags sorgusu ile anti-debug",
    },
    Sig {
        needle: "outputdebugstring",
        name: "OutputDebugString",
        category: "anti_debug",
        note: "OllyDbg-tarzı anti-debug hilesi olası",
    },
    Sig {
        needle: "setunhandledexceptionfilter",
        name: "SetUnhandledExceptionFilter",
        category: "anti_debug",
        note: "Exception tabanlı anti-debug olası",
    },
    Sig {
        needle: "queryperformancecounter",
        name: "QueryPerformanceCounter",
        category: "anti_debug",
        note: "Zamanlama tabanlı debugger tespiti olası",
    },
    Sig {
        needle: "rdtsc",
        name: "RDTSC",
        category: "anti_debug",
        note: "Zamanlama (rdtsc) tabanlı anti-debug",
    },
    // Linux/Android anti-debug
    Sig {
        needle: "ptrace",
        name: "ptrace",
        category: "anti_debug",
        note: "ptrace(PTRACE_TRACEME) self-attach anti-debug — çağrıyı NOP'la",
    },
    Sig {
        needle: "tracerpid",
        name: "TracerPid (/proc/self/status)",
        category: "anti_debug",
        note: "TracerPid okuyarak debugger tespiti",
    },
    // Bütünlük / öldürme
    Sig {
        needle: "sigkill",
        name: "kill(SIGKILL)",
        category: "integrity",
        note: "Tespit halinde süreç öldürme — çağrıyı etkisiz kıl",
    },
    Sig {
        needle: "exit(",
        name: "exit()",
        category: "integrity",
        note: "Koşullu erken çıkış olası",
    },
    // İmza kontrolü (Android)
    Sig {
        needle: "getpackageinfo",
        name: "PackageManager.getPackageInfo",
        category: "signature",
        note: "İmza doğrulama — sonucu bypass et",
    },
    Sig {
        needle: "getsignatures",
        name: "getSignatures",
        category: "signature",
        note: "İmza dizisi kontrolü — bypass",
    },
    Sig {
        needle: "checksignatures",
        name: "checkSignatures",
        category: "signature",
        note: "İmza eşleşme kontrolü",
    },
    // CRC / bütünlük
    Sig {
        needle: "crc32",
        name: "CRC32",
        category: "integrity",
        note: "Kod/dosya bütünlük (CRC) kontrolü olası",
    },
    // Emülatör tespiti
    Sig {
        needle: "qemu",
        name: "QEMU göstergesi",
        category: "anti_emulator",
        note: "Emülatör tespiti",
    },
    Sig {
        needle: "goldfish",
        name: "goldfish (emülatör)",
        category: "anti_emulator",
        note: "Android emülatör tespiti",
    },
];

/// Import/symbol/string adları listesinde koruma göstergelerini arar.
/// Aynı mekanizma birden çok kez geçse de tek kayıt döner.
pub fn scan_antitamper(tokens: &[String]) -> Vec<TamperHit> {
    let lower: Vec<String> = tokens.iter().map(|t| t.to_lowercase()).collect();
    let mut hits: Vec<TamperHit> = Vec::new();
    for sig in SIGS {
        if lower.iter().any(|t| t.contains(sig.needle)) && !hits.iter().any(|h| h.name == sig.name)
        {
            hits.push(TamperHit {
                name: sig.name.to_string(),
                category: sig.category.to_string(),
                note: sig.note.to_string(),
            });
        }
    }
    hits
}

#[cfg(test)]
mod tests {
    use super::*;

    fn owned(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn detects_windows_anti_debug() {
        let hits = scan_antitamper(&owned(&["IsDebuggerPresent", "GetProcAddress", "printf"]));
        assert!(hits
            .iter()
            .any(|h| h.name == "IsDebuggerPresent" && h.category == "anti_debug"));
    }

    #[test]
    fn detects_ptrace_and_sigkill() {
        let hits = scan_antitamper(&owned(&["ptrace", "libc_kill_SIGKILL_wrapper"]));
        assert!(hits.iter().any(|h| h.category == "anti_debug"));
        assert!(hits.iter().any(|h| h.category == "integrity"));
    }

    #[test]
    fn clean_binary_no_hits() {
        let hits = scan_antitamper(&owned(&["malloc", "free", "printf", "main"]));
        assert!(hits.is_empty());
    }

    #[test]
    fn dedups_same_mechanism() {
        let hits = scan_antitamper(&owned(&["ptrace", "__ptrace", "ptrace@plt"]));
        assert_eq!(hits.iter().filter(|h| h.name == "ptrace").count(), 1);
    }
}
