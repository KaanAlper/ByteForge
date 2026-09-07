use crate::Result;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::path::Path;

/// İki APK arşivinin dosya düzeyi farkı.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct ArchiveDiff {
    /// A paketinin tam dosya SHA-256'sı (bütünlük parmak izi).
    pub sha256_a: String,
    pub sha256_b: String,
    /// Yalnızca B'de bulunan girişler.
    pub added: Vec<String>,
    /// Yalnızca A'da bulunan girişler.
    pub removed: Vec<String>,
    /// İki pakette de olup içeriği (CRC32) değişen girişler.
    pub modified: Vec<String>,
    /// İçeriği aynı kalan giriş sayısı.
    pub unchanged: usize,
}

/// İki bayt dizisinin farkı (ör. değişen bir `.so`).
#[derive(Debug, Clone, Serialize, Default, PartialEq, Eq)]
pub struct ByteDiff {
    pub len_a: usize,
    pub len_b: usize,
    /// İlk farklı baytın ofseti (fark yoksa None).
    pub first_diff_offset: Option<usize>,
    /// Ortak uzunlukta farklı bayt sayısı + uzunluk farkı.
    pub differing_bytes: usize,
}

/// Bir dosyanın tam SHA-256'sını onaltılık dize olarak döndürür.
pub fn sha256_file(path: &Path) -> Result<String> {
    let bytes = std::fs::read(path)?;
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    let digest = hasher.finalize();
    let mut hex = String::with_capacity(64);
    for b in digest {
        hex.push_str(&format!("{b:02x}"));
    }
    Ok(hex)
}

/// Arşivdeki her girişin adını CRC32'sine eşler (merkezî dizinden; içerik okumaz).
fn entry_crcs(path: &Path) -> Result<BTreeMap<String, u32>> {
    let file = std::fs::File::open(path)?;
    let mut archive = zip::ZipArchive::new(file)?;
    let mut map = BTreeMap::new();
    for i in 0..archive.len() {
        let entry = archive.by_index(i)?;
        map.insert(entry.name().to_string(), entry.crc32());
    }
    Ok(map)
}

/// İki APK'yı dosya düzeyinde karşılaştırır.
pub fn diff_archives(path_a: &Path, path_b: &Path) -> Result<ArchiveDiff> {
    let a = entry_crcs(path_a)?;
    let b = entry_crcs(path_b)?;

    let mut diff = ArchiveDiff {
        sha256_a: sha256_file(path_a)?,
        sha256_b: sha256_file(path_b)?,
        ..Default::default()
    };

    for (name, crc_a) in &a {
        match b.get(name) {
            None => diff.removed.push(name.clone()),
            Some(crc_b) if crc_b == crc_a => diff.unchanged += 1,
            Some(_) => diff.modified.push(name.clone()),
        }
    }
    for name in b.keys() {
        if !a.contains_key(name) {
            diff.added.push(name.clone());
        }
    }
    Ok(diff)
}

/// İki dosya arasındaki tek bir değişen bölge (yerinde bayt yaması).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DiffRegion {
    pub offset: usize,
    /// Orijinal (temiz) baytlar.
    pub old: Vec<u8>,
    /// Yeni (modlu) baytlar.
    pub new: Vec<u8>,
}

/// İki eşit-uzunlukta bayt dizisi arasındaki bitişik değişen bölgeleri bulur
/// (yerinde yama tespiti). `merge_gap` kadar yakın bölgeler birleştirilir
/// (bir komut sınırında bölünmüş yamayı tek parça yapmak için). En fazla `max`.
pub fn byte_diff_regions(a: &[u8], b: &[u8], merge_gap: usize, max: usize) -> Vec<DiffRegion> {
    let n = a.len().min(b.len());
    let mut raw: Vec<(usize, usize)> = Vec::new(); // (start, end)
    let mut i = 0;
    while i < n {
        if a[i] != b[i] {
            let start = i;
            while i < n && a[i] != b[i] {
                i += 1;
            }
            raw.push((start, i));
        } else {
            i += 1;
        }
    }
    // Bitişikleri (gap <= merge_gap) birleştir.
    let mut merged: Vec<(usize, usize)> = Vec::new();
    for (s, e) in raw {
        if let Some(last) = merged.last_mut() {
            if s <= last.1 + merge_gap {
                last.1 = e;
                continue;
            }
        }
        merged.push((s, e));
    }
    merged
        .into_iter()
        .take(max)
        .map(|(s, e)| DiffRegion {
            offset: s,
            old: a[s..e].to_vec(),
            new: b[s..e].to_vec(),
        })
        .collect()
}

/// İki bayt dizisini karşılaştırır.
pub fn byte_diff(a: &[u8], b: &[u8]) -> ByteDiff {
    let min = a.len().min(b.len());
    let mut first_diff_offset = None;
    let mut differing = 0usize;
    for i in 0..min {
        if a[i] != b[i] {
            if first_diff_offset.is_none() {
                first_diff_offset = Some(i);
            }
            differing += 1;
        }
    }
    let tail = a.len().abs_diff(b.len());
    if first_diff_offset.is_none() && tail > 0 {
        first_diff_offset = Some(min);
    }
    ByteDiff {
        len_a: a.len(),
        len_b: b.len(),
        first_diff_offset,
        differing_bytes: differing + tail,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn make_zip(entries: &[(&str, &[u8])]) -> tempfile::NamedTempFile {
        let file = tempfile::NamedTempFile::new().unwrap();
        let mut zip = zip::ZipWriter::new(file.reopen().unwrap());
        let opts: zip::write::FileOptions<()> =
            zip::write::FileOptions::default().compression_method(zip::CompressionMethod::Stored);
        for (name, content) in entries {
            zip.start_file(*name, opts).unwrap();
            zip.write_all(content).unwrap();
        }
        zip.finish().unwrap();
        file
    }

    #[test]
    fn sha256_of_known_content() {
        let f = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(f.path(), b"abc").unwrap();
        assert_eq!(
            sha256_file(f.path()).unwrap(),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn byte_diff_identical() {
        let d = byte_diff(b"hello", b"hello");
        assert_eq!(d.first_diff_offset, None);
        assert_eq!(d.differing_bytes, 0);
    }

    #[test]
    fn byte_diff_middle_change() {
        let d = byte_diff(b"hello", b"heLlo");
        assert_eq!(d.first_diff_offset, Some(2));
        assert_eq!(d.differing_bytes, 1);
    }

    #[test]
    fn byte_diff_length_change() {
        let d = byte_diff(b"hello", b"hello world");
        assert_eq!(d.len_a, 5);
        assert_eq!(d.len_b, 11);
        assert_eq!(d.first_diff_offset, Some(5));
        assert_eq!(d.differing_bytes, 6);
    }

    #[test]
    fn byte_diff_regions_finds_and_merges() {
        //          0  1  2  3  4  5  6  7  8  9
        let a = [0, 0, 9, 9, 0, 0, 0, 9, 0, 0];
        let b = [0, 0, 1, 1, 0, 0, 0, 1, 0, 0];
        // merge_gap=0 → iki ayrı bölge (2..4 ve 7..8)
        let r0 = byte_diff_regions(&a, &b, 0, 10);
        assert_eq!(r0.len(), 2);
        assert_eq!(r0[0].offset, 2);
        assert_eq!(r0[0].new, vec![1, 1]);
        assert_eq!(r0[1].offset, 7);
        // merge_gap=3 → 4 baytlık boşluk birleştirir (2..8 tek bölge)
        let r1 = byte_diff_regions(&a, &b, 3, 10);
        assert_eq!(r1.len(), 1);
        assert_eq!(r1[0].offset, 2);
        assert_eq!(r1[0].old.len(), 6);
    }

    #[test]
    fn byte_diff_regions_respects_max() {
        let a = [0u8, 9, 0, 9, 0, 9];
        let b = [0u8, 1, 0, 1, 0, 1];
        let r = byte_diff_regions(&a, &b, 0, 2);
        assert_eq!(r.len(), 2); // 3 bölge var ama max=2
    }

    #[test]
    fn archive_diff_classifies_entries() {
        let a = make_zip(&[
            ("same.txt", b"identical"),
            ("changed.dex", b"version-one"),
            ("only_a.so", b"gone"),
        ]);
        let b = make_zip(&[
            ("same.txt", b"identical"),
            ("changed.dex", b"version-two"),
            ("only_b.so", b"new"),
        ]);
        let d = diff_archives(a.path(), b.path()).unwrap();
        assert_eq!(d.added, vec!["only_b.so".to_string()]);
        assert_eq!(d.removed, vec!["only_a.so".to_string()]);
        assert_eq!(d.modified, vec!["changed.dex".to_string()]);
        assert_eq!(d.unchanged, 1);
        assert_ne!(d.sha256_a, d.sha256_b);
        assert_eq!(d.sha256_a.len(), 64);
    }

    #[test]
    fn identical_archives_have_no_changes() {
        let a = make_zip(&[("x", b"1"), ("y", b"2")]);
        let b = make_zip(&[("x", b"1"), ("y", b"2")]);
        let d = diff_archives(a.path(), b.path()).unwrap();
        assert!(d.added.is_empty() && d.removed.is_empty() && d.modified.is_empty());
        assert_eq!(d.unchanged, 2);
    }
}
