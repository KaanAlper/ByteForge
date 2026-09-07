use thiserror::Error;

#[derive(Debug, Error)]
pub enum CoreError {
    #[error("dosya açılamadı: {0}")]
    Io(#[from] std::io::Error),
    #[error("geçersiz arşiv (ZIP değil ya da bozuk): {0}")]
    Archive(#[from] zip::result::ZipError),
    #[error("AndroidManifest ayrıştırılamadı: {0}")]
    Manifest(#[from] axmldecoder::ParseError),
    #[error("arşiv girişi güvenlik sınırından büyük: {0}")]
    EntryTooLarge(String),
    #[error("ELF/.so ayrıştırılamadı: {0}")]
    Elf(#[from] goblin::error::Error),
    #[error("yama dosya sınırının dışında (ofset {offset}, uzunluk {len}, dosya {file_len})")]
    PatchOutOfBounds {
        offset: usize,
        len: usize,
        file_len: usize,
    },
    #[error("Smali yaması uygulanamadı: {0}")]
    SmaliPatch(String),
    #[error("hex ayrıştırma hatası: {0}")]
    Hex(String),
    #[error("desteklenmeyen ya da tanınmayan paket biçimi")]
    UnknownFormat,
}

pub type Result<T> = std::result::Result<T, CoreError>;
