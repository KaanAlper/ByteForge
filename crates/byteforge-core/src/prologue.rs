//! Fonksiyon başlangıcı (prologue) doğrulaması — çökme önleme.
//!
//! Bir ofsete yama basmadan önce oranın gerçekten bir fonksiyon başlangıcı
//! olduğunu teyit eder (ortadaki bir baytı ezip oyunu çökertmemek için).

use crate::patch::{detect_arm64_patch, detect_x86_patch};
use serde::Serialize;

/// Prologue kontrol sonucu.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum PrologueCheck {
    /// Geçerli bir fonksiyon başlangıcı gibi görünüyor — yama güvenli.
    Prologue { detail: String },
    /// Zaten bilinen bir yamayla başlıyor.
    AlreadyPatched { template: String },
    /// Fonksiyon başlangıcı gibi görünmüyor — yama çökme riski taşır.
    NotPrologue { warning: String },
}

/// ARM64 baytlarının bir fonksiyon başlangıcı olup olmadığını denetler.
pub fn check_arm64(head: &[u8]) -> PrologueCheck {
    if let Some(t) = detect_arm64_patch(head) {
        return PrologueCheck::AlreadyPatched {
            template: t.to_string(),
        };
    }
    if head.len() < 4 {
        return PrologueCheck::NotPrologue {
            warning: "yeterli bayt okunamadı".into(),
        };
    }
    let b = &head[..4];
    // PACIASP: 3F 23 03 D5
    if b == [0x3F, 0x23, 0x03, 0xD5] {
        return PrologueCheck::Prologue {
            detail: "PACIASP (pointer auth, fonksiyon başı)".into(),
        };
    }
    // STP X29, X30, [SP, #imm]! : FD 7B .. A9
    if b[0] == 0xFD && b[1] == 0x7B && b[3] == 0xA9 {
        return PrologueCheck::Prologue {
            detail: "STP X29, X30, [SP, ...]! (frame kurulumu)".into(),
        };
    }
    // SUB SP, SP, #imm : FF .. .. D1
    if b[0] == 0xFF && b[3] == 0xD1 {
        return PrologueCheck::Prologue {
            detail: "SUB SP, SP, #imm (stack ayırma)".into(),
        };
    }
    // Diğer callee-saved STP'ler ([sp] tabanlı, high byte 0xA9/0xA8)
    if (b[3] == 0xA9 || b[3] == 0xA8) && b[1] & 0x03 == 0x03 {
        return PrologueCheck::Prologue {
            detail: "STP (register saklama, olası frame)".into(),
        };
    }
    PrologueCheck::NotPrologue {
        warning: "ARM64 prologue deseni (STP X29,X30 / SUB SP / PACIASP) bulunamadı — \
                  ortadaki bir komut olabilir, yama çökme riski taşır"
            .into(),
    }
}

/// x86/x64 baytlarının bir fonksiyon başlangıcı olup olmadığını denetler.
pub fn check_x86(head: &[u8]) -> PrologueCheck {
    if let Some(t) = detect_x86_patch(head) {
        return PrologueCheck::AlreadyPatched {
            template: t.to_string(),
        };
    }
    if head.is_empty() {
        return PrologueCheck::NotPrologue {
            warning: "bayt yok".into(),
        };
    }
    // endbr64: F3 0F 1E FA
    if head.len() >= 4 && head[..4] == [0xF3, 0x0F, 0x1E, 0xFA] {
        return PrologueCheck::Prologue {
            detail: "ENDBR64 (CET, fonksiyon başı)".into(),
        };
    }
    // push rbp/ebp
    if head[0] == 0x55 {
        return PrologueCheck::Prologue {
            detail: "PUSH RBP (frame kurulumu)".into(),
        };
    }
    // mov rbp, rsp (48 89 E5) — bazen push olmadan
    if head.len() >= 3 && head[0] == 0x48 && head[1] == 0x89 && head[2] == 0xE5 {
        return PrologueCheck::Prologue {
            detail: "MOV RBP, RSP".into(),
        };
    }
    // sub rsp, #imm (48 83 EC / 48 81 EC)
    if head.len() >= 3 && head[0] == 0x48 && (head[1] == 0x83 || head[1] == 0x81) && head[2] == 0xEC
    {
        return PrologueCheck::Prologue {
            detail: "SUB RSP, #imm (stack ayırma)".into(),
        };
    }
    // mov edi, edi (hotpatch stub): 8B FF
    if head.len() >= 2 && head[0] == 0x8B && head[1] == 0xFF {
        return PrologueCheck::Prologue {
            detail: "MOV EDI, EDI (hotpatch stub)".into(),
        };
    }
    PrologueCheck::NotPrologue {
        warning: "x86 prologue deseni (push rbp / sub rsp / endbr64) bulunamadı — \
                  ortadaki bir komut olabilir, yama çökme riski taşır"
            .into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arm64_stp_frame_is_prologue() {
        // stp x29, x30, [sp, #-16]! = FD 7B BF A9
        assert!(matches!(
            check_arm64(&[0xFD, 0x7B, 0xBF, 0xA9]),
            PrologueCheck::Prologue { .. }
        ));
        // PACIASP
        assert!(matches!(
            check_arm64(&[0x3F, 0x23, 0x03, 0xD5]),
            PrologueCheck::Prologue { .. }
        ));
        // sub sp, sp, #0x20 = FF 83 00 D1
        assert!(matches!(
            check_arm64(&[0xFF, 0x83, 0x00, 0xD1]),
            PrologueCheck::Prologue { .. }
        ));
    }

    #[test]
    fn arm64_random_middle_is_not_prologue() {
        // add w0, w0, #1 = 00 04 00 11 → prologue değil
        assert!(matches!(
            check_arm64(&[0x00, 0x04, 0x00, 0x11]),
            PrologueCheck::NotPrologue { .. }
        ));
    }

    #[test]
    fn arm64_detects_already_patched() {
        assert!(matches!(
            check_arm64(&[0x20, 0x00, 0x80, 0x52, 0xC0, 0x03, 0x5F, 0xD6]),
            PrologueCheck::AlreadyPatched { .. }
        ));
    }

    #[test]
    fn x86_push_rbp_is_prologue() {
        assert!(matches!(
            check_x86(&[0x55, 0x48, 0x89, 0xE5]),
            PrologueCheck::Prologue { .. }
        ));
        assert!(matches!(
            check_x86(&[0xF3, 0x0F, 0x1E, 0xFA]),
            PrologueCheck::Prologue { .. }
        ));
    }

    #[test]
    fn x86_random_is_not_prologue() {
        // add eax, ecx = 01 C8
        assert!(matches!(
            check_x86(&[0x01, 0xC8, 0x90]),
            PrologueCheck::NotPrologue { .. }
        ));
    }

    #[test]
    fn x86_detects_already_patched() {
        assert!(matches!(
            check_x86(&[0xB8, 0x01, 0x00, 0x00, 0x00, 0xC3]),
            PrologueCheck::AlreadyPatched { .. }
        ));
    }
}
