//! Reading handouts inside the app: which handouts were generated, how far
//! the student read each one, and what they highlighted.

use std::collections::BTreeSet;

use chrono::NaiveDate;

use crate::plan::PlanId;

macro_rules! id_type {
    ($name:ident) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub struct $name(pub i64);
    };
}

id_type!(ReadingId);
id_type!(HighlightId);

/// A generated handout kept for reading, with the reading progress.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reading {
    pub id: ReadingId,
    pub plan_id: PlanId,
    pub title: String,
    /// Where its PDF was saved.
    pub path: String,
    pub created_on: NaiveDate,
    /// Pages in the handout; 0 until it was first opened.
    pub page_count: usize,
    /// Page the student was on, 0-based.
    pub last_page: usize,
    /// Pages the student has looked at, 0-based.
    pub pages_seen: BTreeSet<usize>,
}

impl Reading {
    /// Share of the pages seen, `0.0..=1.0`.
    pub fn progress(&self) -> f32 {
        if self.page_count == 0 {
            return 0.0;
        }
        let seen = self
            .pages_seen
            .iter()
            .filter(|p| **p < self.page_count)
            .count();
        seen as f32 / self.page_count as f32
    }
}

/// Highlighter colors.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum HighlightColor {
    #[default]
    Yellow,
    Green,
    Blue,
    Pink,
}

impl HighlightColor {
    pub const ALL: [Self; 4] = [Self::Yellow, Self::Green, Self::Blue, Self::Pink];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Yellow => "yellow",
            Self::Green => "green",
            Self::Blue => "blue",
            Self::Pink => "pink",
        }
    }

    pub fn parse(value: &str) -> Self {
        match value {
            "green" => Self::Green,
            "blue" => Self::Blue,
            "pink" => Self::Pink,
            _ => Self::Yellow,
        }
    }

    /// Translucent RGBA, as drawn over the text.
    pub fn rgba(self) -> [u8; 4] {
        match self {
            Self::Yellow => [0xF5, 0xC5, 0x42, 0x70],
            Self::Green => [0x6C, 0xC2, 0x86, 0x6A],
            Self::Blue => [0x6A, 0xA8, 0xE0, 0x66],
            Self::Pink => [0xEE, 0x8A, 0xB0, 0x6A],
        }
    }
}

/// A highlighted run of text on one page, maybe with a note.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Highlight {
    pub id: HighlightId,
    pub page: usize,
    /// First and last text box of the run on the page (inclusive).
    pub first: usize,
    pub last: usize,
    /// The highlighted words, for the list and for finding them again.
    pub text: String,
    pub color: HighlightColor,
    pub note: String,
}

/// Pages seen, as stored: "0,1,2,5".
pub(crate) fn pages_to_text(pages: &BTreeSet<usize>) -> String {
    pages
        .iter()
        .map(usize::to_string)
        .collect::<Vec<_>>()
        .join(",")
}

pub(crate) fn pages_from_text(text: &str) -> BTreeSet<usize> {
    text.split(',')
        .filter_map(|p| p.trim().parse().ok())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn progress_counts_pages_seen() {
        let mut reading = Reading {
            id: ReadingId(1),
            plan_id: PlanId(1),
            title: String::new(),
            path: String::new(),
            created_on: NaiveDate::default(),
            page_count: 0,
            last_page: 0,
            pages_seen: BTreeSet::new(),
        };
        assert_eq!(reading.progress(), 0.0);
        reading.page_count = 4;
        reading.pages_seen = pages_from_text("0, 1,x,9");
        assert_eq!(pages_to_text(&reading.pages_seen), "0,1,9");
        assert_eq!(reading.progress(), 0.5);
        assert_eq!(HighlightColor::parse("pink"), HighlightColor::Pink);
        assert_eq!(HighlightColor::parse("?"), HighlightColor::Yellow);
    }
}
