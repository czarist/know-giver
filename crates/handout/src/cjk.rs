//! Chinese and Japanese fonts. They are large (10–20 MB each), so instead of
//! embedding them the app uses the ones every desktop system ships: Yu
//! Gothic and Microsoft YaHei / JhengHei on Windows, Hiragino and PingFang on
//! macOS, Noto Sans CJK on Linux. Each is read once and kept for the session.

use std::path::PathBuf;
use std::sync::OnceLock;

/// Han characters are drawn differently in Japan, mainland China and
/// Taiwan/Hong Kong, so each script has its own font.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CjkScript {
    Japanese,
    Simplified,
    Traditional,
}

impl CjkScript {
    pub const ALL: [Self; 3] = [Self::Japanese, Self::Simplified, Self::Traditional];

    /// The script of a UI language, if it is one of these.
    pub fn of_locale(locale: &str) -> Option<Self> {
        match locale {
            "ja" => Some(Self::Japanese),
            "zh-Hans" => Some(Self::Simplified),
            "zh-Hant" => Some(Self::Traditional),
            _ => None,
        }
    }

    /// Font files to try, in order, with the face index inside a collection.
    fn candidates(self) -> Vec<(PathBuf, u32)> {
        let windows = std::env::var_os("WINDIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(r"C:\Windows"))
            .join("Fonts");
        let win = |name: &str| (windows.join(name), 0);
        let path = |p: &str, index: u32| (PathBuf::from(p), index);
        // Noto Sans CJK collections hold JP, KR, SC, TC and HK, in that order.
        let noto = |index: u32| {
            [
                "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
                "/usr/share/fonts/noto-cjk/NotoSansCJK-Regular.ttc",
                "/usr/share/fonts/google-noto-cjk/NotoSansCJK-Regular.ttc",
            ]
            .into_iter()
            .map(move |p| path(p, index))
        };
        match self {
            Self::Japanese => [
                win("YuGothM.ttc"),
                win("meiryo.ttc"),
                win("msgothic.ttc"),
                path("/System/Library/Fonts/ヒラギノ角ゴシック W3.ttc", 0),
            ]
            .into_iter()
            .chain(noto(0))
            .collect(),
            Self::Simplified => [
                win("msyh.ttc"),
                win("simsun.ttc"),
                path("/System/Library/Fonts/PingFang.ttc", 0),
            ]
            .into_iter()
            .chain(noto(2))
            .collect(),
            Self::Traditional => [
                win("msjh.ttc"),
                win("mingliu.ttc"),
                path("/System/Library/Fonts/PingFang.ttc", 0),
            ]
            .into_iter()
            .chain(noto(3))
            .collect(),
        }
    }
}

/// A font file read into memory, with the face to use.
#[derive(Debug)]
pub struct CjkFont {
    pub bytes: Vec<u8>,
    pub index: u32,
}

static FONTS: [OnceLock<Option<CjkFont>>; 3] = [OnceLock::new(), OnceLock::new(), OnceLock::new()];

/// The system font for `script`, or `None` when none is installed.
pub fn cjk_font(script: CjkScript) -> Option<&'static CjkFont> {
    let slot = match script {
        CjkScript::Japanese => &FONTS[0],
        CjkScript::Simplified => &FONTS[1],
        CjkScript::Traditional => &FONTS[2],
    };
    slot.get_or_init(|| {
        script.candidates().into_iter().find_map(|(path, index)| {
            let bytes = std::fs::read(&path).ok()?;
            Some(CjkFont { bytes, index })
        })
    })
    .as_ref()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn locales_map_to_scripts() {
        assert_eq!(CjkScript::of_locale("ja"), Some(CjkScript::Japanese));
        assert_eq!(
            CjkScript::of_locale("zh-Hant"),
            Some(CjkScript::Traditional)
        );
        assert_eq!(CjkScript::of_locale("pt-BR"), None);
        for script in CjkScript::ALL {
            assert!(!script.candidates().is_empty());
        }
    }
}
