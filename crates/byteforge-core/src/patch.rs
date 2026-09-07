use crate::{CoreError, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// ARM64 `RET` (x30/LR'ye dönüş): D65F03C0 (little-endian).
const RET: [u8; 4] = [0xC0, 0x03, 0x5F, 0xD6];
/// ARM64 `NOP`: D503201F (little-endian).
const NOP: [u8; 4] = [0x1F, 0x20, 0x03, 0xD5];

/// Bir fonksiyon giriş noktasına uygulanabilecek hazır ARM64 yamaları.
/// (ARMv7/Thumb desteği sonraki iterasyonda.)
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum PatchTemplate {
    /// MOV W0,#1; RET — mantıksal doğru döndür.
    ReturnTrue,
    /// MOV W0,#0; RET — mantıksal yanlış / sıfır döndür.
    ReturnFalse,
    /// MOV W0,#n; RET — sabit değer döndür (16-bit).
    ReturnValue(u16),
    /// MOVZ+MOVK ile W0 = 0x7FFFFFFF; RET — maksimum int döndür.
    ReturnMaxInt,
    /// FMOV S0,#1.0; RET — 1.0 float döndür. Oran/ilerleme getter'ını
    /// (ör. ActiveDurationRatio) hep dolu tutar → güç/etki sınırsız olur.
    ReturnOneFloat,
    /// NOP — tek komutu etkisiz kıl.
    Nop,
    /// RET — hemen dön.
    Ret,
}

/// Yamanın hedeflediği komut kümesi mimarisi.
/// `X86`, hem 32-bit x86 hem 64-bit x86-64 için geçerlidir: bu şablonların
/// ürettiği EAX-dönüş opcode'ları iki modda da aynıdır (B8/31C0/C3/90).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Arch {
    /// ARM64 / AArch64 (Android native `.so`, Apple silicon).
    Arm64,
    /// x86 + x86-64 (Windows PE, Linux ELF native).
    X86,
}

/// ARM64 `MOVZ W0, #imm` komutunu üretir: 0x52800000 | (imm << 5).
fn movz_w0(imm: u16) -> [u8; 4] {
    let op: u32 = 0x5280_0000 | ((imm as u32) << 5);
    op.to_le_bytes()
}

/// x86/x64 şablonunun ham makine kodu. Tüm şablonlar EAX üzerinden döner
/// (32-bit dönüş değeri; 64-bit'te üst yarı sıfırlanır — bool/int için doğru).
fn x86_template(t: PatchTemplate) -> Vec<u8> {
    match t {
        // mov eax, 1 ; ret
        PatchTemplate::ReturnTrue => vec![0xB8, 0x01, 0x00, 0x00, 0x00, 0xC3],
        // xor eax, eax ; ret  (en kısa "return 0")
        PatchTemplate::ReturnFalse => vec![0x31, 0xC0, 0xC3],
        // mov eax, imm32 ; ret
        PatchTemplate::ReturnValue(n) => {
            let mut v = vec![0xB8];
            v.extend_from_slice(&(n as u32).to_le_bytes());
            v.push(0xC3);
            v
        }
        // mov eax, 0x7FFFFFFF ; ret
        PatchTemplate::ReturnMaxInt => vec![0xB8, 0xFF, 0xFF, 0xFF, 0x7F, 0xC3],
        // mov eax, 0x3f800000 ; movd xmm0, eax ; ret  → 1.0f (xmm0)
        PatchTemplate::ReturnOneFloat => {
            vec![0xB8, 0x00, 0x00, 0x80, 0x3F, 0x66, 0x0F, 0x6E, 0xC0, 0xC3]
        }
        // nop
        PatchTemplate::Nop => vec![0x90],
        // ret
        PatchTemplate::Ret => vec![0xC3],
    }
}

/// Bir şablonun belirtilen mimaride ürettiği ham makine kodu baytları.
pub fn template_bytes_arch(t: PatchTemplate, arch: Arch) -> Vec<u8> {
    match arch {
        Arch::Arm64 => template_bytes(t),
        Arch::X86 => x86_template(t),
    }
}

/// Bir şablonun ürettiği ham makine kodu baytları.
pub fn template_bytes(t: PatchTemplate) -> Vec<u8> {
    match t {
        PatchTemplate::ReturnTrue => [movz_w0(1).as_slice(), RET.as_slice()].concat(),
        PatchTemplate::ReturnFalse => [movz_w0(0).as_slice(), RET.as_slice()].concat(),
        PatchTemplate::ReturnValue(n) => [movz_w0(n).as_slice(), RET.as_slice()].concat(),
        PatchTemplate::ReturnMaxInt => vec![
            0xE0, 0xFF, 0x9F, 0x52, // movz w0, #0xffff
            0xE0, 0xFF, 0xAF, 0x72, // movk w0, #0x7fff, lsl #16
            0xC0, 0x03, 0x5F, 0xD6, // ret
        ],
        PatchTemplate::ReturnOneFloat => vec![
            0x00, 0x10, 0x2E, 0x1E, // fmov s0, #1.0
            0xC0, 0x03, 0x5F, 0xD6, // ret
        ],
        PatchTemplate::Nop => NOP.to_vec(),
        PatchTemplate::Ret => RET.to_vec(),
    }
}

/// Bir fonksiyonun başındaki (ARM64) baytların bilinen bir yama şablonuyla
/// eşleşip eşleşmediğini tespit eder. Eşleşirse şablon kimliğini döndürür
/// (ör. "return_true"). Sembol tablosunda "MODLU" rozeti için kullanılır.
/// 8 baytlık kalıplar önce denenir (RET/NOP 4 baytlık ön ek olabilir).
pub fn detect_arm64_patch(head: &[u8]) -> Option<&'static str> {
    let ret = template_bytes(PatchTemplate::Ret); // 4B
    let nop = template_bytes(PatchTemplate::Nop); // 4B
    let rt = template_bytes(PatchTemplate::ReturnTrue); // 8B
    let rf = template_bytes(PatchTemplate::ReturnFalse); // 8B
    let rmax = template_bytes(PatchTemplate::ReturnMaxInt); // 12B
    let rone = template_bytes(PatchTemplate::ReturnOneFloat); // 8B

    if head.len() >= rmax.len() && head[..rmax.len()] == rmax[..] {
        return Some("return_max_int");
    }
    if head.len() >= rone.len() && head[..rone.len()] == rone[..] {
        return Some("return_one_float");
    }
    if head.len() >= rt.len() && head[..rt.len()] == rt[..] {
        return Some("return_true");
    }
    if head.len() >= rf.len() && head[..rf.len()] == rf[..] {
        return Some("return_false");
    }
    if head.len() >= ret.len() && head[..ret.len()] == ret[..] {
        return Some("ret");
    }
    if head.len() >= nop.len() && head[..nop.len()] == nop[..] {
        return Some("nop");
    }
    None
}

/// x86/x64 sürümü: bir fonksiyonun başındaki baytlar bilinen bir yama
/// şablonuyla eşleşiyor mu (Windows PE / Linux ELF native). Eşleşirse şablon
/// kimliğini döndürür. Uzun kalıplar önce (C3/90 tek baytlık ön ek olabilir).
pub fn detect_x86_patch(head: &[u8]) -> Option<&'static str> {
    let rt = template_bytes_arch(PatchTemplate::ReturnTrue, Arch::X86); // B8 01000000 C3 (6B)
    let rmax = template_bytes_arch(PatchTemplate::ReturnMaxInt, Arch::X86); // B8 FFFFFF7F C3 (6B)
    let rf = template_bytes_arch(PatchTemplate::ReturnFalse, Arch::X86); // 31 C0 C3 (3B)

    if head.len() >= rt.len() && head[..rt.len()] == rt[..] {
        return Some("return_true");
    }
    if head.len() >= rmax.len() && head[..rmax.len()] == rmax[..] {
        return Some("return_max_int");
    }
    if head.len() >= rf.len() && head[..rf.len()] == rf[..] {
        return Some("return_false");
    }
    // NOP (0x90) ve RET (0xC3) tek başına.
    if head.first() == Some(&0xC3) {
        return Some("ret");
    }
    if head.first() == Some(&0x90) {
        return Some("nop");
    }
    None
}

/// Bir ofsetteki mevcut baytları boşlukla ayrılmış onaltılık dize olarak gösterir.
pub fn hex_dump(bytes: &[u8], offset: usize, len: usize) -> String {
    bytes
        .iter()
        .skip(offset)
        .take(len)
        .map(|b| format!("{b:02X}"))
        .collect::<Vec<_>>()
        .join(" ")
}

/// Bellek-içi yama: orijinali değiştirmeden yamalı yeni bir kopya döndürür.
pub fn apply_patch(original: &[u8], offset: usize, patch: &[u8]) -> Result<Vec<u8>> {
    let end = offset
        .checked_add(patch.len())
        .filter(|&e| e <= original.len())
        .ok_or(CoreError::PatchOutOfBounds {
            offset,
            len: patch.len(),
            file_len: original.len(),
        })?;
    let mut out = original.to_vec();
    out[offset..end].copy_from_slice(patch);
    Ok(out)
}

/// `<path>.bak` yedeği aldıktan sonra dosyayı ofsetten itibaren yamalar.
pub fn write_patched_file(path: &Path, offset: usize, patch: &[u8]) -> Result<()> {
    let original = std::fs::read(path)?;
    let patched = apply_patch(&original, offset, patch)?;

    let mut backup = path.as_os_str().to_owned();
    backup.push(".bak");
    std::fs::write(PathBuf::from(backup), &original)?;
    std::fs::write(path, &patched)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn return_one_float_bytes_verified() {
        // llvm-mc doğruladı: fmov s0,#1.0 = 00 10 2E 1E, ret = C0 03 5F D6
        assert_eq!(
            template_bytes(PatchTemplate::ReturnOneFloat),
            vec![0x00, 0x10, 0x2E, 0x1E, 0xC0, 0x03, 0x5F, 0xD6]
        );
        // x86-64: mov eax,0x3f800000; movd xmm0,eax; ret
        assert_eq!(
            template_bytes_arch(PatchTemplate::ReturnOneFloat, Arch::X86),
            vec![0xB8, 0x00, 0x00, 0x80, 0x3F, 0x66, 0x0F, 0x6E, 0xC0, 0xC3]
        );
        // Kendi çıktısı geri tespit edilmeli.
        assert_eq!(
            detect_arm64_patch(&template_bytes(PatchTemplate::ReturnOneFloat)),
            Some("return_one_float")
        );
    }

    use super::*;

    #[test]
    fn return_true_produces_mov1_ret() {
        assert_eq!(
            template_bytes(PatchTemplate::ReturnTrue),
            vec![0x20, 0x00, 0x80, 0x52, 0xC0, 0x03, 0x5F, 0xD6]
        );
    }

    #[test]
    fn return_false_produces_mov0_ret() {
        assert_eq!(
            template_bytes(PatchTemplate::ReturnFalse),
            vec![0x00, 0x00, 0x80, 0x52, 0xC0, 0x03, 0x5F, 0xD6]
        );
    }

    #[test]
    fn return_value_encodes_immediate() {
        // MOV W0,#5 = 0x528000A0 → A0 00 80 52
        assert_eq!(
            template_bytes(PatchTemplate::ReturnValue(5)),
            vec![0xA0, 0x00, 0x80, 0x52, 0xC0, 0x03, 0x5F, 0xD6]
        );
    }

    #[test]
    fn return_max_int_opcodes() {
        assert_eq!(
            template_bytes(PatchTemplate::ReturnMaxInt),
            vec![0xE0, 0xFF, 0x9F, 0x52, 0xE0, 0xFF, 0xAF, 0x72, 0xC0, 0x03, 0x5F, 0xD6]
        );
    }

    #[test]
    fn nop_and_ret_opcodes() {
        assert_eq!(
            template_bytes(PatchTemplate::Nop),
            vec![0x1F, 0x20, 0x03, 0xD5]
        );
        assert_eq!(
            template_bytes(PatchTemplate::Ret),
            vec![0xC0, 0x03, 0x5F, 0xD6]
        );
    }

    #[test]
    fn x86_return_true_false() {
        // mov eax,1 ; ret
        assert_eq!(
            template_bytes_arch(PatchTemplate::ReturnTrue, Arch::X86),
            vec![0xB8, 0x01, 0x00, 0x00, 0x00, 0xC3]
        );
        // xor eax,eax ; ret
        assert_eq!(
            template_bytes_arch(PatchTemplate::ReturnFalse, Arch::X86),
            vec![0x31, 0xC0, 0xC3]
        );
    }

    #[test]
    fn x86_value_maxint_nop_ret() {
        // mov eax,5 ; ret
        assert_eq!(
            template_bytes_arch(PatchTemplate::ReturnValue(5), Arch::X86),
            vec![0xB8, 0x05, 0x00, 0x00, 0x00, 0xC3]
        );
        // mov eax,0x7FFFFFFF ; ret
        assert_eq!(
            template_bytes_arch(PatchTemplate::ReturnMaxInt, Arch::X86),
            vec![0xB8, 0xFF, 0xFF, 0xFF, 0x7F, 0xC3]
        );
        assert_eq!(
            template_bytes_arch(PatchTemplate::Nop, Arch::X86),
            vec![0x90]
        );
        assert_eq!(
            template_bytes_arch(PatchTemplate::Ret, Arch::X86),
            vec![0xC3]
        );
    }

    #[test]
    fn arch_arm64_matches_default() {
        for t in [
            PatchTemplate::ReturnTrue,
            PatchTemplate::ReturnFalse,
            PatchTemplate::Nop,
            PatchTemplate::Ret,
            PatchTemplate::ReturnMaxInt,
            PatchTemplate::ReturnValue(42),
        ] {
            assert_eq!(template_bytes_arch(t, Arch::Arm64), template_bytes(t));
        }
    }

    #[test]
    fn hex_dump_formats_bytes() {
        assert_eq!(hex_dump(&[0x1F, 0x20, 0x03, 0xD5], 0, 4), "1F 20 03 D5");
        assert_eq!(hex_dump(&[0xAA, 0xBB, 0xCC], 1, 2), "BB CC");
    }

    #[test]
    fn detects_patched_function_heads() {
        assert_eq!(
            detect_arm64_patch(&template_bytes(PatchTemplate::ReturnTrue)),
            Some("return_true")
        );
        assert_eq!(
            detect_arm64_patch(&template_bytes(PatchTemplate::ReturnFalse)),
            Some("return_false")
        );
        assert_eq!(
            detect_arm64_patch(&template_bytes(PatchTemplate::ReturnMaxInt)),
            Some("return_max_int")
        );
        // RET/NOP tek başına
        assert_eq!(detect_arm64_patch(&[0xC0, 0x03, 0x5F, 0xD6]), Some("ret"));
        assert_eq!(detect_arm64_patch(&[0x1F, 0x20, 0x03, 0xD5]), Some("nop"));
    }

    #[test]
    fn normal_code_is_not_detected_as_patch() {
        // Rastgele/olağan fonksiyon başlangıcı (stp x29, x30, ...) → yama değil.
        assert_eq!(
            detect_arm64_patch(&[0xFD, 0x7B, 0xBF, 0xA9, 0xFD, 0x03, 0x00, 0x91]),
            None
        );
        assert_eq!(detect_arm64_patch(&[]), None);
        assert_eq!(detect_arm64_patch(&[0xC0, 0x03]), None); // eksik
    }

    #[test]
    fn detects_x86_patched_heads() {
        assert_eq!(
            detect_x86_patch(&[0xB8, 0x01, 0x00, 0x00, 0x00, 0xC3]),
            Some("return_true")
        );
        assert_eq!(detect_x86_patch(&[0x31, 0xC0, 0xC3]), Some("return_false"));
        assert_eq!(
            detect_x86_patch(&[0xB8, 0xFF, 0xFF, 0xFF, 0x7F, 0xC3]),
            Some("return_max_int")
        );
        assert_eq!(detect_x86_patch(&[0xC3]), Some("ret"));
        assert_eq!(detect_x86_patch(&[0x90, 0x90]), Some("nop"));
        // Olağan prolog (push rbp; mov rbp,rsp) → yama değil.
        assert_eq!(detect_x86_patch(&[0x55, 0x48, 0x89, 0xE5]), None);
        assert_eq!(detect_x86_patch(&[]), None);
    }

    #[test]
    fn apply_patch_is_immutable_and_replaces() {
        let orig = vec![0u8; 16];
        let patched = apply_patch(&orig, 4, &[0xAA, 0xBB]).unwrap();
        assert_eq!(&patched[4..6], &[0xAA, 0xBB]);
        assert_eq!(patched.len(), 16);
        assert_eq!(orig, vec![0u8; 16]); // orijinal değişmedi
    }

    #[test]
    fn apply_patch_rejects_out_of_bounds() {
        assert!(apply_patch(&[0u8; 4], 3, &[1, 2, 3]).is_err());
        assert!(apply_patch(&[0u8; 4], usize::MAX, &[1]).is_err());
    }

    #[test]
    fn write_patched_file_creates_backup() {
        let f = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(f.path(), [0u8; 8]).unwrap();

        write_patched_file(f.path(), 0, &template_bytes(PatchTemplate::Nop)).unwrap();

        let patched = std::fs::read(f.path()).unwrap();
        assert_eq!(&patched[0..4], &[0x1F, 0x20, 0x03, 0xD5]);

        let mut bak = f.path().as_os_str().to_owned();
        bak.push(".bak");
        let backup = std::fs::read(PathBuf::from(bak)).unwrap();
        assert_eq!(backup, vec![0u8; 8]);
    }
}
