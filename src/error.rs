use thiserror::Error;

#[derive(Error, Debug)]
pub enum VstorageError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Encryption error: {0}")]
    Crypto(String),

    #[error("Reed-Solomon error: {0}")]
    Ecc(String),

    #[error("Invalid header: {0}")]
    Header(String),

    #[error("FFmpeg error: {0}")]
    Ffmpeg(String),

    #[error("Invalid configuration: {0}")]
    Config(String),

    #[error("Image error: {0}")]
    Image(#[from] image::ImageError),
}

pub type Result<T> = std::result::Result<T, VstorageError>;
