//! Free subtitle search + download (OpenSubtitles REST, no paid key).
//!
//! Uses the public REST search used by many open clients, ranked by
//! IMDb match, rating, and downloads so picks stay accurate like MovieBox.

use flate2::read::GzDecoder;
use serde::Deserialize;
use std::io::Read;
use std::path::PathBuf;
use std::time::Duration;

const OS_SEARCH: &str = "https://rest.opensubtitles.org/search";
/// OpenSubtitles requires a non-empty custom User-Agent on the legacy REST API.
const OS_UA: &str = "MovieBoxTUI v0.1";

#[derive(Debug, Clone)]
pub struct FreeSubtitle {
    pub language: String,
    pub label: String,
    pub download_url: String,
    pub release_name: String,
    pub rating: f64,
    pub downloads: u64,
    pub format: String,
}

#[derive(Debug, thiserror::Error)]
pub enum SubtitleError {
    #[error("network error: {0}")]
    Network(#[from] reqwest::Error),
    #[error("parse error: {0}")]
    Parse(String),
    #[error("download failed: {0}")]
    Download(String),
}

#[derive(Clone)]
pub struct SubtitleClient {
    http: reqwest::Client,
}

impl Default for SubtitleClient {
    fn default() -> Self {
        Self::new()
    }
}

impl SubtitleClient {
    pub fn new() -> Self {
        Self {
            http: reqwest::Client::builder()
                .timeout(Duration::from_secs(25))
                .connect_timeout(Duration::from_secs(8))
                .user_agent(OS_UA)
                .redirect(reqwest::redirect::Policy::limited(8))
                .build()
                .unwrap_or_default(),
        }
    }

    /// Search free subtitles. Prefer IMDb id when available (most accurate).
    pub async fn search(
        &self,
        imdb_id: Option<&str>,
        title: Option<&str>,
        year: Option<&str>,
        season: Option<usize>,
        episode: Option<usize>,
        release_hint: Option<&str>,
    ) -> Result<Vec<FreeSubtitle>, SubtitleError> {
        let mut results = Vec::new();

        if let Some(imdb) = imdb_id.map(normalize_imdb_digits).filter(|s| !s.is_empty()) {
            // Preferred languages first, then everything else.
            for lang in ["eng", "ara", "spa", "fre", "ger", "ita", "por", "tur", "all"] {
                let mut path = format!("imdbid-{imdb}/sublanguageid-{lang}");
                if let (Some(s), Some(e)) = (season, episode) {
                    if s > 0 && e > 0 {
                        path.push_str(&format!("/season-{s}/episode-{e}"));
                    }
                }
                if let Ok(mut batch) = self.search_path(&path).await {
                    results.append(&mut batch);
                }
                // eng+all usually enough; stop early if we already have a solid set
                if lang == "all" || results.len() >= 40 {
                    break;
                }
            }
        }

        // Fallback: title query when no IMDb (Archive-only ids).
        if results.is_empty() {
            if let Some(t) = title.map(str::trim).filter(|s| !s.is_empty()) {
                let mut path = format!("query-{}/sublanguageid-all", sanitize_query(t));
                if let Some(y) = year.map(str::trim).filter(|s| s.len() >= 4) {
                    path.push_str(&format!("/movieyear-{}", &y[..4]));
                }
                if let (Some(s), Some(e)) = (season, episode) {
                    if s > 0 && e > 0 {
                        path.push_str(&format!("/season-{s}/episode-{e}"));
                    }
                }
                if let Ok(batch) = self.search_path(&path).await {
                    results = batch;
                }
            }
        }

        // Rank for accuracy.
        let hint = release_hint.unwrap_or("").to_ascii_lowercase();
        results.sort_by(|a, b| {
            let ar = release_bonus(&hint, &a.release_name) + lang_bonus(&a.language);
            let br = release_bonus(&hint, &b.release_name) + lang_bonus(&b.language);
            br.cmp(&ar)
                .then_with(|| {
                    b.rating
                        .partial_cmp(&a.rating)
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
                .then_with(|| b.downloads.cmp(&a.downloads))
        });

        // One best entry per language (keeps the popup clean like MovieBox).
        let mut seen_lang = std::collections::HashSet::new();
        let mut deduped = Vec::new();
        for sub in results {
            let key = sub.language.to_ascii_lowercase();
            // Allow English a second high-quality pick if release matches strongly.
            let allow_extra = key == "english"
                && release_bonus(&hint, &sub.release_name) >= 40
                && seen_lang.contains(&key);
            if seen_lang.contains(&key) && !allow_extra {
                continue;
            }
            if !allow_extra {
                seen_lang.insert(key);
            }
            deduped.push(sub);
            if deduped.len() >= 16 {
                break;
            }
        }

        Ok(deduped)
    }

    /// MovieBox-shaped caption payload for the existing subtitle picker UI.
    pub async fn as_moviebox_captions(
        &self,
        imdb_id: Option<&str>,
        title: Option<&str>,
        year: Option<&str>,
        season: Option<usize>,
        episode: Option<usize>,
        release_hint: Option<&str>,
    ) -> serde_json::Value {
        let subs = self
            .search(imdb_id, title, year, season, episode, release_hint)
            .await
            .unwrap_or_default();
        let ext: Vec<serde_json::Value> = subs
            .into_iter()
            .map(|s| {
                let rating = if s.rating > 0.0 {
                    format!(" ★{:.0}", s.rating)
                } else {
                    String::new()
                };
                let rel = if s.release_name.is_empty() {
                    String::new()
                } else {
                    format!(" · {}", truncate(&s.release_name, 36))
                };
                serde_json::json!({
                    "lanName": format!("{}{}{}", s.language, rating, rel),
                    "url": s.download_url,
                    "language": s.language,
                    "format": s.format,
                })
            })
            .collect();
        serde_json::json!({ "extCaptions": ext })
    }

    /// Download (and gunzip if needed) a subtitle to a local temp .srt path.
    pub async fn materialize_local(&self, download_url: &str) -> Result<PathBuf, SubtitleError> {
        let bytes = self
            .http
            .get(download_url)
            .header("User-Agent", OS_UA)
            .send()
            .await?
            .error_for_status()?
            .bytes()
            .await?;

        let raw = maybe_gunzip(&bytes).map_err(|e| SubtitleError::Download(e))?;
        // Basic sanity: looks like a subtitle
        let text = String::from_utf8_lossy(&raw);
        if !text.contains("-->") && !text.contains("[Script Info]") {
            return Err(SubtitleError::Download(
                "downloaded file does not look like a subtitle".into(),
            ));
        }

        let path = std::env::temp_dir().join(format!(
            "moviebox_free_sub_{}.srt",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis()
        ));
        tokio::fs::write(&path, raw)
            .await
            .map_err(|e| SubtitleError::Download(e.to_string()))?;
        Ok(path)
    }

    async fn search_path(&self, path: &str) -> Result<Vec<FreeSubtitle>, SubtitleError> {
        let url = format!("{OS_SEARCH}/{path}");
        let resp = self
            .http
            .get(&url)
            .header("User-Agent", OS_UA)
            .header("Accept", "application/json")
            .send()
            .await?
            .error_for_status()?;
        let rows: Vec<OsSub> = resp
            .json()
            .await
            .map_err(|e| SubtitleError::Parse(e.to_string()))?;

        let mut out = Vec::new();
        for row in rows {
            let url = row
                .sub_download_link
                .or(row.zip_download_link)
                .filter(|u| !u.is_empty());
            let Some(download_url) = url else {
                continue;
            };
            // Skip obviously bad/ads-only if flagged
            if row.sub_bad.as_deref() == Some("1") {
                continue;
            }
            let language = row
                .language_name
                .or(row.sub_language_id)
                .unwrap_or_else(|| "Unknown".into());
            let release = row
                .movie_release_name
                .or(row.sub_file_name)
                .unwrap_or_default();
            let rating = row
                .sub_rating
                .as_deref()
                .and_then(|s| s.parse::<f64>().ok())
                .unwrap_or(0.0);
            let downloads = row
                .sub_downloads_cnt
                .as_deref()
                .and_then(|s| s.parse::<u64>().ok())
                .unwrap_or(0);
            let format = row.sub_format.unwrap_or_else(|| "srt".into());
            let label = format!("{language} · {release}");
            out.push(FreeSubtitle {
                language,
                label,
                download_url,
                release_name: release,
                rating,
                downloads,
                format,
            });
        }
        Ok(out)
    }
}

#[derive(Debug, Deserialize)]
struct OsSub {
    #[serde(rename = "SubDownloadLink")]
    sub_download_link: Option<String>,
    #[serde(rename = "ZipDownloadLink")]
    zip_download_link: Option<String>,
    #[serde(rename = "LanguageName")]
    language_name: Option<String>,
    #[serde(rename = "SubLanguageID")]
    sub_language_id: Option<String>,
    #[serde(rename = "MovieReleaseName")]
    movie_release_name: Option<String>,
    #[serde(rename = "SubFileName")]
    sub_file_name: Option<String>,
    #[serde(rename = "SubRating")]
    sub_rating: Option<String>,
    #[serde(rename = "SubDownloadsCnt")]
    sub_downloads_cnt: Option<String>,
    #[serde(rename = "SubFormat")]
    sub_format: Option<String>,
    #[serde(rename = "SubBad")]
    sub_bad: Option<String>,
}

fn normalize_imdb_digits(id: &str) -> String {
    id.trim()
        .trim_start_matches("tt")
        .trim_start_matches("TT")
        .chars()
        .filter(|c| c.is_ascii_digit())
        .collect()
}

fn sanitize_query(title: &str) -> String {
    title
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '+' })
        .collect::<String>()
        .trim_matches('+')
        .to_string()
}

fn lang_bonus(language: &str) -> i32 {
    match language.to_ascii_lowercase().as_str() {
        "english" | "eng" => 30,
        "arabic" | "ara" => 20,
        "spanish" | "spa" | "french" | "fre" | "german" | "ger" => 15,
        _ => 5,
    }
}

fn release_bonus(hint: &str, release: &str) -> i32 {
    if hint.is_empty() || release.is_empty() {
        return 0;
    }
    let r = release.to_ascii_lowercase();
    let mut score = 0;
    for token in ["bluray", "web-dl", "webrip", "hdtv", "1080p", "720p", "2160p", "x264", "x265", "hevc"]
    {
        if hint.contains(token) && r.contains(token) {
            score += 15;
        }
    }
    // Shared alphanumeric tokens from the stream filename.
    let hint_tokens: Vec<_> = hint
        .split(|c: char| !c.is_alphanumeric())
        .filter(|t| t.len() >= 4)
        .collect();
    let matched = hint_tokens.iter().filter(|t| r.contains(*t)).count();
    score += (matched as i32) * 8;
    score.min(80)
}

fn maybe_gunzip(bytes: &[u8]) -> Result<Vec<u8>, String> {
    // gzip magic 1f 8b
    if bytes.len() >= 2 && bytes[0] == 0x1f && bytes[1] == 0x8b {
        let mut decoder = GzDecoder::new(bytes);
        let mut out = Vec::new();
        decoder
            .read_to_end(&mut out)
            .map_err(|e| format!("gunzip: {e}"))?;
        return Ok(out);
    }
    Ok(bytes.to_vec())
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let mut out: String = s.chars().take(max.saturating_sub(1)).collect();
        out.push('…');
        out
    }
}
