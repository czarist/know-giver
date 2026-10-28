//! The black-and-white engraving of Thoth, god of writing and knowledge,
//! used like the animal engraving on a programming book cover.

use egui::{Image, ImageSource, include_image, vec2};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Art {
    /// Thoth seated, with his staff (portrait).
    Seated,
    /// Head and collar, square.
    Head,
}

impl Art {
    fn source(self) -> ImageSource<'static> {
        match self {
            Self::Seated => include_image!("../assets/art/thoth.png"),
            Self::Head => include_image!("../assets/art/thoth-head.png"),
        }
    }

    /// Width over height of the image file.
    pub fn aspect(self) -> f32 {
        match self {
            Self::Seated => 440.0 / 546.0,
            Self::Head => 1.0,
        }
    }

    /// The engraving at `height` points.
    pub fn image(self, height: f32) -> Image<'static> {
        Image::new(self.source()).fit_to_exact_size(vec2(height * self.aspect(), height))
    }
}
