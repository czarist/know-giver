//! The Know Giver mark as a cover plate, like the animal engraving on a
//! programming book cover: Thoth's ibis head, god of writing and knowledge.

use egui::{Image, ImageSource, include_image, vec2};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Art {
    /// The ibis under the lunar disc, square, ink on dark.
    Mark,
}

impl Art {
    fn source(self) -> ImageSource<'static> {
        match self {
            Self::Mark => include_image!("../assets/logo.svg"),
        }
    }

    /// Width over height of the artwork.
    pub fn aspect(self) -> f32 {
        match self {
            Self::Mark => 1.0,
        }
    }

    /// The artwork at `height` points.
    pub fn image(self, height: f32) -> Image<'static> {
        Image::new(self.source()).fit_to_exact_size(vec2(height * self.aspect(), height))
    }
}
