//! Mini-decompiler (native Rust) — Ghidra'nın decompiler mimarisini modelleyen
//! odaklı bir sürüm. Ghidra hattı: lift (P-code) → heritage/veri-akışı (SSA) →
//! sadeleştirme kuralları → yapılandırma → C üretimi. Burada aynı fikir, ama
//! basit fonksiyonlara (getter/setter/sabit-döndür/basit aritmetik-float)
//! odaklı: bu hedeflerin çoğu mod hedefidir (IsSkinOwned, GetCurrency,
//! get_ActiveDurationRatio...). Karmaşık akış (döngü, çok dallı, çağrı) →
//! "karmaşık" işaretlenir, kullanıcı disassembly/Ghidra'ya yönlendirilir.
//!
//! Kapsam: ARM64 (AArch64) düz-akışlı fonksiyonlar. yaxpeax ile operand
//! seviyesinde lift edilir (string ayrıştırma değil).

use serde::Serialize;
use yaxpeax_arch::{Decoder, U8Reader};
use yaxpeax_arm::armv8::a64::{InstDecoder, Opcode, Operand, SizeCode};

/// Sembolik ifade — Ghidra'nın varnode/expr ağacının mini karşılığı.
#[derive(Debug, Clone)]
enum Expr {
    /// Tamsayı sabit.
    Const(i64),
    /// Float sabit.
    FConst(f64),
    /// Gelen argüman register'ı (x0=this, x1=a1, ...).
    Arg(u8),
    /// Bellek yüklemesi (LOAD): base->field_0xNN.
    Field {
        base: Box<Expr>,
        off: u32,
        float: bool,
        size: u8,
    },
    /// İkili işlem.
    Bin {
        op: &'static str,
        l: Box<Expr>,
        r: Box<Expr>,
    },
    /// Çözülemeyen (çağrı sonucu, desteklenmeyen komut).
    Unknown(&'static str),
}

impl Expr {
    /// C benzeri metin üretimi (Ghidra'nın PrintC'sinin mini hali).
    fn emit(&self) -> String {
        match self {
            Expr::Const(n) => {
                if *n == 0x7fff_ffff {
                    "INT_MAX".into()
                } else if *n < 0 || *n > 0xffff {
                    format!("0x{:x}", *n)
                } else {
                    n.to_string()
                }
            }
            Expr::FConst(f) => {
                if f.fract() == 0.0 {
                    format!("{f:.1}f")
                } else {
                    format!("{f}f")
                }
            }
            Expr::Arg(0) => "this".into(),
            Expr::Arg(n) => format!("a{n}"),
            Expr::Field { base, off, .. } => {
                let b = base.emit();
                format!("{b}->field_0x{off:x}")
            }
            Expr::Bin { op, l, r } => format!("({} {} {})", l.emit(), op, r.emit()),
            Expr::Unknown(s) => format!("/*{s}*/").to_string(),
        }
    }
    fn is_arg(&self) -> bool {
        matches!(self, Expr::Arg(_))
    }
}

/// Çözümlenmiş fonksiyon.
#[derive(Debug, Clone, Serialize)]
pub struct Decompiled {
    /// Tahmini imza (dönüş tipi + ad).
    pub signature: String,
    /// C benzeri gövde.
    pub pseudocode: String,
    /// Güven/tür etiketi (ör. "basit getter").
    pub note: String,
    /// Karmaşık akış → tam çözülemedi.
    pub complex: bool,
}

const GP: usize = 32;
const SIMD: usize = 32;

/// Basit bir ARM64 fonksiyonunu psödokoda çevirir.
pub fn decompile_arm64(code: &[u8], name: &str) -> Decompiled {
    let decoder = InstDecoder::default();

    // Register durumu (heritage-lite): her register'ın sembolik değeri.
    // GP 0..30, 31 = ZR (okumada 0). SIMD 0..31.
    let mut gp: Vec<Option<Expr>> = vec![None; GP];
    let mut simd: Vec<Option<Expr>> = vec![None; SIMD];
    for i in 0..8u8 {
        gp[i as usize] = Some(Expr::Arg(i)); // x0..x7 = argümanlar
    }

    // Son store (setter tespiti için): (base_expr, off, value).
    let mut last_store: Option<(Expr, u32, Expr)> = None;
    let mut used_float_ret = false;
    let mut complex = false;

    let read_gp = |gp: &Vec<Option<Expr>>, n: u16| -> Expr {
        if n == 31 {
            Expr::Const(0) // ZR
        } else {
            gp.get(n as usize).and_then(|x| x.clone()).unwrap_or(Expr::Unknown("?"))
        }
    };
    let read_simd = |simd: &Vec<Option<Expr>>, n: u16| -> Expr {
        simd.get(n as usize).and_then(|x| x.clone()).unwrap_or(Expr::Unknown("?"))
    };

    let mut off = 0usize;
    let mut instrs = 0;
    while off + 4 <= code.len() && instrs < 64 {
        let chunk = &code[off..off + 4];
        let mut reader = U8Reader::new(chunk);
        let inst = match decoder.decode(&mut reader) {
            Ok(i) => i,
            Err(_) => {
                off += 4;
                instrs += 1;
                continue;
            }
        };
        instrs += 1;
        let ops = &inst.operands;

        match inst.opcode {
            // --- return ---
            Opcode::RET => break,

            // --- MOV/MOVZ (sabit) ---
            Opcode::MOVZ => {
                if let (Operand::Register(_, d), Operand::ImmShift(imm, sh)) = (&ops[0], &ops[1]) {
                    set_gp(&mut gp, *d, Expr::Const((*imm as i64) << *sh));
                }
            }
            Opcode::MOVK => {
                if let (Operand::Register(_, d), Operand::ImmShift(imm, sh)) = (&ops[0], &ops[1]) {
                    // Mevcut sabitin üstüne yaz (MOVZ+MOVK = 32-bit sabit).
                    let base = match read_gp(&gp, *d) {
                        Expr::Const(c) => c,
                        _ => 0,
                    };
                    let mask = !(0xffffi64 << *sh);
                    set_gp(&mut gp, *d, Expr::Const((base & mask) | ((*imm as i64) << *sh)));
                }
            }
            // ORR Wd, WZR, #imm  → sabit (mov takma adı)
            Opcode::ORR => {
                if let (Operand::Register(_, d), Operand::Register(_, 31), Operand::Immediate(imm)) =
                    (&ops[0], &ops[1], &ops[2])
                {
                    set_gp(&mut gp, *d, Expr::Const(*imm as i64));
                } else if let (Operand::Register(_, d), Operand::Register(_, 31), Operand::Register(_, n)) = (&ops[0], &ops[1], &ops[2]) {
                    let v = read_gp(&gp, *n);
                    set_gp(&mut gp, *d, v);
                } else if let Operand::Register(_, d) = &ops[0] {
                    set_gp(&mut gp, *d, Expr::Unknown("or"));
                }
            }

            // --- LOAD (getter çekirdeği) ---
            Opcode::LDR | Opcode::LDRB | Opcode::LDRSB | Opcode::LDRH | Opcode::LDRSW => {
                let size: u8 = match inst.opcode {
                    Opcode::LDRB | Opcode::LDRSB => 1,
                    Opcode::LDRH => 2,
                    Opcode::LDRSW => 4,
                    _ => if matches!(ops[0], Operand::Register(SizeCode::X, _)) { 8 } else { 4 },
                };
                if let (Operand::Register(_, d), Operand::RegPreIndex(base, ioff, _)) =
                    (&ops[0], &ops[1])
                {
                    let base_e = read_gp(&gp, *base);
                    set_gp(
                        &mut gp,
                        *d,
                        Expr::Field {
                            base: Box::new(base_e),
                            off: *ioff as u32,
                            float: false,
                            size,
                        },
                    );
                }
            }
            // --- STORE (setter çekirdeği) ---
            Opcode::STR | Opcode::STRB | Opcode::STRH => {
                if let (val_op, Operand::RegPreIndex(base, ioff, _)) = (&ops[0], &ops[1]) {
                    let base_e = read_gp(&gp, *base);
                    let v = match val_op {
                        Operand::Register(_, n) => read_gp(&gp, *n),
                        Operand::SIMDRegister(_, n) => read_simd(&simd, *n),
                        _ => Expr::Unknown("?"),
                    };
                    last_store = Some((base_e, *ioff as u32, v));
                }
            }

            // --- INT aritmetik ---
            Opcode::ADD | Opcode::SUB => {
                let opsym = if inst.opcode == Opcode::ADD { "+" } else { "-" };
                if let Operand::Register(_, d) = &ops[0] {
                    let a = op_val(&ops[1], &gp, &simd, &read_gp, &read_simd);
                    let b = op_val(&ops[2], &gp, &simd, &read_gp, &read_simd);
                    set_gp(&mut gp, *d, fold_bin(opsym, a, b));
                }
            }

            // --- FLOAT ---
            Opcode::FMOV => {
                if let Operand::SIMDRegister(_, d) = &ops[0] {
                    used_float_ret = true;
                    let v = match &ops[1] {
                        Operand::ImmShift(imm, _) => Expr::FConst(f64::from_bits(*imm as u64)),
                        Operand::SIMDRegister(_, n) => read_simd(&simd, *n),
                        Operand::Register(_, 31) => Expr::FConst(0.0),
                        // fmov s0, #1.0 → yaxpeax bunu Imm olarak verir; kaba tahmin:
                        _ => Expr::Unknown("fmov"),
                    };
                    set_simd(&mut simd, *d, v);
                }
            }
            Opcode::FADD | Opcode::FSUB | Opcode::FMUL | Opcode::FDIV => {
                used_float_ret = true;
                let sym = match inst.opcode {
                    Opcode::FADD => "+",
                    Opcode::FSUB => "-",
                    Opcode::FMUL => "*",
                    _ => "/",
                };
                if let (
                    Operand::SIMDRegister(_, d),
                    Operand::SIMDRegister(_, a),
                    Operand::SIMDRegister(_, b),
                ) = (&ops[0], &ops[1], &ops[2])
                {
                    let ae = read_simd(&simd, *a);
                    let be = read_simd(&simd, *b);
                    set_simd(&mut simd, *d, fold_bin(sym, ae, be));
                }
            }

            // --- IL2CPP init-guard: erken + küçük ileri tbnz/tbz → init bloğunu
            //     atla (gerçek dallanma değil, boilerplate). Hedefe zıpla. ---
            Opcode::TBNZ | Opcode::TBZ => {
                if instrs <= 10 {
                    if let Operand::PCOffset(delta) = ops[2] {
                        if delta > 0 && delta <= 0x40 {
                            off = (off as i64 + delta) as usize;
                            continue; // off güncellendi; +4 yapma
                        }
                    }
                }
                complex = true;
            }

            // --- gerçek kontrol akışı / çağrı → karmaşık ---
            Opcode::Bcc(_)
            | Opcode::B
            | Opcode::BL
            | Opcode::BLR
            | Opcode::BR
            | Opcode::CBZ
            | Opcode::CBNZ => {
                complex = true;
            }

            _ => {
                // desteklenmeyen komut — hedef register'ı bilinmez yap
                if let Operand::Register(_, d) = &ops[0] {
                    set_gp(&mut gp, *d, Expr::Unknown("?"));
                } else if let Operand::SIMDRegister(_, d) = &ops[0] {
                    set_simd(&mut simd, *d, Expr::Unknown("?"));
                }
            }
        }
        off += 4;
    }

    // --- Üretim (PrintC-lite) ---
    let low = name.to_lowercase();
    let bool_hint = low.starts_with("get_is")
        || low.starts_with("is")
        || low.starts_with("has")
        || low.starts_with("can")
        || low.contains("owned")
        || low.contains("unlock");

    // Setter mı? (parametreli, bir alana yazıyor, dönüş yok)
    if let Some((base, off, val)) = &last_store {
        if base.is_arg() && gp[0].as_ref().map(|_| true).unwrap_or(false) {
            let sig = format!("void {name}({}, T value)", "T this");
            let body = format!("    {}->field_0x{off:x} = {};", base.emit(), val.emit());
            return Decompiled {
                signature: sig,
                pseudocode: format!("{{\n{body}\n}}"),
                note: "basit setter".into(),
                complex,
            };
        }
    }

    // Dönüş değeri: float mı int mi?
    let ret_expr = if used_float_ret {
        simd[0].clone()
    } else {
        gp[0].clone()
    };
    let (ret_ty, note) = match &ret_expr {
        Some(Expr::Const(0)) | Some(Expr::Const(1)) if bool_hint => ("bool", "sabit döndürür"),
        Some(Expr::Const(_)) => ("int", "sabit döndürür"),
        Some(Expr::FConst(_)) => ("float", "sabit float döndürür"),
        Some(Expr::Field { float: true, .. }) => ("float", "basit getter"),
        Some(Expr::Field { size: 1, .. }) if bool_hint => ("bool", "basit getter"),
        Some(Expr::Field { .. }) => ("int", "basit getter"),
        Some(Expr::Bin { .. }) if used_float_ret => ("float", "hesaplanmış float"),
        Some(Expr::Bin { .. }) => ("int", "hesaplanmış değer"),
        _ => ("int", "çözülemedi"),
    };

    let ret_str = match &ret_expr {
        Some(e) => e.emit(),
        None => "0".into(),
    };
    // bool + sabit: true/false
    let ret_str = if ret_ty == "bool" {
        match &ret_expr {
            Some(Expr::Const(0)) => "false".into(),
            Some(Expr::Const(1)) => "true".into(),
            _ => ret_str,
        }
    } else {
        ret_str
    };

    let complex = complex || matches!(ret_expr, Some(Expr::Unknown(_)) | None);
    let body = if complex && matches!(ret_expr, Some(Expr::Unknown(_)) | None) {
        "    // karmaşık akış — tam çözüm için disassembly'ye bakın".to_string()
    } else {
        format!("    return {ret_str};")
    };

    Decompiled {
        signature: format!("{ret_ty} {name}(T this)"),
        pseudocode: format!("{{\n{body}\n}}"),
        note: note.into(),
        complex,
    }
}

fn set_gp(gp: &mut [Option<Expr>], n: u16, e: Expr) {
    if (n as usize) < gp.len() && n != 31 {
        gp[n as usize] = Some(e);
    }
}
fn set_simd(simd: &mut [Option<Expr>], n: u16, e: Expr) {
    if (n as usize) < simd.len() {
        simd[n as usize] = Some(e);
    }
}

fn op_val(
    op: &Operand,
    gp: &Vec<Option<Expr>>,
    simd: &Vec<Option<Expr>>,
    read_gp: &impl Fn(&Vec<Option<Expr>>, u16) -> Expr,
    read_simd: &impl Fn(&Vec<Option<Expr>>, u16) -> Expr,
) -> Expr {
    match op {
        Operand::Register(_, n) => read_gp(gp, *n),
        Operand::SIMDRegister(_, n) => read_simd(simd, *n),
        Operand::Immediate(i) => Expr::Const(*i as i64),
        Operand::ImmShift(i, s) => Expr::Const((*i as i64) << *s),
        _ => Expr::Unknown("?"),
    }
}

/// Sabit katlama (Ghidra'nın TrivialArith kuralının mini hali).
fn fold_bin(op: &'static str, l: Expr, r: Expr) -> Expr {
    if let (Expr::Const(a), Expr::Const(b)) = (&l, &r) {
        return match op {
            "+" => Expr::Const(a + b),
            "-" => Expr::Const(a - b),
            "*" => Expr::Const(a * b),
            _ => Expr::Bin { op, l: Box::new(l), r: Box::new(r) },
        };
    }
    Expr::Bin { op, l: Box::new(l), r: Box::new(r) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn const_true_getter() {
        // MOV W0,#1 ; RET
        let code = [0x20, 0x00, 0x80, 0x52, 0xC0, 0x03, 0x5F, 0xD6];
        let d = decompile_arm64(&code, "get_IsPremium");
        assert!(d.pseudocode.contains("return true"), "psödokod: {}", d.pseudocode);
        assert!(d.signature.starts_with("bool"));
    }

    #[test]
    fn int_getter() {
        // LDR W0,[X0,#0x38] ; RET
        let code = [0x00, 0x38, 0x40, 0xB9, 0xC0, 0x03, 0x5F, 0xD6];
        let d = decompile_arm64(&code, "get_Count");
        assert!(
            d.pseudocode.contains("this->field_0x38"),
            "psödokod: {}",
            d.pseudocode
        );
    }
}
