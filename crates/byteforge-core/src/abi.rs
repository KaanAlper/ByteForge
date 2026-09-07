/// `lib/<abi>/...` girişlerinden benzersiz ABI listesini (sıralı) çıkarır.
pub fn detect_abis(entries: &[String]) -> Vec<String> {
    let mut abis: Vec<String> = entries
        .iter()
        .filter_map(|e| e.strip_prefix("lib/"))
        .filter_map(|rest| rest.split('/').next())
        .map(|abi| abi.to_string())
        .collect();
    abis.sort();
    abis.dedup();
    abis
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|x| x.to_string()).collect()
    }

    #[test]
    fn extracts_unique_sorted_abis() {
        let e = s(&[
            "lib/arm64-v8a/libfoo.so",
            "lib/armeabi-v7a/libfoo.so",
            "lib/arm64-v8a/libbar.so",
            "classes.dex",
        ]);
        assert_eq!(
            detect_abis(&e),
            vec!["arm64-v8a".to_string(), "armeabi-v7a".to_string()]
        );
    }

    #[test]
    fn empty_when_no_lib() {
        let e = s(&["classes.dex", "AndroidManifest.xml"]);
        assert!(detect_abis(&e).is_empty());
    }
}
