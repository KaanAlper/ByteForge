//! Native disassembly — saf Rust (yaxpeax, C bağımlılığı yok).
//! ARM64 (AArch64) ve x86-64 için bir bayt dizisini mnemonic'lere çevirir.
//! Hex'in yanında "ne yapıyor bu fonksiyon" görünümü (Ghidra'nın disassembly
//! panelinin işlevi; decompiler değil).

use serde::Serialize;

/// Tek bir çözülmüş komut.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct DisasmLine {
    /// Komutun sanal adresi (base + ofset).
    pub address: u64,
    /// Ham baytlar (hex).
    pub bytes: String,
    /// Mnemonic + operandlar (ör. "mov w0, #0x1").
    pub text: String,
}

fn hex(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|b| format!("{b:02X}"))
        .collect::<Vec<_>>()
        .join(" ")
}

/// ARM64 (sabit 4 baytlık komutlar). Geçersiz kelime `.word` olarak işaretlenir.
pub fn disassemble_arm64(code: &[u8], base: u64) -> Vec<DisasmLine> {
    use yaxpeax_arch::{Decoder, U8Reader};
    use yaxpeax_arm::armv8::a64::InstDecoder;

    let decoder = InstDecoder::default();
    let mut out = Vec::new();
    let mut off = 0usize;
    while off + 4 <= code.len() {
        let chunk = &code[off..off + 4];
        let mut reader = U8Reader::new(chunk);
        let text = match decoder.decode(&mut reader) {
            Ok(inst) => inst.to_string(),
            Err(_) => {
                let w = u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
                format!(".word 0x{w:08x}")
            }
        };
        out.push(DisasmLine {
            address: base + off as u64,
            bytes: hex(chunk),
            text,
        });
        off += 4;
    }
    out
}

/// x86-64 (değişken uzunluklu komutlar).
pub fn disassemble_x86(code: &[u8], base: u64) -> Vec<DisasmLine> {
    use yaxpeax_arch::{Decoder, LengthedInstruction, U8Reader};
    use yaxpeax_x86::long_mode::InstDecoder;

    let decoder = InstDecoder::default();
    let mut out = Vec::new();
    let mut off = 0usize;
    while off < code.len() {
        let mut reader = U8Reader::new(&code[off..]);
        match decoder.decode(&mut reader) {
            Ok(inst) => {
                let len = (inst.len().to_const() as usize).max(1);
                let end = (off + len).min(code.len());
                out.push(DisasmLine {
                    address: base + off as u64,
                    bytes: hex(&code[off..end]),
                    text: inst.to_string(),
                });
                off = end;
            }
            Err(_) => {
                out.push(DisasmLine {
                    address: base + off as u64,
                    bytes: hex(&code[off..off + 1]),
                    text: format!(".byte 0x{:02x}", code[off]),
                });
                off += 1;
            }
        }
    }
    out
}

/// Mimari seçerek çözer ("arm64" | "x86").
pub fn disassemble(code: &[u8], base: u64, arch: &str) -> Vec<DisasmLine> {
    match arch {
        "x86" | "x86_64" | "x64" => disassemble_x86(code, base),
        _ => disassemble_arm64(code, base),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arm64_return_true() {
        // MOV W0,#1 ; RET  →  20 00 80 52 C0 03 5F D6
        let code = [0x20, 0x00, 0x80, 0x52, 0xC0, 0x03, 0x5F, 0xD6];
        let d = disassemble_arm64(&code, 0x1000);
        assert_eq!(d.len(), 2);
        assert_eq!(d[0].address, 0x1000);
        assert!(d[0].text.contains("0x1"), "MOV W0,#1 bekleniyor: {}", d[0].text);
        assert!(d[1].text.contains("ret"), "RET bekleniyor: {}", d[1].text);
    }

    #[test]
    fn x86_return_true() {
        // mov eax,1 ; ret  →  B8 01 00 00 00 C3
        let code = [0xB8, 0x01, 0x00, 0x00, 0x00, 0xC3];
        let d = disassemble_x86(&code, 0x400000);
        assert!(d.len() >= 2);
        assert!(d[0].text.contains("mov"), "mov bekleniyor: {}", d[0].text);
        assert!(d.iter().any(|l| l.text.contains("ret")), "ret bekleniyor");
    }
}
