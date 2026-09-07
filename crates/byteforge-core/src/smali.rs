use crate::{CoreError, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;

/// `apktool d -f <apk> -o <out_dir>` argümanları (mevcut çıktının üzerine yaz).
pub fn apktool_decode_args(apk: &Path, out_dir: &Path) -> Vec<String> {
    vec![
        "d".into(),
        "-f".into(),
        apk.to_string_lossy().into_owned(),
        "-o".into(),
        out_dir.to_string_lossy().into_owned(),
    ]
}

/// `apktool b <dir> -o <out_apk>` argümanları.
pub fn apktool_build_args(dir: &Path, out_apk: &Path) -> Vec<String> {
    vec![
        "b".into(),
        dir.to_string_lossy().into_owned(),
        "-o".into(),
        out_apk.to_string_lossy().into_owned(),
    ]
}

/// Bir Smali dosyasındaki tek bir metod.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SmaliMethod {
    /// Metod imzası, ör. `isPremium()Z` veya `<init>()V`.
    pub signature: String,
    /// JVM dönüş tanımlayıcısı, ör. `Z`, `V`, `Ljava/lang/String;`.
    pub return_type: String,
    /// `.method` satırının 0-tabanlı indeksi.
    pub start_line: usize,
    /// `.end method` satırının 0-tabanlı indeksi.
    pub end_line: usize,
}

/// Bir metodun gövdesinin değiştirileceği zorunlu dönüş biçimi.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ForcedReturn {
    /// `const/4 v0, 0x1; return v0` — Z/B/S/C/I dönüşleri.
    True,
    /// `const/4 v0, 0x0; return v0` — Z/B/S/C/I dönüşleri.
    False,
    /// `return-void` — yalnızca V dönüşü.
    Void,
    /// `const/4 v0, 0x0; return-object v0` — nesne/dizi dönüşleri.
    Null,
    /// `const v0, 0x7fffffff; return v0` — int dönüşü MAX_INT yapar (para/sayaç).
    MaxInt,
}

/// `.method` satırından imza ve dönüş tanımlayıcısını çıkarır.
fn parse_method_line(line: &str) -> Option<(String, String)> {
    let sig = line.split_whitespace().find(|t| t.contains('('))?;
    let ret = sig.rsplit(')').next()?.to_string();
    if ret.is_empty() {
        return None;
    }
    Some((sig.to_string(), ret))
}

/// Bir Smali dosyasındaki tüm metodları bulur.
pub fn find_methods(text: &str) -> Vec<SmaliMethod> {
    let lines: Vec<&str> = text.lines().collect();
    let mut methods = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let trimmed = lines[i].trim_start();
        if trimmed.starts_with(".method") {
            if let Some((signature, return_type)) = parse_method_line(trimmed) {
                if let Some(end) =
                    (i + 1..lines.len()).find(|&j| lines[j].trim_start().starts_with(".end method"))
                {
                    methods.push(SmaliMethod {
                        signature,
                        return_type,
                        start_line: i,
                        end_line: end,
                    });
                    i = end + 1;
                    continue;
                }
            }
        }
        i += 1;
    }
    methods
}

/// Zorunlu dönüşün dönüş tipiyle uyumlu olduğunu doğrular ve gövde satırlarını üretir.
fn forced_body(return_type: &str, forced: ForcedReturn) -> Result<(usize, Vec<String>)> {
    let is_int_like = matches!(return_type, "Z" | "B" | "S" | "C" | "I");
    let is_object = return_type.starts_with('L') || return_type.starts_with('[');
    match forced {
        ForcedReturn::True | ForcedReturn::False if is_int_like => {
            let v = if matches!(forced, ForcedReturn::True) {
                "0x1"
            } else {
                "0x0"
            };
            Ok((
                1,
                vec![format!("    const/4 v0, {v}"), "    return v0".into()],
            ))
        }
        ForcedReturn::MaxInt if is_int_like => Ok((
            1,
            vec!["    const v0, 0x7fffffff".into(), "    return v0".into()],
        )),
        ForcedReturn::Void if return_type == "V" => Ok((0, vec!["    return-void".into()])),
        ForcedReturn::Null if is_object => Ok((
            1,
            vec!["    const/4 v0, 0x0".into(), "    return-object v0".into()],
        )),
        _ => Err(CoreError::SmaliPatch(format!(
            "seçilen dönüş '{return_type}' tipiyle uyumsuz"
        ))),
    }
}

/// Verilen imzalı metodun gövdesini zorunlu dönüşle değiştirir.
pub fn apply_forced_return(
    text: &str,
    method_signature: &str,
    forced: ForcedReturn,
) -> Result<String> {
    let methods = find_methods(text);
    let target = methods
        .iter()
        .find(|m| m.signature == method_signature)
        .ok_or_else(|| CoreError::SmaliPatch(format!("metod bulunamadı: {method_signature}")))?;

    let (locals, body) = forced_body(&target.return_type, forced)?;

    let lines: Vec<&str> = text.lines().collect();
    let mut out: Vec<String> = Vec::with_capacity(lines.len());
    out.extend(lines[..target.start_line].iter().map(|s| s.to_string()));
    out.push(lines[target.start_line].to_string()); // orijinal .method satırı
    out.push(format!("    .locals {locals}"));
    out.extend(body);
    out.push(lines[target.end_line].to_string()); // .end method
    out.extend(lines[target.end_line + 1..].iter().map(|s| s.to_string()));

    Ok(out.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#".class public Lcom/example/License;
.super Ljava/lang/Object;

.method public isPremium()Z
    .registers 2
    const/4 v0, 0x0
    return v0
.end method

.method public getName()Ljava/lang/String;
    .registers 2
    const-string v0, "free"
    return-object v0
.end method

.method public logout()V
    .registers 1
    return-void
.end method
"#;

    #[test]
    fn finds_all_methods_with_return_types() {
        let m = find_methods(SAMPLE);
        assert_eq!(m.len(), 3);
        assert_eq!(m[0].signature, "isPremium()Z");
        assert_eq!(m[0].return_type, "Z");
        assert_eq!(m[1].signature, "getName()Ljava/lang/String;");
        assert_eq!(m[1].return_type, "Ljava/lang/String;");
        assert_eq!(m[2].return_type, "V");
    }

    #[test]
    fn forces_max_int_on_int_return() {
        let src = r#".class public Lc;
.super Ljava/lang/Object;
.method public getCoins()I
    .registers 2
    const/4 v0, 0x0
    return v0
.end method
"#;
        let out = apply_forced_return(src, "getCoins()I", ForcedReturn::MaxInt).unwrap();
        assert!(out.contains("const v0, 0x7fffffff"));
        assert!(out.contains("return v0"));
        // Void metoda MaxInt uygulanamaz.
        assert!(apply_forced_return(SAMPLE, "logout()V", ForcedReturn::MaxInt).is_err());
    }

    #[test]
    fn forces_boolean_true() {
        let out = apply_forced_return(SAMPLE, "isPremium()Z", ForcedReturn::True).unwrap();
        assert!(out.contains("const/4 v0, 0x1"));
        // Yalnızca hedef metod değişti; diğerleri korunur.
        assert!(out.contains("const-string v0, \"free\""));
        let methods = find_methods(&out);
        assert_eq!(methods.len(), 3);
    }

    #[test]
    fn forces_object_null() {
        let out =
            apply_forced_return(SAMPLE, "getName()Ljava/lang/String;", ForcedReturn::Null).unwrap();
        assert!(out.contains("return-object v0"));
        assert!(out.contains("const/4 v0, 0x0"));
    }

    #[test]
    fn forces_void() {
        let out = apply_forced_return(SAMPLE, "logout()V", ForcedReturn::Void).unwrap();
        assert!(out.contains("return-void"));
    }

    #[test]
    fn rejects_incompatible_return() {
        // Void metoda True uygulanamaz.
        assert!(apply_forced_return(SAMPLE, "logout()V", ForcedReturn::True).is_err());
        // Object metoda True uygulanamaz.
        assert!(
            apply_forced_return(SAMPLE, "getName()Ljava/lang/String;", ForcedReturn::True).is_err()
        );
    }

    #[test]
    fn errors_on_missing_method() {
        assert!(apply_forced_return(SAMPLE, "nope()V", ForcedReturn::Void).is_err());
    }

    #[test]
    fn apktool_arg_shapes() {
        let d = apktool_decode_args(Path::new("a.apk"), Path::new("out"));
        assert_eq!(d, vec!["d", "-f", "a.apk", "-o", "out"]);
        let b = apktool_build_args(Path::new("out"), Path::new("new.apk"));
        assert_eq!(b, vec!["b", "out", "-o", "new.apk"]);
    }
}
