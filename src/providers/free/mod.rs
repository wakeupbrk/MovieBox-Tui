//! Free HTTP sources — no debrid, no torrents.
//!
//! Search: Cinemeta (IMDB catalog) + Internet Archive  
//! Streams: direct Archive.org video files (mp4/mkv/…) playable in mpv  
//! Subtitles: OpenSubtitles (free REST) ranked by IMDb + rating/downloads

mod client;
mod subtitles;

pub use client::{FreeClient, FreeError};
pub use subtitles::SubtitleClient;

use crate::providers::models::{
    CatalogItem, MediaDetails, MediaType, PlaybackSource, ProviderKind, ProviderMediaId, Season,
};
use client::FreeStream;

pub fn search_to_moviebox_json(items: &[CatalogItem]) -> serde_json::Value {
    let subjects = items
        .iter()
        .map(|item| {
            serde_json::json!({
                "subjectId": item.id.value,
                "title": item.title,
                "subjectType": if item.media_type == MediaType::Series { 2 } else { 1 },
                "releaseDate": item.year,
                "cover": { "url": item.poster_url },
                "season": item.season_count.unwrap_or_default(),
                "hasResource": true,
                "_provider": ProviderKind::Free.cache_key(),
            })
        })
        .collect::<Vec<_>>();
    serde_json::json!({ "results": [{ "subjects": subjects }] })
}

pub fn details_to_moviebox_json(details: &MediaDetails) -> serde_json::Value {
    let seasons = details
        .seasons
        .iter()
        .map(|season| {
            serde_json::json!({
                "se": season.number,
                "maxEp": season.episodes.iter().map(|e| e.number).max().unwrap_or(0),
                "episodeNumbers": season.episodes.iter().map(|e| e.number).collect::<Vec<_>>(),
            })
        })
        .collect::<Vec<_>>();
    serde_json::json!({
        "id": details.id.value,
        "subjectId": details.id.value,
        "title": details.title,
        "subjectType": if details.media_type == MediaType::Series { 2 } else { 1 },
        "releaseDate": details.year,
        "description": details.description,
        "imdbRatingValue": details.imdb_rating,
        "director": details.director,
        "stars": details.stars,
        "cover": { "url": details.poster_url },
        "genre": details.genres,
        "hasResource": true,
        "seasons": { "seasons": seasons },
        "_provider": ProviderKind::Free.cache_key(),
    })
}

pub fn streams_to_moviebox_json(streams: &[FreeStream]) -> serde_json::Value {
    // MovieBox UI groups by resolution (1080p · N options) and shows:
    // size | codec | duration | uploader — or language when present.
    let mut cleaned = streams.to_vec();
    cleaned.sort_by(|a, b| {
        b.resolution
            .unwrap_or(0)
            .cmp(&a.resolution.unwrap_or(0))
            .then_with(|| b.size_bytes.unwrap_or(0).cmp(&a.size_bytes.unwrap_or(0)))
    });

    // One best pick per quality + audio + codec bucket (keeps the menu clean).
    let mut seen = std::collections::HashSet::<String>::new();
    cleaned.retain(|s| {
        let key = format!(
            "{}|{}|{}",
            s.resolution.unwrap_or(0),
            s.audio.to_ascii_lowercase(),
            s.codec.to_ascii_lowercase()
        );
        seen.insert(key)
    });
    // Cap list length so Free never dumps 50 messy rows.
    cleaned.truncate(12);

    let list = cleaned
        .iter()
        .enumerate()
        .map(|(index, stream)| {
            // Raw byte size string so details UI can format GB/MB like MovieBox.
            let size_bytes = stream
                .size_bytes
                .map(|b| b.to_string())
                .unwrap_or_else(|| "0".into());
            let duration = stream.duration_secs.unwrap_or(0);
            serde_json::json!({
                "resourceId": format!("free-{}", index),
                "resourceLink": stream.url,
                // Short structured title for any secondary UI.
                "title": stream.display_title(),
                "fileName": stream.filename,
                "size": size_bytes,
                "resolution": stream.resolution.unwrap_or(0),
                "codecName": stream.codec,
                "language": stream.audio,
                "uploadBy": stream.source_label(),
                "duration": duration,
                "se": 0,
                "ep": 0,
                "hasResource": true,
                "_free_stream": true,
            })
        })
        .collect::<Vec<_>>();
    serde_json::Value::Array(list)
}

pub fn playback(url: impl Into<String>, label: impl Into<String>) -> PlaybackSource {
    PlaybackSource {
        provider: ProviderKind::Free,
        url: url.into(),
        headers: vec![("User-Agent".into(), "MovieBox-Tui/0.1".into())],
        subtitle: None,
        source_label: label.into(),
    }
}

#[allow(dead_code)]
pub fn catalog_id(value: impl Into<String>) -> ProviderMediaId {
    ProviderMediaId {
        provider: ProviderKind::Free,
        value: value.into(),
    }
}

#[allow(dead_code)]
pub fn empty_seasons() -> Vec<Season> {
    Vec::new()
}
