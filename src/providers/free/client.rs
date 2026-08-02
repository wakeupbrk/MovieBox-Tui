//! Free catalog + streams from Cinemeta (search) and Internet Archive (files).

use super::subtitles::SubtitleClient;
use crate::providers::models::{
    CatalogItem, Episode, MediaDetails, MediaType, ProviderKind, ProviderMediaId, Season,
};
use serde::Deserialize;
use std::time::Duration;

const CINEMETA: &str = "https://v3-cinemeta.strem.io";
const ARCHIVE_SEARCH: &str = "https://archive.org/advancedsearch.php";
const ARCHIVE_META: &str = "https://archive.org/metadata";
const ARCHIVE_DOWNLOAD: &str = "https://archive.org/download";

#[derive(Debug, thiserror::Error)]
pub enum FreeError {
    #[error("network error: {0}")]
    Network(#[from] reqwest::Error),
    #[error("parse error: {0}")]
    Parse(String),
    #[error("not found")]
    NotFound,
    #[error("no free streams found for this title")]
    NoStreams,
}

#[derive(Clone)]
pub struct FreeClient {
    http: reqwest::Client,
    subs: SubtitleClient,
}

impl Default for FreeClient {
    fn default() -> Self {
        Self::new()
    }
}

impl FreeClient {
    pub fn new() -> Self {
        Self {
            http: reqwest::Client::builder()
                .timeout(Duration::from_secs(25))
                .connect_timeout(Duration::from_secs(8))
                .user_agent(
                    "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) \
                     MovieBox-Tui/0.1 FreeClient",
                )
                .build()
                .unwrap_or_default(),
            subs: SubtitleClient::new(),
        }
    }

    /// Free subtitle options in MovieBox `extCaptions` shape for the shared picker.
    pub async fn subtitles_moviebox_json(
        &self,
        imdb_id: Option<&str>,
        title: Option<&str>,
        year: Option<&str>,
        season: Option<usize>,
        episode: Option<usize>,
        release_hint: Option<&str>,
    ) -> serde_json::Value {
        self.subs
            .as_moviebox_captions(imdb_id, title, year, season, episode, release_hint)
            .await
    }

    /// Download OpenSubtitles (.gz) to a local temp .srt for mpv/VLC.
    pub async fn materialize_subtitle(
        &self,
        download_url: &str,
    ) -> Result<std::path::PathBuf, FreeError> {
        self.subs
            .materialize_local(download_url)
            .await
            .map_err(|e| FreeError::Parse(e.to_string()))
    }

    /// Search Cinemeta + Archive.org and merge free-playable candidates first.
    pub async fn search(&self, query: &str) -> Result<Vec<CatalogItem>, FreeError> {
        let q = query.trim();
        if q.is_empty() {
            return Ok(Vec::new());
        }

        let (cine, arch) = tokio::join!(self.search_cinemeta(q), self.search_archive(q));

        let mut items = Vec::new();
        let mut seen = std::collections::HashSet::new();

        // Cinemeta first (clean titles / posters), then Archive free-file hits.
        if let Ok(list) = cine {
            for item in list {
                if seen.insert(item.id.value.clone()) {
                    items.push(item);
                }
            }
        }
        if let Ok(list) = arch {
            for item in list {
                // Drop obvious non-movies from Archive search UI.
                if is_archive_noise(&item.id.value, &item.title) {
                    continue;
                }
                if seen.insert(item.id.value.clone()) {
                    items.push(item);
                }
            }
        }

        // Prefer titles that match the query closely.
        items.sort_by(|a, b| {
            let sa = title_match_score(q, &a.title);
            let sb = title_match_score(q, &b.title);
            sb.cmp(&sa)
                .then_with(|| a.title.to_lowercase().cmp(&b.title.to_lowercase()))
        });
        Ok(items)
    }

    pub async fn details(&self, id: &str) -> Result<MediaDetails, FreeError> {
        let id = id.trim();
        if id.starts_with("tt") {
            return self.details_cinemeta(id).await;
        }
        // Archive identifier
        self.details_archive(id).await
    }

    /// Resolve free HTTPS video files for a title/id.
    pub async fn streams(
        &self,
        id: &str,
        title_hint: Option<&str>,
    ) -> Result<Vec<FreeStream>, FreeError> {
        let id = id.trim();
        let mut streams = Vec::new();

        if !id.starts_with("tt") {
            // Direct archive item
            streams.extend(self.archive_item_streams(id).await.unwrap_or_default());
        }

        // Title-based archive search (works for Cinemeta IMDB ids too).
        let title = if let Some(t) = title_hint.filter(|s| !s.is_empty()) {
            t.to_string()
        } else if id.starts_with("tt") {
            self.details_cinemeta(id)
                .await
                .map(|d| d.title)
                .unwrap_or_else(|_| id.to_string())
        } else {
            id.to_string()
        };

        // Search archive for video files matching the title closely.
        let mut candidates: Vec<CatalogItem> = Vec::new();
        if let Ok(hits) = self.search_archive(&title).await {
            candidates.extend(hits);
        }
        for variant in archive_query_variants(&title) {
            if let Ok(hits) = self.search_archive_raw(&variant).await {
                candidates.extend(hits);
            }
        }

        let mut seen_ids = std::collections::HashSet::new();
        candidates.retain(|hit| {
            if is_archive_noise(&hit.id.value, &hit.title) {
                return false;
            }
            // Only keep archive items that look like the same movie.
            if title_match_score(&title, &hit.title) < 500 {
                return false;
            }
            seen_ids.insert(hit.id.value.clone())
        });
        // Prefer best title matches first.
        candidates.sort_by(|a, b| {
            title_match_score(&title, &b.title).cmp(&title_match_score(&title, &a.title))
        });

        for hit in candidates.into_iter().take(6) {
            if let Ok(files) = self.archive_item_streams(&hit.id.value).await {
                for f in files {
                    // Prefer filenames that also mention the title.
                    if title_match_score(&title, &f.filename) < 200
                        && title_match_score(&title, &hit.title) < 700
                    {
                        continue;
                    }
                    streams.push(f);
                }
            }
        }

        // Dedupe by URL
        let mut seen = std::collections::HashSet::new();
        streams.retain(|s| seen.insert(s.url.clone()));
        // Prefer larger / higher-res first
        streams.sort_by(|a, b| {
            b.resolution
                .unwrap_or(0)
                .cmp(&a.resolution.unwrap_or(0))
                .then_with(|| b.size_bytes.unwrap_or(0).cmp(&a.size_bytes.unwrap_or(0)))
        });

        if streams.is_empty() {
            return Err(FreeError::NoStreams);
        }
        Ok(streams)
    }

    async fn search_cinemeta(&self, query: &str) -> Result<Vec<CatalogItem>, FreeError> {
        let encoded = url_encode(query);
        let movie_url = format!("{CINEMETA}/catalog/movie/top/search={encoded}.json");
        let series_url = format!("{CINEMETA}/catalog/series/top/search={encoded}.json");
        let (m, s) = tokio::join!(
            self.get_json::<CineCatalog>(&movie_url),
            self.get_json::<CineCatalog>(&series_url)
        );
        let mut items = Vec::new();
        for cat in [m.ok(), s.ok()].into_iter().flatten() {
            for meta in cat.metas {
                let id = meta
                    .imdb_id
                    .or(meta.id)
                    .filter(|x| x.starts_with("tt"))
                    .unwrap_or_default();
                if id.is_empty() {
                    continue;
                }
                let media_type = if meta.r#type.as_deref() == Some("series") {
                    MediaType::Series
                } else {
                    MediaType::Movie
                };
                items.push(CatalogItem {
                    id: ProviderMediaId {
                        provider: ProviderKind::Free,
                        value: id,
                    },
                    title: meta.name.unwrap_or_else(|| "Unknown".into()),
                    media_type,
                    year: meta
                        .release_info
                        .or(meta.year)
                        .map(|y| y.chars().take(4).collect()),
                    poster_url: meta.poster,
                    season_count: None,
                });
            }
        }
        Ok(items)
    }

    async fn search_archive(&self, query: &str) -> Result<Vec<CatalogItem>, FreeError> {
        // Prefer items that actually host video files.
        let q = format!(
            "title:(\"{}\") AND (format:MPEG4 OR format:\"h.264\" OR format:Matroska OR format:Ogg Video OR format:WebM)",
            query.replace('"', "")
        );
        self.search_archive_raw(&q).await
    }

    async fn search_archive_raw(&self, query: &str) -> Result<Vec<CatalogItem>, FreeError> {
        let url = format!(
            "{ARCHIVE_SEARCH}?q={}&fl[]=identifier&fl[]=title&fl[]=year&fl[]=downloads&rows=12&page=1&output=json",
            url_encode(query)
        );
        let resp: ArchiveSearch = self.get_json(&url).await?;
        let mut items = Vec::new();
        for doc in resp.response.docs {
            let Some(identifier) = doc.identifier else {
                continue;
            };
            // Skip obvious non-feature dumps
            let title = doc.title.unwrap_or_else(|| identifier.clone());
            let lower = title.to_ascii_lowercase();
            if lower.contains("commentary") && !lower.contains("enter the void") {
                // still allow Enter the Void commentaries as last resort — keep for now
            }
            items.push(CatalogItem {
                id: ProviderMediaId {
                    provider: ProviderKind::Free,
                    value: identifier,
                },
                title,
                media_type: MediaType::Movie,
                year: doc.year.and_then(|y| match y {
                    serde_json::Value::String(s) => Some(s.chars().take(4).collect()),
                    serde_json::Value::Number(n) => Some(n.to_string()),
                    _ => None,
                }),
                poster_url: None,
                season_count: None,
            });
        }
        // Rank by downloads when present
        items.sort_by(|a, b| b.id.value.cmp(&a.id.value));
        Ok(items)
    }

    async fn details_cinemeta(&self, imdb: &str) -> Result<MediaDetails, FreeError> {
        let movie_url = format!("{CINEMETA}/meta/movie/{imdb}.json");
        let series_url = format!("{CINEMETA}/meta/series/{imdb}.json");
        let meta = match self.get_json::<CineMetaWrap>(&movie_url).await {
            Ok(w) if w.meta.as_ref().and_then(|m| m.name.as_ref()).is_some() => w.meta.unwrap(),
            _ => {
                self.get_json::<CineMetaWrap>(&series_url)
                    .await?
                    .meta
                    .ok_or(FreeError::NotFound)?
            }
        };
        let media_type = if meta.r#type.as_deref() == Some("series") {
            MediaType::Series
        } else {
            MediaType::Movie
        };
        let mut seasons_map: std::collections::BTreeMap<usize, Vec<Episode>> =
            std::collections::BTreeMap::new();
        for video in meta.videos.unwrap_or_default() {
            let season = video.season.unwrap_or(0);
            let number = video.episode.or(video.number).unwrap_or(0);
            if season == 0 || number == 0 {
                continue;
            }
            seasons_map.entry(season).or_default().push(Episode {
                season,
                number,
                title: video.name,
            });
        }
        let seasons = seasons_map
            .into_iter()
            .map(|(number, mut episodes)| {
                episodes.sort_by_key(|e| e.number);
                Season { number, episodes }
            })
            .collect();
        Ok(MediaDetails {
            id: ProviderMediaId {
                provider: ProviderKind::Free,
                value: meta.imdb_id.or(meta.id).unwrap_or_else(|| imdb.into()),
            },
            title: meta.name.unwrap_or_else(|| "Unknown".into()),
            media_type,
            year: meta
                .release_info
                .or(meta.year)
                .map(|y| y.chars().take(4).collect()),
            description: meta.description,
            tagline: None,
            imdb_rating: meta.imdb_rating,
            director: meta.director.map(|d| d.join(", ")),
            stars: meta.cast.map(|c| c.join(", ")),
            prints: None,
            audios: None,
            poster_url: meta.poster,
            genres: meta.genres.or(meta.genre).unwrap_or_default(),
            seasons,
        })
    }

    async fn details_archive(&self, identifier: &str) -> Result<MediaDetails, FreeError> {
        let url = format!("{ARCHIVE_META}/{identifier}");
        let meta: ArchiveMeta = self.get_json(&url).await?;
        let m = meta.metadata.unwrap_or_default();
        let title = m
            .title
            .and_then(|t| t.into_string())
            .unwrap_or_else(|| identifier.into());
        let description = m.description.and_then(|t| t.into_string());
        let year = m.year.and_then(|t| t.into_string());
        Ok(MediaDetails {
            id: ProviderMediaId {
                provider: ProviderKind::Free,
                value: identifier.to_string(),
            },
            title,
            media_type: MediaType::Movie,
            year,
            description,
            tagline: None,
            imdb_rating: None,
            director: m.creator.and_then(|t| t.into_string()),
            stars: None,
            prints: None,
            audios: None,
            poster_url: Some(format!(
                "https://archive.org/services/img/{identifier}"
            )),
            genres: Vec::new(),
            seasons: Vec::new(),
        })
    }

    async fn archive_item_streams(&self, identifier: &str) -> Result<Vec<FreeStream>, FreeError> {
        let url = format!("{ARCHIVE_META}/{identifier}");
        let meta: ArchiveMeta = self.get_json(&url).await?;
        let mut streams = Vec::new();
        for file in meta.files.unwrap_or_default() {
            let name = file.name.unwrap_or_default();
            let lower = name.to_ascii_lowercase();
            if !(lower.ends_with(".mp4")
                || lower.ends_with(".mkv")
                || lower.ends_with(".webm")
                || lower.ends_with(".ogv")
                || lower.ends_with(".avi")
                || lower.ends_with(".m4v"))
            {
                continue;
            }
            // Skip previews, thumbs, samples, partials.
            if lower.contains("sample")
                || lower.contains("trailer")
                || lower.contains("preview")
                || lower.contains("_ia.")
                || lower.contains(".thumbs")
            {
                continue;
            }
            let size = file
                .size
                .as_deref()
                .and_then(|s| s.parse::<u64>().ok())
                .unwrap_or(0);
            if size > 0 && size < 8_000_000 {
                continue;
            }
            let height = file
                .height
                .as_deref()
                .and_then(|h| h.parse::<u32>().ok())
                .unwrap_or(0);
            let width = file
                .width
                .as_deref()
                .and_then(|w| w.parse::<u32>().ok())
                .unwrap_or(0);
            let resolution = guess_resolution(height, width, &lower);
            let duration_secs = file
                .length
                .as_deref()
                .and_then(|l| l.parse::<f64>().ok())
                .map(|s| s.round() as u64)
                .filter(|&s| s > 0);
            let codec = guess_codec(&lower, file.format.as_deref());
            let audio = guess_audio(&lower);
            let quality_tag = guess_quality_tag(&lower);
            let encoded_name = url_encode_path(&name);
            let play_url = format!("{ARCHIVE_DOWNLOAD}/{identifier}/{encoded_name}");
            streams.push(FreeStream {
                title: format!(
                    "{} · {} · {}",
                    resolution.map(|r| format!("{r}p")).unwrap_or_else(|| "SD".into()),
                    codec,
                    audio
                ),
                filename: name.clone(),
                url: play_url,
                size_bytes: if size > 0 { Some(size) } else { None },
                resolution,
                duration_secs,
                codec,
                audio,
                quality_tag,
                format: file.format,
                source: "Archive.org".into(),
                archive_id: identifier.to_string(),
            });
        }
        Ok(streams)
    }

    async fn get_json<T: for<'de> Deserialize<'de>>(&self, url: &str) -> Result<T, FreeError> {
        let text = self
            .http
            .get(url)
            .header("Accept", "application/json")
            .send()
            .await?
            .error_for_status()?
            .text()
            .await?;
        serde_json::from_str(&text).map_err(|e| FreeError::Parse(format!("{e}: {:.180}", text)))
    }
}

#[derive(Debug, Clone)]
pub struct FreeStream {
    pub title: String,
    pub filename: String,
    pub url: String,
    pub size_bytes: Option<u64>,
    pub resolution: Option<u32>,
    pub duration_secs: Option<u64>,
    pub codec: String,
    pub audio: String,
    pub quality_tag: String,
    pub format: Option<String>,
    pub source: String,
    pub archive_id: String,
}

impl FreeStream {
    pub fn display_title(&self) -> String {
        let res = self
            .resolution
            .map(|r| format!("{r}p"))
            .unwrap_or_else(|| "SD".into());
        format!("{res} · {} · {}", self.codec, self.audio)
    }

    pub fn source_label(&self) -> String {
        if self.quality_tag.is_empty() {
            "Archive.org".into()
        } else {
            format!("Archive · {}", self.quality_tag)
        }
    }

    pub fn size_label(&self) -> Option<String> {
        self.size_bytes.map(|b| {
            const KB: f64 = 1024.0;
            const MB: f64 = KB * 1024.0;
            const GB: f64 = MB * 1024.0;
            let n = b as f64;
            if n >= GB {
                format!("{:.1} GB", n / GB)
            } else if n >= MB {
                format!("{:.0} MB", n / MB)
            } else {
                format!("{b} B")
            }
        })
    }
}

fn guess_resolution(height: u32, width: u32, name: &str) -> Option<u32> {
    if name.contains("2160") || name.contains("4k") || name.contains("uhd") {
        return Some(2160);
    }
    if name.contains("1080") || name.contains("fhd") {
        return Some(1080);
    }
    if name.contains("720") {
        return Some(720);
    }
    if name.contains("480") {
        return Some(480);
    }
    if name.contains("360") {
        return Some(360);
    }
    if height >= 1600 || width >= 3000 {
        Some(2160)
    } else if height >= 1000 || width >= 1700 {
        Some(1080)
    } else if height >= 700 || width >= 1200 {
        Some(720)
    } else if height >= 400 {
        Some(480)
    } else if height > 0 {
        Some(360)
    } else {
        None
    }
}

fn guess_codec(name: &str, format: Option<&str>) -> String {
    if name.contains("x265") || name.contains("h265") || name.contains("hevc") {
        return "H265".into();
    }
    if name.contains("av1") {
        return "AV1".into();
    }
    if name.contains("x264") || name.contains("h264") || name.contains("avc") {
        return "H264".into();
    }
    if name.contains("xvid") {
        return "XVID".into();
    }
    match format.unwrap_or("").to_ascii_lowercase().as_str() {
        "matroska" => "MKV".into(),
        "mpeg4" | "h.264" => "H264".into(),
        "h.265" | "high efficiency video coding" => "H265".into(),
        "webm" => "VP9".into(),
        "ogg video" => "THEORA".into(),
        other if !other.is_empty() => other
            .split_whitespace()
            .next()
            .unwrap_or("MP4")
            .to_ascii_uppercase(),
        _ => "MP4".into(),
    }
}

fn guess_audio(name: &str) -> String {
    let mut parts = Vec::new();
    // Language / track style
    if name.contains("dual") || name.contains("multi") {
        parts.push("Multi-Audio");
    } else if name.contains("hindi") || name.contains("[hin]") {
        parts.push("Hindi");
    } else if name.contains("arabic") || name.contains("[ara]") {
        parts.push("Arabic");
    } else if name.contains("spanish") || name.contains("[spa]") {
        parts.push("Spanish");
    } else if name.contains("french") || name.contains("[fre]") {
        parts.push("French");
    } else if name.contains("english") || name.contains("[eng]") || name.contains(".en.") {
        parts.push("English");
    } else {
        parts.push("Default");
    }
    // Codec / channels
    if name.contains("atmos") {
        parts.push("Atmos");
    } else if name.contains("dts-hd") || name.contains("dts_hd") {
        parts.push("DTS-HD");
    } else if name.contains("dts") {
        parts.push("DTS");
    } else if name.contains("truehd") {
        parts.push("TrueHD");
    } else if name.contains("eac3") || name.contains("dd+") || name.contains("ddp") {
        parts.push("DD+");
    } else if name.contains("ac3") || name.contains("dd5") {
        parts.push("DD");
    } else if name.contains("aac") {
        parts.push("AAC");
    }
    if name.contains("5.1") || name.contains("5ch") {
        parts.push("5.1");
    } else if name.contains("7.1") {
        parts.push("7.1");
    } else if name.contains("2.0") || name.contains("stereo") {
        parts.push("2.0");
    }
    parts.join(" ")
}

fn guess_quality_tag(name: &str) -> String {
    if name.contains("remux") {
        "REMUX".into()
    } else if name.contains("bluray") || name.contains("blu-ray") || name.contains("bdrip") {
        "BluRay".into()
    } else if name.contains("web-dl") || name.contains("webdl") {
        "WEB-DL".into()
    } else if name.contains("webrip") || name.contains("web-rip") {
        "WEBRip".into()
    } else if name.contains("hdtv") {
        "HDTV".into()
    } else if name.contains("dvdrip") || name.contains("dvd") {
        "DVD".into()
    } else if name.contains("cam") {
        "CAM".into()
    } else {
        String::new()
    }
}

fn archive_query_variants(title: &str) -> Vec<String> {
    let clean = title.replace('"', "");
    vec![
        format!("\"{clean}\" AND format:MPEG4"),
        format!("title:(\"{clean}\") AND format:MPEG4"),
        format!("{clean} AND format:MPEG4"),
    ]
}

fn normalize_title_key(value: &str) -> String {
    value
        .to_lowercase()
        .chars()
        .filter(|c| c.is_alphanumeric())
        .collect()
}

/// Rough similarity for ranking Archive hits against a movie title.
fn title_match_score(query: &str, title: &str) -> i32 {
    let q = normalize_title_key(query);
    let t = normalize_title_key(title);
    if q.is_empty() || t.is_empty() {
        return 0;
    }
    if t == q {
        return 1000;
    }
    if t.starts_with(&q) || q.starts_with(&t) {
        return 850;
    }
    if t.contains(&q) {
        return 750 - (t.len().saturating_sub(q.len()) as i32).min(200);
    }
    if q.contains(&t) && t.len() >= 6 {
        return 650;
    }
    // token overlap
    let q_tokens: Vec<&str> = query
        .split(|c: char| !c.is_alphanumeric())
        .filter(|s| s.len() >= 3)
        .collect();
    if q_tokens.is_empty() {
        return 0;
    }
    let matched = q_tokens
        .iter()
        .filter(|tok| t.contains(&tok.to_ascii_lowercase()))
        .count();
    if matched == q_tokens.len() {
        return 550 + matched as i32 * 20;
    }
    if matched * 100 / q_tokens.len() >= 70 {
        return 420;
    }
    0
}

fn is_archive_noise(identifier: &str, title: &str) -> bool {
    let id = identifier.to_ascii_lowercase();
    let t = title.to_ascii_lowercase();
    id.starts_with("youtube-")
        || id.contains("macos-tas")
        || t.contains("full mix")
        || t.contains("| enter the void 2")
        || t.contains("commentary")
        || t.contains("cablecast")
        || t.contains("gameplay")
        || t.contains("speedrun")
}

fn url_encode(input: &str) -> String {
    let mut out = String::with_capacity(input.len() * 2);
    for b in input.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            b' ' => out.push_str("%20"),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// Encode a path segment but keep `/` if any (filenames usually flat).
fn url_encode_path(input: &str) -> String {
    url_encode(input)
}

// --- JSON types ---

#[derive(Debug, Deserialize)]
struct CineCatalog {
    #[serde(default)]
    metas: Vec<CineMeta>,
}

#[derive(Debug, Deserialize)]
struct CineMetaWrap {
    meta: Option<CineMeta>,
}

#[derive(Debug, Deserialize)]
struct CineMeta {
    id: Option<String>,
    imdb_id: Option<String>,
    name: Option<String>,
    r#type: Option<String>,
    poster: Option<String>,
    description: Option<String>,
    #[serde(rename = "releaseInfo")]
    release_info: Option<String>,
    year: Option<String>,
    #[serde(rename = "imdbRating")]
    imdb_rating: Option<String>,
    director: Option<Vec<String>>,
    cast: Option<Vec<String>>,
    genres: Option<Vec<String>>,
    genre: Option<Vec<String>>,
    videos: Option<Vec<CineVideo>>,
}

#[derive(Debug, Deserialize)]
struct CineVideo {
    name: Option<String>,
    season: Option<usize>,
    episode: Option<usize>,
    number: Option<usize>,
}

#[derive(Debug, Deserialize)]
struct ArchiveSearch {
    response: ArchiveResponse,
}

#[derive(Debug, Deserialize)]
struct ArchiveResponse {
    #[serde(default)]
    docs: Vec<ArchiveDoc>,
}

#[derive(Debug, Deserialize)]
struct ArchiveDoc {
    identifier: Option<String>,
    title: Option<String>,
    year: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
struct ArchiveMeta {
    metadata: Option<ArchiveMetadata>,
    files: Option<Vec<ArchiveFile>>,
}

#[derive(Debug, Default, Deserialize)]
struct ArchiveMetadata {
    title: Option<FlexStr>,
    description: Option<FlexStr>,
    year: Option<FlexStr>,
    creator: Option<FlexStr>,
}

#[derive(Debug, Deserialize)]
struct ArchiveFile {
    name: Option<String>,
    size: Option<String>,
    format: Option<String>,
    height: Option<String>,
    width: Option<String>,
    /// Duration in seconds (string float from IA).
    length: Option<String>,
}

/// Archive fields are sometimes string, sometimes array of strings.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum FlexStr {
    One(String),
    Many(Vec<String>),
    Num(i64),
}

impl FlexStr {
    fn into_string(self) -> Option<String> {
        match self {
            Self::One(s) => Some(s),
            Self::Many(v) => v.into_iter().next(),
            Self::Num(n) => Some(n.to_string()),
        }
    }
}

impl Default for FlexStr {
    fn default() -> Self {
        Self::One(String::new())
    }
}
