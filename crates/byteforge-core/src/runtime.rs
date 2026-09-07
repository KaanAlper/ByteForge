use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeKind {
    UnityIl2cpp,
    UnityMono,
    Flutter,
    ReactNative,
    Unreal,
    Godot,
    JavaKotlin,
    Unknown,
}

/// Arşiv giriş adlarından uygulama çalışma zamanını tespit eder.
/// Öncelik sırası: IL2CPP > Unity(Mono) > Flutter > React Native > Java/Kotlin.
///
/// Eşleşmeler alt-dizge değil, dosya-adı/yol yapısı üzerindendir; böylece
/// `assets/backup_libil2cpp.so.old` gibi girişler yanlış tespite yol açmaz.
pub fn detect_runtime(entries: &[String]) -> RuntimeKind {
    fn base(e: &str) -> &str {
        e.rsplit('/').next().unwrap_or(e)
    }
    let has_file = |name: &str| entries.iter().any(|e| base(e) == name);
    let has_prefix = |pre: &str| entries.iter().any(|e| e.starts_with(pre));

    if has_file("libil2cpp.so") {
        return RuntimeKind::UnityIl2cpp;
    }
    if has_prefix("assets/bin/Data/Managed/") {
        return RuntimeKind::UnityMono;
    }
    if has_file("libflutter.so") || has_file("kernel_blob.bin") {
        return RuntimeKind::Flutter;
    }
    if has_file("libreactnativejni.so") || has_file("index.android.bundle") {
        return RuntimeKind::ReactNative;
    }
    if has_file("libUE4.so") || has_file("libUnreal.so") {
        return RuntimeKind::Unreal;
    }
    if has_file("libgodot_android.so")
        || has_file("libgodot.so")
        || entries.iter().any(|e| e.ends_with(".pck"))
    {
        return RuntimeKind::Godot;
    }
    if entries.iter().any(|e| {
        let b = base(e);
        b.starts_with("classes") && b.ends_with(".dex")
    }) {
        return RuntimeKind::JavaKotlin;
    }
    RuntimeKind::Unknown
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|x| x.to_string()).collect()
    }

    #[test]
    fn detects_il2cpp() {
        let e = s(&["lib/arm64-v8a/libil2cpp.so", "assets/bin/Data/Managed/x"]);
        assert_eq!(detect_runtime(&e), RuntimeKind::UnityIl2cpp);
    }

    #[test]
    fn detects_unity_mono() {
        let e = s(&["assets/bin/Data/Managed/Assembly-CSharp.dll"]);
        assert_eq!(detect_runtime(&e), RuntimeKind::UnityMono);
    }

    #[test]
    fn detects_flutter() {
        let e = s(&[
            "lib/arm64-v8a/libflutter.so",
            "assets/flutter_assets/kernel_blob.bin",
        ]);
        assert_eq!(detect_runtime(&e), RuntimeKind::Flutter);
    }

    #[test]
    fn detects_react_native() {
        let e = s(&["lib/arm64-v8a/libreactnativejni.so"]);
        assert_eq!(detect_runtime(&e), RuntimeKind::ReactNative);
        let e2 = s(&["assets/index.android.bundle"]);
        assert_eq!(detect_runtime(&e2), RuntimeKind::ReactNative);
    }

    #[test]
    fn detects_java_kotlin() {
        let e = s(&["classes.dex", "classes2.dex", "AndroidManifest.xml"]);
        assert_eq!(detect_runtime(&e), RuntimeKind::JavaKotlin);
    }

    #[test]
    fn unknown_when_no_signal() {
        let e = s(&["res/x", "AndroidManifest.xml"]);
        assert_eq!(detect_runtime(&e), RuntimeKind::Unknown);
    }

    #[test]
    fn detects_unreal_and_godot() {
        assert_eq!(
            detect_runtime(&s(&["lib/arm64-v8a/libUE4.so", "classes.dex"])),
            RuntimeKind::Unreal
        );
        assert_eq!(
            detect_runtime(&s(&["lib/arm64-v8a/libgodot_android.so"])),
            RuntimeKind::Godot
        );
        assert_eq!(
            detect_runtime(&s(&["assets/game.pck", "classes.dex"])),
            RuntimeKind::Godot
        );
    }

    #[test]
    fn no_false_positive_from_substring_name() {
        // Alt-dizge olarak libil2cpp içerir ama gerçek native kütüphane değil.
        let e = s(&["assets/backup_libil2cpp.so.old", "classes.dex"]);
        assert_eq!(detect_runtime(&e), RuntimeKind::JavaKotlin);
    }
}
