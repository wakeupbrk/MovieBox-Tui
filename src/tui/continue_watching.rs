//! Continue Watching — track in-progress movies/episodes and resume at the exact second.

use crate::providers::models::ProviderKind;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const MAX_ENTRIES: usize = 40;
/// Drop an entry once the user is this close to the end (when duration is known).
const NEAR_END_RATIO: f64 = 0.92;
/// Ignore tiny positions so accidental opens don't clutter the list.
const MIN_SAVE_SECS: f64 = 5.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WatchKind {
    Movie,
    Series,
    Local,
    LiveTv,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WatchEntry {
    /// Stable id so the same show/episode updates in place.
    pub key: String,
    pub title: String,
    /// e.g. "S02E03", "Movie", "Library", channel group
    pub detail: String,
    pub kind: WatchKind,
    #[serde(default)]
    pub provider: Option<ProviderKind>,
    #[serde(default)]
    pub subject_id: Option<String>,
    #[serde(default)]
    pub season: Option<usize>,
    #[serde(default)]
    pub episode: Option<usize>,
    /// Playback position in seconds.
    pub position_secs: f64,
    #[serde(default)]
    pub duration_secs: Option<f64>,
    /// Last stream URL or local file path (best-effort resume).
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub headers: Vec<(String, String)>,
    #[serde(default)]
    pub subtitle: Option<String>,
    #[serde(default)]
    pub is_local: bool,
    pub last_watched_unix: u64,
}

impl WatchEntry {
    pub fn position_label(&self) -> String {
        format_hms(self.position_secs)
    }

    pub fn progress_label(&self) -> String {
        match self.duration_secs.filter(|d| *d > 1.0) {
            Some(dur) => {
                let pct = ((self.position_secs / dur) * 100.0).clamp(0.0, 100.0) as u32;
                format!("{} / {} ({}%)", format_hms(self.position_secs), format_hms(dur), pct)
            }
            None => format_hms(self.position_secs),
        }
    }

    pub fn kind_label(&self) -> &'static str {
        match self.kind {
            WatchKind::Movie => "Movie",
            WatchKind::Series => "Series",
            WatchKind::Local => "Local",
            WatchKind::LiveTv => "Live TV",
        }
    }

    pub fn is_near_end(&self) -> bool {
        match self.duration_secs {
            Some(dur) if dur > 30.0 => self.position_secs >= dur * NEAR_END_RATIO,
            _ => false,
        }
    }

    pub fn display_line(&self) -> String {
        if self.detail.is_empty() {
            self.title.clone()
        } else {
            format!("{} — {}", self.title, self.detail)
        }
    }
}

pub fn store_path() -> PathBuf {
    let mut path = dirs::config_dir().unwrap_or_else(|| PathBuf::from("."));
    path.push("moviebox-tui");
    path.push("continue_watching.json");
    path
}

pub fn watch_later_root() -> PathBuf {
    let mut path = dirs::cache_dir().unwrap_or_else(std::env::temp_dir);
    path.push("moviebox-tui");
    path.push("watch-later");
    path
}

/// Per-entry directory so mpv's watch-later files don't collide.
pub fn watch_later_dir_for(key: &str) -> PathBuf {
    let safe: String = key
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    let truncated = if safe.len() > 80 { &safe[..80] } else { &safe };
    watch_later_root().join(truncated)
}

pub fn load() -> Vec<WatchEntry> {
    let path = store_path();
    let Ok(raw) = fs::read_to_string(&path) else {
        return Vec::new();
    };
    let Ok(mut entries) = serde_json::from_str::<Vec<WatchEntry>>(&raw) else {
        return Vec::new();
    };
    entries.sort_by(|a, b| b.last_watched_unix.cmp(&a.last_watched_unix));
    entries
}

pub fn save(entries: &[WatchEntry]) {
    let path = store_path();
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let Ok(json) = serde_json::to_vec_pretty(entries) else {
        return;
    };
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let temporary = path.with_extension(format!("tmp-{}-{stamp}", std::process::id()));
    if fs::write(&temporary, json).is_err() {
        return;
    }
    if fs::rename(&temporary, &path).is_err() {
        let _ = fs::remove_file(&path);
        if fs::rename(&temporary, &path).is_err() {
            let _ = fs::remove_file(temporary);
        }
    }
}

/// Insert or update an entry (moves it to the front). Drops near-complete items.
pub fn upsert(entries: &mut Vec<WatchEntry>, entry: WatchEntry) {
    entries.retain(|e| e.key != entry.key);
    if entry.is_near_end() {
        // Finished — remove any prior record too.
        save(entries);
        return;
    }
    // Keep brand-new opens even below MIN_SAVE_SECS so the row appears immediately;
    // only skip re-saving tiny positions when we already have a better one.
    entries.insert(0, entry);
    if entries.len() > MAX_ENTRIES {
        entries.truncate(MAX_ENTRIES);
    }
    save(entries);
}

/// Update position after the player quits. Removes near-end entries.
pub fn update_position(
    entries: &mut Vec<WatchEntry>,
    key: &str,
    position_secs: f64,
    duration_secs: Option<f64>,
) {
    let Some(idx) = entries.iter().position(|e| e.key == key) else {
        return;
    };
    if position_secs < MIN_SAVE_SECS {
        // User quit almost immediately — leave prior progress if any, else drop.
        if entries[idx].position_secs < MIN_SAVE_SECS {
            entries.remove(idx);
            save(entries);
        }
        return;
    }
    entries[idx].position_secs = position_secs;
    if duration_secs.is_some() {
        entries[idx].duration_secs = duration_secs;
    }
    entries[idx].last_watched_unix = now_unix();
    if entries[idx].is_near_end() {
        entries.remove(idx);
        save(entries);
        return;
    }
    // Move to front
    let entry = entries.remove(idx);
    entries.insert(0, entry);
    save(entries);
}

pub fn remove(entries: &mut Vec<WatchEntry>, key: &str) {
    let before = entries.len();
    entries.retain(|e| e.key != key);
    if entries.len() != before {
        save(entries);
    }
}

pub fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

pub fn make_key(
    kind: WatchKind,
    provider: Option<ProviderKind>,
    subject_id: Option<&str>,
    season: Option<usize>,
    episode: Option<usize>,
    url: Option<&str>,
) -> String {
    match kind {
        WatchKind::Series => {
            let p = provider.map(|p| p.cache_key()).unwrap_or("unknown");
            let id = subject_id.unwrap_or("unknown");
            let s = season.unwrap_or(0);
            let e = episode.unwrap_or(0);
            format!("{p}:{id}:s{s:02}e{e:02}")
        }
        WatchKind::Movie => {
            let p = provider.map(|p| p.cache_key()).unwrap_or("unknown");
            let id = subject_id.unwrap_or("unknown");
            format!("{p}:{id}:movie")
        }
        WatchKind::Local => {
            let path = url.unwrap_or("local");
            format!("local:{}", path)
        }
        WatchKind::LiveTv => {
            let path = url.unwrap_or("live");
            format!("live:{}", path)
        }
    }
}

/// Read the highest `start=` value from mpv watch-later configs in `dir`.
pub fn read_watch_later_position(dir: &Path) -> Option<f64> {
    let entries = fs::read_dir(dir).ok()?;
    let mut best: Option<f64> = None;
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let Ok(text) = fs::read_to_string(&path) else {
            continue;
        };
        for line in text.lines() {
            let line = line.trim();
            if let Some(rest) = line.strip_prefix("start=") {
                if let Ok(secs) = rest.trim().parse::<f64>() {
                    if secs.is_finite() && secs >= 0.0 {
                        best = Some(best.map_or(secs, |b: f64| b.max(secs)));
                    }
                }
            }
        }
    }
    best
}

pub fn format_hms(secs: f64) -> String {
    let total = secs.max(0.0).floor() as u64;
    let h = total / 3600;
    let m = (total % 3600) / 60;
    let s = total % 60;
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m}:{s:02}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_hms_basic() {
        assert_eq!(format_hms(0.0), "0:00");
        assert_eq!(format_hms(65.0), "1:05");
        assert_eq!(format_hms(3723.0), "1:02:03");
    }

    #[test]
    fn near_end_detection() {
        let mut e = WatchEntry {
            key: "k".into(),
            title: "t".into(),
            detail: String::new(),
            kind: WatchKind::Movie,
            provider: None,
            subject_id: None,
            season: None,
            episode: None,
            position_secs: 56.0 * 60.0,
            duration_secs: Some(60.0 * 60.0),
            url: None,
            headers: vec![],
            subtitle: None,
            is_local: false,
            last_watched_unix: 0,
        };
        assert!(e.is_near_end());
        e.position_secs = 10.0 * 60.0;
        assert!(!e.is_near_end());
    }
}
