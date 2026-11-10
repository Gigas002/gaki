//! Game art: the decoded image type, and (with the `art` feature) its loader.

#[cfg(feature = "art")]
mod load;
#[cfg(feature = "art")]
pub use load::{ImageKind, ImageSlot, poll, select};

/// A decoded RGBA image.
#[cfg(feature = "art")]
pub type ImgBuf = image::ImageBuffer<image::Rgba<u8>, Vec<u8>>;

/// Without the `art` feature no image ever loads: this type has no values,
/// so every `Option<&ImgBuf>` is `None`.
#[cfg(not(feature = "art"))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImgBuf {}
