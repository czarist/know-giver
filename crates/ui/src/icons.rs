//! Icons (Lucide, ISC license, white strokes so they can be tinted) and the
//! logo, rendered from SVG by egui_extras.

use egui::{Color32, Image, ImageSource, include_image, vec2};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Icon {
    Edital,
    Plan,
    Board,
    Lesson,
    Quiz,
    Review,
    Progress,
    Settings,
    Today,
    Handout,
    Streak,
    Sparkles,
    Check,
    Play,
    Stop,
    Close,
    Download,
    Mic,
    Writing,
    GitHub,
    Update,
}

impl Icon {
    fn source(self) -> ImageSource<'static> {
        match self {
            Self::Edital => include_image!("../assets/icons/file-text.svg"),
            Self::Plan => include_image!("../assets/icons/map.svg"),
            Self::Board => include_image!("../assets/icons/landmark.svg"),
            Self::Lesson => include_image!("../assets/icons/mic.svg"),
            Self::Quiz => include_image!("../assets/icons/list-checks.svg"),
            Self::Review => include_image!("../assets/icons/repeat.svg"),
            Self::Progress => include_image!("../assets/icons/chart-column.svg"),
            Self::Settings => include_image!("../assets/icons/settings.svg"),
            Self::Today => include_image!("../assets/icons/sun.svg"),
            Self::Handout => include_image!("../assets/icons/book-open-text.svg"),
            Self::Streak => include_image!("../assets/icons/flame.svg"),
            Self::Sparkles => include_image!("../assets/icons/sparkles.svg"),
            Self::Check => include_image!("../assets/icons/circle-check.svg"),
            Self::Play => include_image!("../assets/icons/play.svg"),
            Self::Stop => include_image!("../assets/icons/square.svg"),
            Self::Close => include_image!("../assets/icons/x.svg"),
            Self::Download => include_image!("../assets/icons/file-down.svg"),
            Self::Mic => include_image!("../assets/icons/mic.svg"),
            Self::Writing => include_image!("../assets/icons/pen-line.svg"),
            Self::GitHub => include_image!("../assets/icons/github.svg"),
            Self::Update => include_image!("../assets/icons/download.svg"),
        }
    }

    /// The icon at `size` points, in `color`.
    pub fn image(self, size: f32, color: Color32) -> Image<'static> {
        Image::new(self.source())
            .fit_to_exact_size(vec2(size, size))
            .tint(color)
    }
}

/// The app icon (Thoth as a baboon, engraved, on a jade tile) at `size` points.
pub fn logo(size: f32) -> Image<'static> {
    Image::new(include_image!("../assets/icon.png")).fit_to_exact_size(vec2(size, size))
}
