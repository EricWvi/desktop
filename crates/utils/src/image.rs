//! Header-only parsing for the raster image formats a host may accept as an asset.
//!
//! Every parser here reads only what the container declares about itself: the magic bytes that
//! identify the format and the fields that state the canvas size. No pixel data is decoded, so
//! bounding an image by its dimensions never costs more than a handful of byte reads and the
//! crate pulls in no image-decoding dependency.

mod jpeg;
mod png;
mod webp;

use thiserror::Error;

/// One raster image format whose dimensions can be read straight from its header.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImageFormat {
    Png,
    Webp,
    Jpeg,
}

impl ImageFormat {
    /// Returns the lowercase spelling used in diagnostics.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Png => "png",
            Self::Webp => "webp",
            Self::Jpeg => "jpeg",
        }
    }
}

/// The canvas size an image header declares, in pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ImageDimensions {
    pub width: u32,
    pub height: u32,
}

/// One image's format and declared canvas size, as stated by its own header.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ImageHeader {
    pub format: ImageFormat,
    pub dimensions: ImageDimensions,
}

/// Reports why a byte slice could not be read as a bounded still image.
///
/// The variants are deliberately coarse: a caller that treats an unreadable image as an absent
/// one only needs to know that the bytes are unusable, while diagnostics still name which format
/// was recognised before the header stopped making sense.
#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum ImageHeaderError {
    #[error("bytes do not begin with the magic bytes of a supported image format")]
    UnrecognizedFormat,
    #[error("{format} header ends before the declared canvas size")]
    Truncated { format: ImageFormat },
    #[error("{format} header is malformed: {detail}")]
    Malformed {
        format: ImageFormat,
        detail: &'static str,
    },
    #[error("{format} image is animated")]
    Animated { format: ImageFormat },
}

impl std::fmt::Display for ImageFormat {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// Identifies the format of `bytes` from its magic bytes alone, ignoring any filename.
///
/// Returning `None` is the whitelist in action: a format the host cannot bound without decoding
/// — an animated GIF among them — is indistinguishable here from arbitrary bytes, which is
/// exactly how callers are expected to treat it.
pub fn detect_format(bytes: &[u8]) -> Option<ImageFormat> {
    if png::has_magic(bytes) {
        Some(ImageFormat::Png)
    } else if webp::has_magic(bytes) {
        Some(ImageFormat::Webp)
    } else if jpeg::has_magic(bytes) {
        Some(ImageFormat::Jpeg)
    } else {
        None
    }
}

/// Reads the format and declared canvas size of a still raster image.
///
/// The format is decided by [`detect_format`] before any field is interpreted, so a file whose
/// name disagrees with its contents is reported against what the bytes actually are. Animated
/// containers are rejected rather than measured: an icon that plays on its own is a different
/// kind of asset than the one callers asked to bound.
pub fn read_header(bytes: &[u8]) -> Result<ImageHeader, ImageHeaderError> {
    let format = detect_format(bytes).ok_or(ImageHeaderError::UnrecognizedFormat)?;
    let dimensions = match format {
        ImageFormat::Png => png::read_dimensions(bytes),
        ImageFormat::Webp => webp::read_dimensions(bytes),
        ImageFormat::Jpeg => jpeg::read_dimensions(bytes),
    }?;
    Ok(ImageHeader { format, dimensions })
}

/// Reads a big-endian `u32` at `offset`, or reports the header as truncated.
fn be_u32(bytes: &[u8], offset: usize, format: ImageFormat) -> Result<u32, ImageHeaderError> {
    let field = bytes
        .get(offset..offset + 4)
        .ok_or(ImageHeaderError::Truncated { format })?;
    Ok(u32::from_be_bytes([field[0], field[1], field[2], field[3]]))
}

/// Reads a big-endian `u16` at `offset`, or reports the header as truncated.
fn be_u16(bytes: &[u8], offset: usize, format: ImageFormat) -> Result<u16, ImageHeaderError> {
    let field = bytes
        .get(offset..offset + 2)
        .ok_or(ImageHeaderError::Truncated { format })?;
    Ok(u16::from_be_bytes([field[0], field[1]]))
}
