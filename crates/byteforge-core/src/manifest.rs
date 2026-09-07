use crate::Result;
use axmldecoder::{Element, Node, XmlDocument};
use serde::Serialize;

const ANDROID_NAME: &str = "android:name";

/// AndroidManifest.xml'den çıkarılan üst düzey bilgiler.
/// axmldecoder resource ID'leri çözmediği için resource-referanslı değerler
/// (ör. etiket/simge) burada tutulmaz; yalnızca düz metin/sayı değerler alınır.
#[derive(Debug, Clone, Default, Serialize)]
pub struct ManifestInfo {
    pub package: Option<String>,
    pub version_code: Option<String>,
    pub version_name: Option<String>,
    pub min_sdk: Option<String>,
    pub target_sdk: Option<String>,
    pub debuggable: bool,
    pub permissions: Vec<String>,
    pub launchable_activities: Vec<String>,
}

/// Bir binary AndroidManifest.xml (AXML) baytlarını ayrıştırıp özet çıkarır.
pub fn parse_manifest(bytes: &[u8]) -> Result<ManifestInfo> {
    let doc: XmlDocument = axmldecoder::parse(bytes)?;
    let mut info = ManifestInfo::default();

    let Some(Node::Element(root)) = doc.get_root() else {
        return Ok(info);
    };

    let attrs = root.get_attributes();
    info.package = attrs.get("package").cloned();
    info.version_code = attrs.get("android:versionCode").cloned();
    info.version_name = attrs.get("android:versionName").cloned();

    for child in root.get_children() {
        let Node::Element(el) = child else { continue };
        match el.get_tag() {
            "uses-sdk" => {
                let a = el.get_attributes();
                info.min_sdk = a.get("android:minSdkVersion").cloned();
                info.target_sdk = a.get("android:targetSdkVersion").cloned();
            }
            "uses-permission" | "uses-permission-sdk-23" => {
                if let Some(name) = el.get_attributes().get(ANDROID_NAME) {
                    info.permissions.push(name.clone());
                }
            }
            "application" => {
                info.debuggable = el
                    .get_attributes()
                    .get("android:debuggable")
                    .is_some_and(|v| v == "true");
                collect_launchables(el, &mut info.launchable_activities);
            }
            _ => {}
        }
    }

    Ok(info)
}

/// <application> altındaki, MAIN + LAUNCHER intent-filter'ı olan activity'leri toplar.
fn collect_launchables(application: &Element, out: &mut Vec<String>) {
    for child in application.get_children() {
        let Node::Element(el) = child else { continue };
        let tag = el.get_tag();
        if (tag == "activity" || tag == "activity-alias") && is_launcher(el) {
            if let Some(name) = el.get_attributes().get(ANDROID_NAME) {
                out.push(name.clone());
            }
        }
    }
}

/// Bir activity'nin MAIN action + LAUNCHER category taşıyan intent-filter'ı var mı?
fn is_launcher(activity: &Element) -> bool {
    for child in activity.get_children() {
        let Node::Element(filter) = child else {
            continue;
        };
        if filter.get_tag() != "intent-filter" {
            continue;
        }
        let mut has_main = false;
        let mut has_launcher = false;
        for fc in filter.get_children() {
            let Node::Element(e) = fc else { continue };
            let name = e.get_attributes().get(ANDROID_NAME).map(|s| s.as_str());
            match e.get_tag() {
                "action" if name == Some("android.intent.action.MAIN") => has_main = true,
                "category" if name == Some("android.intent.category.LAUNCHER") => {
                    has_launcher = true
                }
                _ => {}
            }
        }
        if has_main && has_launcher {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> ManifestInfo {
        let bytes = include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/AndroidManifest.xml"
        ));
        parse_manifest(bytes).unwrap()
    }

    #[test]
    fn extracts_package_and_version() {
        let m = fixture();
        assert_eq!(m.package.as_deref(), Some("org.t0t0.androguard.TC"));
        assert_eq!(m.version_code.as_deref(), Some("1"));
        assert_eq!(m.version_name.as_deref(), Some("1.0"));
    }

    #[test]
    fn detects_debuggable_flag() {
        assert!(fixture().debuggable);
    }

    #[test]
    fn finds_launchable_activity() {
        assert_eq!(
            fixture().launchable_activities,
            vec!["TCActivity".to_string()]
        );
    }

    #[test]
    fn absent_fields_are_empty() {
        let m = fixture();
        assert!(m.permissions.is_empty());
        assert_eq!(m.min_sdk, None);
        assert_eq!(m.target_sdk, None);
    }

    #[test]
    fn errors_on_non_axml() {
        assert!(parse_manifest(b"this is not binary xml").is_err());
    }
}
