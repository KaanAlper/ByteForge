use serde::Serialize;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

/// APK Signing Block'un sonundaki 16 baytlık sihirli dize.
const APK_SIG_BLOCK_MAGIC: &[u8; 16] = b"APK Sig Block 42";
/// APK Signature Scheme v2 blok ID'si.
const V2_ID: u32 = 0x7109871a;
/// APK Signature Scheme v3 blok ID'si.
const V3_ID: u32 = 0xf05368c0;
/// ZIP End of Central Directory kaydının sihirli sayısı.
const EOCD_MAGIC: u32 = 0x0605_4b50;
/// EOCD kaydı: 22 sabit bayt + en fazla 65535 baytlık yorum.
const EOCD_MAX_TAIL: u64 = 22 + 0xFFFF;
/// Signing block için makul üst sınır (bozuk/kötü niyetli boyut alanına karşı).
const MAX_SIG_BLOCK: u64 = 64 * 1024 * 1024;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct SignatureSchemes {
    pub v1: bool,
    pub v2: bool,
    pub v3: bool,
}

/// v1'i giriş adlarından tespit eder (META-INF sertifika dosyaları).
///
/// Yalnızca `META-INF/` altındaki adaylar için ayrıştırma yapar; tüm giriş
/// adları için gereksiz `to_uppercase()` allocation'ından kaçınır.
pub fn detect_v1(entries: &[String]) -> bool {
    entries.iter().any(|e| {
        // Önce ucuz, case-insensitive prefix testi.
        e.get(..9)
            .map(|p| p.eq_ignore_ascii_case("META-INF/"))
            .unwrap_or(false)
            && matches!(
                Path::new(e).extension().and_then(|x| x.to_str()),
                Some(ext) if ext.eq_ignore_ascii_case("RSA")
                    || ext.eq_ignore_ascii_case("DSA")
                    || ext.eq_ignore_ascii_case("EC")
            )
    })
}

/// v2/v3'ü, ZIP yapısını izleyerek yalnızca doğrulanmış APK Signing Block
/// içinde arar (ham baytlarda kör tarama yapmaz — yanlış-pozitif üretmez).
///
/// Yalnızca dosyanın kuyruğunu (EOCD + signing block bölgesi) okur; tüm
/// dosyayı belleğe almaz, böylece büyük APK'larda ucuz ve DoS'a dirençlidir.
pub fn detect_v2_v3(path: &Path) -> crate::Result<(bool, bool)> {
    let mut file = File::open(path)?;
    let file_len = file.metadata()?.len();

    let cd_offset = match find_cd_offset(&mut file, file_len)? {
        Some(off) if off <= file_len => off,
        _ => return Ok((false, false)),
    };
    scan_signing_block(&mut file, cd_offset)
}

/// Belirtilen ofsetten `len` bayt okur.
fn read_at(file: &mut File, offset: u64, len: usize) -> std::io::Result<Vec<u8>> {
    file.seek(SeekFrom::Start(offset))?;
    let mut buf = vec![0u8; len];
    file.read_exact(&mut buf)?;
    Ok(buf)
}

/// EOCD kaydını dosya kuyruğundan bulup Central Directory ofsetini döndürür.
fn find_cd_offset(file: &mut File, file_len: u64) -> crate::Result<Option<u64>> {
    if file_len < 22 {
        return Ok(None);
    }
    let read_len = std::cmp::min(file_len, EOCD_MAX_TAIL);
    let start = file_len - read_len;
    let buf = read_at(file, start, read_len as usize)?;
    if buf.len() < 22 {
        return Ok(None);
    }
    // EOCD sihirli sayısını sondan geriye ara (yorum baytları içinde sahte
    // eşleşme olabileceği için ilk geçerli konumu değil, sondan bulunanı alırız).
    let magic = EOCD_MAGIC.to_le_bytes();
    let mut i = buf.len() - 22;
    loop {
        if buf[i..i + 4] == magic {
            let cd = u32::from_le_bytes(buf[i + 16..i + 20].try_into().unwrap());
            return Ok(Some(cd as u64));
        }
        if i == 0 {
            return Ok(None);
        }
        i -= 1;
    }
}

/// Central Directory'den hemen önceki APK Signing Block'u doğrular ve
/// içindeki id-value çiftlerinde v2/v3 şema ID'lerini arar.
fn scan_signing_block(file: &mut File, cd_offset: u64) -> crate::Result<(bool, bool)> {
    // Blok en az: size1(8) + size2(8) + magic(16) = 32 bayt.
    if cd_offset < 32 {
        return Ok((false, false));
    }
    // Magic, Central Directory'den hemen öncedir.
    let magic = read_at(file, cd_offset - 16, 16)?;
    if magic.as_slice() != APK_SIG_BLOCK_MAGIC.as_slice() {
        return Ok((false, false));
    }
    // Sondaki boyut alanı (magic'ten hemen önce).
    let size2 = u64::from_le_bytes(read_at(file, cd_offset - 24, 8)?.try_into().unwrap());
    if !(24..=MAX_SIG_BLOCK).contains(&size2) {
        return Ok((false, false));
    }
    // Baştaki boyut alanının konumu ve değeri; ikisi eşleşmeli.
    let block_start = match cd_offset.checked_sub(8 + size2) {
        Some(s) => s,
        None => return Ok((false, false)),
    };
    let size1 = u64::from_le_bytes(read_at(file, block_start, 8)?.try_into().unwrap());
    if size1 != size2 {
        return Ok((false, false));
    }
    // id-value çiftleri bölgesi: size1 = pairs + size2(8) + magic(16).
    let pairs_len = size1 - 24;
    let pairs = read_at(file, block_start + 8, pairs_len as usize)?;

    let mut v2 = false;
    let mut v3 = false;
    let mut pos = 0usize;
    while pos + 12 <= pairs.len() {
        let len = u64::from_le_bytes(pairs[pos..pos + 8].try_into().unwrap());
        if len < 4 {
            break;
        }
        let id = u32::from_le_bytes(pairs[pos + 8..pos + 12].try_into().unwrap());
        if id == V2_ID {
            v2 = true;
        }
        if id == V3_ID {
            v3 = true;
        }
        match 8usize.checked_add(len as usize) {
            Some(step) => pos += step,
            None => break,
        }
    }
    Ok((v2, v3))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Cursor, Write};

    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|x| x.to_string()).collect()
    }

    /// Verilen ID'lerle minimal ama yapısal olarak geçerli bir signing block kurar.
    fn build_signing_block(ids: &[u32]) -> Vec<u8> {
        let mut pairs = Vec::new();
        for &id in ids {
            let value = [0u8; 4];
            let len = (4 + value.len()) as u64; // id(4) + value
            pairs.extend_from_slice(&len.to_le_bytes());
            pairs.extend_from_slice(&id.to_le_bytes());
            pairs.extend_from_slice(&value);
        }
        let size = (pairs.len() + 8 + 16) as u64; // pairs + size2 + magic
        let mut block = Vec::new();
        block.extend_from_slice(&size.to_le_bytes()); // size1
        block.extend_from_slice(&pairs);
        block.extend_from_slice(&size.to_le_bytes()); // size2
        block.extend_from_slice(APK_SIG_BLOCK_MAGIC);
        block
    }

    /// Stored (sıkıştırmasız) bir zip üretir; içerik baytları ham korunur.
    fn make_zip_stored(entries: &[(&str, Vec<u8>)]) -> Vec<u8> {
        let mut buf = Vec::new();
        {
            let mut zip = zip::ZipWriter::new(Cursor::new(&mut buf));
            let opts: zip::write::FileOptions<()> = zip::write::FileOptions::default()
                .compression_method(zip::CompressionMethod::Stored);
            for (name, content) in entries {
                zip.start_file(*name, opts).unwrap();
                zip.write_all(content).unwrap();
            }
            zip.finish().unwrap();
        }
        buf
    }

    /// Bir zip'e, Central Directory'den önce gerçek bir signing block enjekte eder
    /// ve EOCD'deki CD ofsetini düzeltir — apksigner'ın ürettiği yapının taklidi.
    fn write_apk_with_block(entries: &[(&str, Vec<u8>)], ids: &[u32]) -> tempfile::NamedTempFile {
        let zip = make_zip_stored(entries);
        let eocd = zip.len() - 22; // yorumsuz EOCD
        let cd_offset = u32::from_le_bytes(zip[eocd + 16..eocd + 20].try_into().unwrap()) as usize;
        let block = build_signing_block(ids);

        let mut out = Vec::with_capacity(zip.len() + block.len());
        out.extend_from_slice(&zip[..cd_offset]);
        out.extend_from_slice(&block);
        out.extend_from_slice(&zip[cd_offset..]);

        let new_eocd = out.len() - 22;
        let new_cd = (cd_offset + block.len()) as u32;
        out[new_eocd + 16..new_eocd + 20].copy_from_slice(&new_cd.to_le_bytes());

        let f = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(f.path(), &out).unwrap();
        f
    }

    #[test]
    fn detects_v1_from_meta_inf() {
        assert!(detect_v1(&s(&["META-INF/CERT.RSA", "classes.dex"])));
        assert!(detect_v1(&s(&["META-INF/CERT.EC"])));
        assert!(detect_v1(&s(&["meta-inf/cert.rsa"]))); // case-insensitive
        assert!(!detect_v1(&s(&["META-INF/MANIFEST.MF", "classes.dex"])));
    }

    #[test]
    fn detects_real_v2_and_v3_block() {
        let f = write_apk_with_block(&[("classes.dex", vec![1, 2, 3])], &[V2_ID, V3_ID]);
        assert_eq!(detect_v2_v3(f.path()).unwrap(), (true, true));
    }

    #[test]
    fn detects_only_v2_when_only_v2_present() {
        let f = write_apk_with_block(&[("classes.dex", vec![1, 2, 3])], &[V2_ID]);
        assert_eq!(detect_v2_v3(f.path()).unwrap(), (true, false));
    }

    #[test]
    fn no_false_positive_when_ids_appear_in_content() {
        // Signing block YOK; ama bir dosya içeriği v2/v3 ID baytlarını içeriyor.
        // Kör tarama burada (true, true) verirdi — yapısal parse (false, false) vermeli.
        let mut content = vec![0u8; 128];
        content.extend_from_slice(&V2_ID.to_le_bytes());
        content.extend_from_slice(&V3_ID.to_le_bytes());
        let zip = make_zip_stored(&[("classes.dex", content)]);
        let f = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(f.path(), &zip).unwrap();
        assert_eq!(detect_v2_v3(f.path()).unwrap(), (false, false));
    }

    #[test]
    fn no_v2_v3_for_plain_zip() {
        let zip = make_zip_stored(&[("classes.dex", vec![9, 9, 9])]);
        let f = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(f.path(), &zip).unwrap();
        assert_eq!(detect_v2_v3(f.path()).unwrap(), (false, false));
    }
}
