use serde::Serialize;
use std::path::{Path, PathBuf};

/// Standart Android debug keystore parametreleri.
pub const DEBUG_ALIAS: &str = "androiddebugkey";
pub const DEBUG_STOREPASS: &str = "android";
pub const DEBUG_KEYPASS: &str = "android";

/// Bağlı bir ADB cihazı.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct AdbDevice {
    pub serial: String,
    pub state: String,
}

/// `zipalign` argümanları: sayfa hizalama (-p), üzerine yaz (-f), 4 bayt.
pub fn zipalign_args(input: &Path, output: &Path) -> Vec<String> {
    vec![
        "-p".into(),
        "-f".into(),
        "4".into(),
        input.to_string_lossy().into_owned(),
        output.to_string_lossy().into_owned(),
    ]
}

/// `keytool` ile debug keystore üretme argümanları (RSA-2048, uzun geçerlilik).
pub fn keytool_genkey_args(keystore: &Path) -> Vec<String> {
    vec![
        "-genkeypair".into(),
        "-keystore".into(),
        keystore.to_string_lossy().into_owned(),
        "-alias".into(),
        DEBUG_ALIAS.into(),
        "-keyalg".into(),
        "RSA".into(),
        "-keysize".into(),
        "2048".into(),
        "-validity".into(),
        "10000".into(),
        "-storepass".into(),
        DEBUG_STOREPASS.into(),
        "-keypass".into(),
        DEBUG_KEYPASS.into(),
        "-dname".into(),
        "CN=Android Debug,O=Android,C=US".into(),
    ]
}

/// `apksigner sign` argümanları (v1+v2+v3 varsayılan olarak etkin).
pub fn apksigner_sign_args(keystore: &Path, apk: &Path) -> Vec<String> {
    vec![
        "sign".into(),
        "--ks".into(),
        keystore.to_string_lossy().into_owned(),
        "--ks-pass".into(),
        format!("pass:{DEBUG_STOREPASS}"),
        "--ks-key-alias".into(),
        DEBUG_ALIAS.into(),
        "--key-pass".into(),
        format!("pass:{DEBUG_KEYPASS}"),
        apk.to_string_lossy().into_owned(),
    ]
}

/// `adb install -r -d` argümanları; `serial` verilirse `-s <serial>` ön eki.
pub fn adb_install_args(serial: Option<&str>, apk: &Path) -> Vec<String> {
    let mut args = Vec::new();
    if let Some(s) = serial {
        args.push("-s".into());
        args.push(s.to_string());
    }
    args.push("install".into());
    args.push("-r".into());
    args.push("-d".into());
    args.push(apk.to_string_lossy().into_owned());
    args
}

/// `<girdi-adı>-byteforge-signed.apk` çıktı yolunu üretir.
pub fn signed_output_path(input: &Path) -> PathBuf {
    let stem = input
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "output".into());
    let parent = input.parent().unwrap_or_else(|| Path::new("."));
    parent.join(format!("{stem}-byteforge-signed.apk"))
}

/// `adb devices` çıktısını ayrıştırır (ilk başlık satırını atlar).
pub fn parse_adb_devices(output: &str) -> Vec<AdbDevice> {
    output
        .lines()
        .skip(1)
        .filter_map(|line| {
            let mut parts = line.split_whitespace();
            let serial = parts.next()?;
            let state = parts.next()?;
            Some(AdbDevice {
                serial: serial.to_string(),
                state: state.to_string(),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zipalign_args_shape() {
        let a = zipalign_args(Path::new("in.apk"), Path::new("out.apk"));
        assert_eq!(a, vec!["-p", "-f", "4", "in.apk", "out.apk"]);
    }

    #[test]
    fn apksigner_args_include_keystore_and_alias() {
        let a = apksigner_sign_args(Path::new("debug.ks"), Path::new("app.apk"));
        assert_eq!(a[0], "sign");
        assert!(a.contains(&"--ks".to_string()));
        assert!(a.contains(&"debug.ks".to_string()));
        assert!(a.contains(&"pass:android".to_string()));
        assert!(a.contains(&"androiddebugkey".to_string()));
        assert_eq!(a.last().unwrap(), "app.apk");
    }

    #[test]
    fn adb_install_args_with_and_without_serial() {
        let with = adb_install_args(Some("emulator-5554"), Path::new("a.apk"));
        assert_eq!(
            with,
            vec!["-s", "emulator-5554", "install", "-r", "-d", "a.apk"]
        );
        let without = adb_install_args(None, Path::new("a.apk"));
        assert_eq!(without, vec!["install", "-r", "-d", "a.apk"]);
    }

    #[test]
    fn signed_output_path_naming() {
        let p = signed_output_path(Path::new("/tmp/game.apk"));
        assert_eq!(p, PathBuf::from("/tmp/game-byteforge-signed.apk"));
    }

    #[test]
    fn parse_adb_devices_extracts_serial_and_state() {
        let out = "List of devices attached\nemulator-5554\tdevice\nABC123\tunauthorized\n";
        let devices = parse_adb_devices(out);
        assert_eq!(devices.len(), 2);
        assert_eq!(
            devices[0],
            AdbDevice {
                serial: "emulator-5554".into(),
                state: "device".into()
            }
        );
        assert_eq!(devices[1].state, "unauthorized");
    }

    #[test]
    fn parse_adb_devices_empty() {
        assert!(parse_adb_devices("List of devices attached\n").is_empty());
    }
}
