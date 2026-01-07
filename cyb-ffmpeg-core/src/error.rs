//! Error types for cyb-ffmpeg-core

use std::path::PathBuf;
use thiserror::Error;

/// Result type alias for cyb-ffmpeg-core operations
pub type Result<T> = std::result::Result<T, Error>;

/// Error types for FFmpeg operations
#[derive(Error, Debug)]
pub enum Error {
    /// File not found
    #[error("File not found: {0}")]
    FileNotFound(PathBuf),

    /// Invalid or unsupported format
    #[error("Invalid format: {0}")]
    InvalidFormat(String),

    /// Codec not supported
    #[error("Codec not supported: {0}")]
    CodecNotSupported(String),

    /// Decode error
    #[error("Decode failed: {0}")]
    DecodeFailed(String),

    /// Seek error
    #[error("Seek failed at {0} microseconds")]
    SeekFailed(i64),

    /// Memory allocation error
    #[error("Memory allocation failed")]
    Memory,

    /// Invalid handle
    #[error("Invalid decoder handle")]
    InvalidHandle,

    /// Decoder not prepared
    #[error("Decoder not prepared")]
    NotPrepared,

    /// FFmpeg error with code
    #[error("FFmpeg error {code}: {message}")]
    FFmpeg { code: i32, message: String },

    /// IO error
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// Lock poisoned
    #[error("Lock poisoned")]
    LockPoisoned,

    /// Channel error
    #[error("Channel error: {0}")]
    Channel(String),

    /// Unknown error
    #[error("Unknown error: {0}")]
    Unknown(String),
}

impl Error {
    /// Convert to FFI error code
    pub fn to_ffi_code(&self) -> i32 {
        match self {
            Error::FileNotFound(_) => 1,
            Error::InvalidFormat(_) => 2,
            Error::CodecNotSupported(_) => 3,
            Error::DecodeFailed(_) => 4,
            Error::SeekFailed(_) => 5,
            Error::Memory => 6,
            Error::InvalidHandle => 7,
            Error::NotPrepared => 8,
            Error::FFmpeg { code, .. } => *code,
            Error::Io(_) => 1,
            Error::LockPoisoned => 6,
            Error::Channel(_) => 99,
            Error::Unknown(_) => 99,
        }
    }

    /// Create from FFmpeg error code
    pub fn from_ffmpeg(code: i32) -> Self {
        let message = match code {
            -2 => "No such file or directory",
            -5 => "Input/output error",
            -12 => "Cannot allocate memory",
            -22 => "Invalid argument",
            -32 => "Broken pipe",
            -38 => "Function not implemented",
            -1094995529 => "Invalid data found",
            -1414092869 => "End of file",
            _ => "Unknown FFmpeg error",
        };

        Error::FFmpeg {
            code,
            message: message.to_string(),
        }
    }
}

impl<T> From<std::sync::PoisonError<T>> for Error {
    fn from(_: std::sync::PoisonError<T>) -> Self {
        Error::LockPoisoned
    }
}

impl From<crossbeam_channel::RecvError> for Error {
    fn from(e: crossbeam_channel::RecvError) -> Self {
        Error::Channel(e.to_string())
    }
}

impl<T> From<crossbeam_channel::SendError<T>> for Error {
    fn from(e: crossbeam_channel::SendError<T>) -> Self {
        Error::Channel(e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_codes() {
        assert_eq!(Error::FileNotFound(PathBuf::new()).to_ffi_code(), 1);
        assert_eq!(Error::Memory.to_ffi_code(), 6);
        assert_eq!(Error::NotPrepared.to_ffi_code(), 8);
    }

    #[test]
    fn test_ffmpeg_error() {
        let err = Error::from_ffmpeg(-2);
        assert!(matches!(err, Error::FFmpeg { code: -2, .. }));
    }
}
