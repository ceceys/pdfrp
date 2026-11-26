//! Marks that 1.0.0 (under the program's former name) and earlier copies wrote into documents under the upstream
//! internal name. Read only, never written: a wrapper pair or an image crop such a copy added is
//! still recognised as the app's own, so it can be removed or replaced like a new one. This is the
//! one place the old names are kept; drop an entry once no document written before 1.0.1 matters.

/// A content wrapper's open (see `place::WRAP_OPEN`).
pub(crate) const WRAP_OPEN: &[u8] = b"q %PdfCraft\n";

/// A content wrapper's close (see `place::WRAP_CLOSE`).
pub(crate) const WRAP_CLOSE: &[u8] = b"\nQ %PdfCraft\n";

/// A content wrapper's close as files before the leading newline have it (see
/// `place::OLD_WRAP_CLOSE`).
pub(crate) const OLD_WRAP_CLOSE: &[u8] = b"Q %PdfCraft\n";

/// The marked-content point in front of an image crop (see `images::IMAGE_CROP`).
pub(crate) const IMAGE_CROP: &[u8] = b"PdfCraftImageCrop";
