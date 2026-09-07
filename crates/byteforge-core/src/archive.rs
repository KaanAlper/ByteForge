use crate::{CoreError, Result};
use std::io::Read;
use std::path::{Path, PathBuf};

/// Tek bir arşiv girişinin okunabileceği azami boyut (zip bomb savunması).
const MAX_ENTRY_BYTES: u64 = 128 * 1024 * 1024;

/// Arşivdeki, adı `ext` (ör. ".apk") ile biten girişleri `out_dir`'e çıkarır.
/// Yalnızca dosya adı kullanılır (dizin traversal önlenir). Çıkarılan yolları döndürür.
pub fn extract_entries_with_ext(path: &Path, out_dir: &Path, ext: &str) -> Result<Vec<PathBuf>> {
    let file = std::fs::File::open(path)?;
    let mut archive = zip::ZipArchive::new(file)?;
    std::fs::create_dir_all(out_dir)?;
    let ext_lower = ext.to_lowercase();
    let mut extracted = Vec::new();
    for i in 0..archive.len() {
        let entry = archive.by_index(i)?;
        let name = entry.name().to_string();
        if !name.to_lowercase().ends_with(&ext_lower) {
            continue;
        }
        if entry.size() > MAX_ENTRY_BYTES {
            continue;
        }
        let Some(base) = Path::new(&name).file_name() else {
            continue;
        };
        let out = out_dir.join(base);
        let mut buf = Vec::new();
        entry.take(MAX_ENTRY_BYTES + 1).read_to_end(&mut buf)?;
        if buf.len() as u64 > MAX_ENTRY_BYTES {
            continue;
        }
        std::fs::write(&out, &buf)?;
        extracted.push(out);
    }
    Ok(extracted)
}

/// Bir APK/ZIP arşivindeki tüm giriş adlarını (dosya yollarını) döndürür.
/// Arşivi tam olarak açmadan (bellek üzerine çıkarmadan) yalnızca dizin okur.
pub fn list_entries(path: &Path) -> Result<Vec<String>> {
    let file = std::fs::File::open(path)?;
    let mut archive = zip::ZipArchive::new(file)?;
    let mut names = Vec::with_capacity(archive.len());
    for i in 0..archive.len() {
        let entry = archive.by_index(i)?;
        names.push(entry.name().to_string());
    }
    Ok(names)
}

/// Arşivdeki tek bir girişin ham baytlarını okur (ör. AndroidManifest.xml).
pub fn read_entry(path: &Path, name: &str) -> Result<Vec<u8>> {
    let file = std::fs::File::open(path)?;
    let mut archive = zip::ZipArchive::new(file)?;
    let entry = archive.by_name(name)?;
    // Merkezî dizindeki beyan edilen boyut (yalan olabilir) ile erken reddet.
    if entry.size() > MAX_ENTRY_BYTES {
        return Err(CoreError::EntryTooLarge(name.to_string()));
    }
    // Gerçek okumayı da sınırla: sıkıştırma bombası beyanı atlatabilir.
    let mut buf = Vec::new();
    entry.take(MAX_ENTRY_BYTES + 1).read_to_end(&mut buf)?;
    if buf.len() as u64 > MAX_ENTRY_BYTES {
        return Err(CoreError::EntryTooLarge(name.to_string()));
    }
    Ok(buf)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    /// Test yardımcı: verilen giriş adlarıyla geçici bir zip dosyası kurar.
    fn make_zip(entries: &[&str]) -> tempfile::NamedTempFile {
        let file = tempfile::NamedTempFile::new().unwrap();
        let mut zip = zip::ZipWriter::new(file.reopen().unwrap());
        let opts: zip::write::FileOptions<()> = zip::write::FileOptions::default();
        for name in entries {
            zip.start_file(*name, opts).unwrap();
            zip.write_all(b"x").unwrap();
        }
        zip.finish().unwrap();
        file
    }

    #[test]
    fn lists_all_entry_names() {
        let zip = make_zip(&[
            "classes.dex",
            "AndroidManifest.xml",
            "lib/arm64-v8a/libfoo.so",
        ]);
        let mut got = list_entries(zip.path()).unwrap();
        got.sort();
        assert_eq!(
            got,
            vec![
                "AndroidManifest.xml".to_string(),
                "classes.dex".to_string(),
                "lib/arm64-v8a/libfoo.so".to_string(),
            ]
        );
    }

    #[test]
    fn errors_on_non_zip() {
        let file = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(file.path(), b"not a zip").unwrap();
        assert!(list_entries(file.path()).is_err());
    }

    #[test]
    fn reads_named_entry_bytes() {
        let zip = make_zip(&["AndroidManifest.xml", "classes.dex"]);
        assert_eq!(read_entry(zip.path(), "AndroidManifest.xml").unwrap(), b"x");
        assert!(read_entry(zip.path(), "does-not-exist").is_err());
    }

    #[test]
    fn extracts_only_matching_ext() {
        let zip = make_zip(&[
            "base.apk",
            "config.arm64_v8a.apk",
            "icon.png",
            "manifest.json",
        ]);
        let dir = tempfile::tempdir().unwrap();
        let apks = extract_entries_with_ext(zip.path(), dir.path(), ".apk").unwrap();
        let mut names: Vec<String> = apks
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        names.sort();
        assert_eq!(names, vec!["base.apk", "config.arm64_v8a.apk"]);
    }
}
