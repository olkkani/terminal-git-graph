pub mod kitty;

use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GfxError {
    RgbaLengthMismatch { expected: usize, actual: usize },
}

impl fmt::Display for GfxError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            GfxError::RgbaLengthMismatch { expected, actual } => {
                write!(f, "RGBA length mismatch: expected {expected} bytes, got {actual}")
            }
        }
    }
}

impl std::error::Error for GfxError {}
