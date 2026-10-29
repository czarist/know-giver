//! Pictures for the lesson whiteboard, from Portuguese Wikipedia.
//!
//! The tutor writes "IMAGEM: <article title>" and the lesson opens with the
//! topic's title. Only that search term leaves the computer; results are
//! cached on disk so a picture is fetched once.

use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::{Engine, Event};

const API: &str = "https://pt.wikipedia.org/w/api.php";
const USER_AGENT: &str = concat!(
    "KnowGiver/",
    env!("CARGO_PKG_VERSION"),
    " (desktop study app; https://github.com/czarist/know-giver)"
);
/// Width of the thumbnail asked from Wikimedia.
const THUMB_WIDTH: u32 = 480;
/// Largest picture accepted.
const MAX_BYTES: u64 = 3 * 1024 * 1024;
/// Longest search term sent.
const MAX_QUERY_CHARS: usize = 120;

/// A picture ready to show.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BoardImage {
    pub bytes: Arc<[u8]>,
    /// File extension of `bytes` ("jpg" or "png"), for the image loader.
    pub format: &'static str,
    /// Wikipedia article the picture illustrates.
    pub title: String,
    /// Article address, where the picture's author and license are listed.
    pub page_url: String,
}

/// State of one search.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Illustration {
    Loading,
    Ready(BoardImage),
    /// Nothing suitable, or the lookup failed: the board skips the entry.
    Missing,
}

/// Every search of this session, by normalized term.
#[derive(Default)]
pub(crate) struct Illustrations {
    found: HashMap<String, Illustration>,
}

/// Search term as the board shows it, used as the key.
pub fn illustration_key(query: &str) -> String {
    query
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(MAX_QUERY_CHARS)
        .collect()
}

impl Engine {
    /// The picture for a board "IMAGEM:" entry, when one was looked up.
    pub fn illustration(&self, query: &str) -> Option<&Illustration> {
        self.illustrations.found.get(&illustration_key(query))
    }

    /// Starts looking up `query` unless it is known or pictures are off.
    pub(crate) fn request_illustration(&mut self, query: &str) {
        let key = illustration_key(query);
        if key.is_empty()
            || !self.settings.board_images
            || self.illustrations.found.contains_key(&key)
        {
            return;
        }
        self.illustrations
            .found
            .insert(key.clone(), Illustration::Loading);
        let cache = self.paths.data_dir.join("board-images");
        self.spawn(move || {
            let result = lookup(&cache, &key);
            if let Err(err) = &result {
                tracing::warn!(%err, query = %key, "board picture not found");
            }
            Event::Illustration {
                query: key,
                image: result.ok().flatten(),
            }
        });
    }

    pub(crate) fn on_illustration(&mut self, query: String, image: Option<BoardImage>) {
        let state = image.map_or(Illustration::Missing, Illustration::Ready);
        self.illustrations.found.insert(query, state);
    }
}

/// Pictures shown per topic in a handout: the topic's own, then sections.
const HANDOUT_PICTURES: usize = 3;

/// Looks up the handout's pictures and attaches them: one of each topic, and
/// those the sections ask for. Topics or sections without a fitting picture
/// are left as they are. Runs on a worker thread (it may use the network).
pub(crate) fn illustrate(document: &mut kg_handout::Document, cache: &Path) {
    let source = document.labels.picture_source.clone();
    let mut files: Vec<(String, Vec<u8>)> = Vec::new();
    let mut find = |query: &str| -> Option<kg_handout::Picture> {
        let query = illustration_key(query);
        if query.is_empty() {
            return None;
        }
        let image = match lookup(cache, &query) {
            Ok(image) => image?,
            Err(err) => {
                tracing::warn!(%err, %query, "handout picture not found");
                return None;
            }
        };
        let file = format!("/img/{}.{}", files.len() + 1, image.format);
        files.push((file.clone(), image.bytes.to_vec()));
        let caption = if source.is_empty() {
            image.title
        } else {
            format!("{source} · {}", image.title)
        };
        Some(kg_handout::Picture { file, caption })
    };
    for topic in &mut document.topics {
        topic.picture = find(&topic.topic);
        let mut left = HANDOUT_PICTURES - usize::from(topic.picture.is_some());
        for section in &mut topic.sections {
            if left == 0 {
                break;
            }
            if !section.image.trim().is_empty() {
                section.picture = find(&section.image);
                left -= usize::from(section.picture.is_some());
            }
        }
    }
    document.files = files;
}

#[derive(Debug, Serialize, Deserialize)]
struct CachedMeta {
    title: String,
    page_url: String,
    format: String,
}

/// Cached result, or a Wikipedia search. `Ok(None)` when nothing fits.
fn lookup(cache: &Path, query: &str) -> Result<Option<BoardImage>, String> {
    let stem = cache.join(format!("{:016x}", fnv(query)));
    let meta_path = stem.with_extension("json");
    if let Ok(json) = fs::read_to_string(&meta_path) {
        if json.trim() == "null" {
            return Ok(None);
        }
        if let Ok(meta) = serde_json::from_str::<CachedMeta>(&json)
            && let Some(format) = known_format(&meta.format)
            && let Ok(bytes) = fs::read(stem.with_extension(format))
        {
            return Ok(Some(BoardImage {
                bytes: bytes.into(),
                format,
                title: meta.title,
                page_url: meta.page_url,
            }));
        }
    }
    let found = search(query)?;
    let _ = fs::create_dir_all(cache);
    match &found {
        Some(image) => {
            let meta = CachedMeta {
                title: image.title.clone(),
                page_url: image.page_url.clone(),
                format: image.format.to_owned(),
            };
            let _ = fs::write(stem.with_extension(image.format), &image.bytes);
            if let Ok(json) = serde_json::to_string(&meta) {
                let _ = fs::write(&meta_path, json);
            }
        }
        // Remember misses too, so the same term is not searched again.
        None => {
            let _ = fs::write(&meta_path, "null");
        }
    }
    Ok(found)
}

#[derive(Debug, Deserialize)]
struct SearchReply {
    #[serde(default)]
    query: Option<SearchPages>,
}

#[derive(Debug, Deserialize)]
struct SearchPages {
    #[serde(default)]
    pages: Vec<Page>,
}

#[derive(Debug, Deserialize)]
struct Page {
    title: String,
    #[serde(default)]
    index: u32,
    #[serde(default)]
    fullurl: String,
    #[serde(default)]
    thumbnail: Option<Thumbnail>,
}

#[derive(Debug, Deserialize)]
struct Thumbnail {
    source: String,
}

fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(15)))
        .build()
        .into()
}

fn search(query: &str) -> Result<Option<BoardImage>, String> {
    let agent = agent();
    let mut response = agent
        .get(API)
        .header("User-Agent", USER_AGENT)
        .query("action", "query")
        .query("format", "json")
        .query("formatversion", "2")
        .query("generator", "search")
        .query("gsrsearch", query)
        .query("gsrnamespace", "0")
        .query("gsrlimit", "4")
        .query("prop", "pageimages|info")
        .query("piprop", "thumbnail")
        .query("pithumbsize", THUMB_WIDTH.to_string())
        .query("inprop", "url")
        .call()
        .map_err(|e| e.to_string())?;
    let reply: SearchReply = response
        .body_mut()
        .with_config()
        .limit(1024 * 1024)
        .read_json()
        .map_err(|e| e.to_string())?;
    let mut pages = reply.query.map(|q| q.pages).unwrap_or_default();
    pages.sort_by_key(|p| p.index);
    let Some((page, thumb)) = pages
        .into_iter()
        .filter(|page| related(query, &page.title))
        .find_map(|page| {
            let source = page.thumbnail.as_ref()?.source.clone();
            Some((page, source))
        })
    else {
        return Ok(None);
    };
    let Some(format) = format_of(&thumb) else {
        return Ok(None);
    };
    let bytes = agent
        .get(&thumb)
        .header("User-Agent", USER_AGENT)
        .call()
        .map_err(|e| e.to_string())?
        .body_mut()
        .with_config()
        .limit(MAX_BYTES)
        .read_to_vec()
        .map_err(|e| e.to_string())?;
    Ok(Some(BoardImage {
        bytes: bytes.into(),
        format,
        title: page.title,
        page_url: page.fullurl,
    }))
}

/// The article shares a meaningful word with the search, so a loose match
/// on an unrelated page is not shown. Words are compared by their first
/// letters, so "redes neurais" matches "rede neural".
fn related(query: &str, title: &str) -> bool {
    let words = |text: &str| -> Vec<String> {
        text.split(|c: char| !c.is_alphanumeric())
            .filter(|w| w.chars().count() >= 4 || w.chars().all(|c| c.is_ascii_digit()))
            .filter(|w| !w.is_empty())
            .map(|w| w.to_lowercase().chars().take(5).collect())
            .collect()
    };
    let title = words(title);
    words(query).iter().any(|w| title.contains(w))
}

fn format_of(url: &str) -> Option<&'static str> {
    // Thumbnail addresses carry tracking parameters after the file name.
    let path = url.split(['?', '#']).next().unwrap_or(url);
    let lower = path.to_lowercase();
    if lower.ends_with(".jpg") || lower.ends_with(".jpeg") {
        Some("jpg")
    } else if lower.ends_with(".png") {
        Some("png")
    } else {
        None
    }
}

fn known_format(format: &str) -> Option<&'static str> {
    match format {
        "jpg" => Some("jpg"),
        "png" => Some("png"),
        _ => None,
    }
}

/// Stable file name for a search term.
fn fnv(text: &str) -> u64 {
    text.bytes().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn handout_gets_pictures_from_the_cache() {
        let cache = tempfile::tempdir().unwrap();
        let put = |query: &str, meta: &str, bytes: Option<&[u8]>| {
            let stem = cache.path().join(format!("{:016x}", fnv(query)));
            fs::write(stem.with_extension("json"), meta).unwrap();
            if let Some(bytes) = bytes {
                fs::write(stem.with_extension("png"), bytes).unwrap();
            }
        };
        put(
            "Crase",
            r#"{"title":"Crase","page_url":"https://pt.wikipedia.org/wiki/Crase","format":"png"}"#,
            Some(b"png-bytes"),
        );
        put("Escola", "null", None);
        let mut document = kg_handout::Document {
            title: String::new(),
            subtitle: String::new(),
            generated: String::new(),
            lang: "pt".into(),
            labels: kg_handout::Labels {
                picture_source: "Imagem: Wikipédia".into(),
                ..Default::default()
            },
            topics: vec![kg_handout::TopicHandout {
                topic: "Crase".into(),
                sections: vec![kg_handout::Section {
                    heading: "Uso".into(),
                    image: "Escola".into(),
                    ..Default::default()
                }],
                ..Default::default()
            }],
            files: Vec::new(),
        };
        illustrate(&mut document, cache.path());
        let topic = &document.topics[0];
        let picture = topic.picture.as_ref().unwrap();
        assert_eq!(picture.caption, "Imagem: Wikipédia · Crase");
        assert_eq!(
            document.files,
            [(picture.file.clone(), b"png-bytes".to_vec())]
        );
        // Known to have no picture: the section stays as it is.
        assert_eq!(topic.sections[0].picture, None);
    }

    #[test]
    fn keeps_only_related_articles_and_known_formats() {
        assert!(related("Palácio do Planalto", "Palácio do Planalto"));
        assert!(related(
            "Constituição de 1988",
            "Constituição brasileira de 1988"
        ));
        assert!(!related("Lei de Responsabilidade Fiscal", "Futebol"));
        assert!(related(
            "Redes neurais artificiais",
            "Rede neural artificial"
        ));
        assert_eq!(
            format_of("https://upload.wikimedia.org/a/b/480px-X.jpg?utm_source=pt.wikipedia.org"),
            Some("jpg")
        );
        assert_eq!(format_of("https://x/y/400px-Foo.JPG"), Some("jpg"));
        assert_eq!(format_of("https://x/y/400px-Foo.svg.png"), Some("png"));
        assert_eq!(format_of("https://x/y/400px-Foo.gif"), None);
        assert_eq!(
            illustration_key("  Palácio   do Planalto "),
            "Palácio do Planalto"
        );
    }

    #[test]
    #[ignore = "reaches pt.wikipedia.org; run with --ignored"]
    fn live_search_finds_pictures() {
        for query in ["Palácio do Planalto", "Redes neurais artificiais"] {
            let image = search(query).unwrap().unwrap();
            assert!(image.bytes.len() > 1_000, "{query}");
            let magic: &[u8] = if image.format == "png" {
                b"\x89PNG"
            } else {
                b"\xFF\xD8"
            };
            assert!(image.bytes.starts_with(magic), "{query}");
            println!(
                "{query}: {} ({}, {} bytes)",
                image.title,
                image.format,
                image.bytes.len()
            );
        }
        assert_eq!(search("xqzv kkkwpq zzzt").unwrap(), None);
    }

    #[test]
    fn cached_results_are_read_back_without_network() {
        let dir = tempfile::tempdir().unwrap();
        let stem = dir.path().join(format!("{:016x}", fnv("Brasília")));
        std::fs::write(stem.with_extension("png"), [1u8, 2, 3]).unwrap();
        std::fs::write(
            stem.with_extension("json"),
            r#"{"title":"Brasília","page_url":"https://pt.wikipedia.org/wiki/Bras%C3%ADlia","format":"png"}"#,
        )
        .unwrap();
        let image = lookup(dir.path(), "Brasília").unwrap().unwrap();
        assert_eq!(&*image.bytes, &[1, 2, 3]);
        assert_eq!(image.title, "Brasília");
        std::fs::write(
            dir.path()
                .join(format!("{:016x}", fnv("Nada")))
                .with_extension("json"),
            "null",
        )
        .unwrap();
        assert_eq!(lookup(dir.path(), "Nada").unwrap(), None);
    }
}
