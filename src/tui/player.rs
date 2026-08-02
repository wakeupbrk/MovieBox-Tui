use crate::tui::state::PlayerKind;
use std::{
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
};

const MPV_WINDOWS: &str = r"C:\Program Files\mpv\mpv.exe";
const MPV_MACOS: &str = "/Applications/mpv.app/Contents/MacOS/mpv";
const VLC_WINDOWS: &str = r"C:\Program Files\VideoLAN\VLC\vlc.exe";
const VLC_WINDOWS_X86: &str = r"C:\Program Files (x86)\VideoLAN\VLC\vlc.exe";
const VLC_MACOS: &str = "/Applications/VLC.app/Contents/MacOS/VLC";

/// Options for launching an external player with optional resume + position tracking.
#[derive(Debug, Clone, Default)]
pub struct PlayOptions {
    pub url: String,
    pub subtitle: Option<String>,
    pub headers: Vec<(String, String)>,
    /// Seek here on open (seconds).
    pub start_secs: Option<f64>,
    /// mpv writes watch-later configs here when the user quits (for exact resume).
    pub watch_later_dir: Option<PathBuf>,
    pub media_title: Option<String>,
}

pub fn detect() -> Vec<PlayerKind> {
    let mut players = Vec::new();

    // Prefer mpv first — it handles signed/headered stream URLs most reliably.
    if mpv_executable().is_some() {
        players.push(PlayerKind::Mpv);
    }

    #[cfg(target_os = "macos")]
    if Path::new("/Applications/IINA.app").exists() || command_exists("iina") {
        players.push(PlayerKind::Iina);
    }

    if vlc_executable().is_some() {
        players.push(PlayerKind::Vlc);
    }

    players
}

/// Index to pre-select in the player picker (mpv when available).
pub fn preferred_index(players: &[PlayerKind]) -> usize {
    players
        .iter()
        .position(|p| *p == PlayerKind::Mpv)
        .unwrap_or(0)
}

pub fn supports_headers(kind: PlayerKind, headers: &[(String, String)]) -> bool {
    kind != PlayerKind::Vlc
        || headers.iter().all(|(name, _)| {
            name.eq_ignore_ascii_case("referer") || name.eq_ignore_ascii_case("user-agent")
        })
}

/// Whether this player can reliably report quit position via watch-later.
pub fn tracks_position(kind: PlayerKind) -> bool {
    matches!(kind, PlayerKind::Mpv | PlayerKind::Iina)
}

pub fn command(kind: PlayerKind, opts: &PlayOptions) -> Command {
    match kind {
        PlayerKind::Mpv => mpv_command(opts, false),
        PlayerKind::Iina => iina_command(opts),
        PlayerKind::Vlc => vlc_command(opts),
    }
}

/// Launch the player as a child process we can wait on (for position capture).
pub fn spawn(kind: PlayerKind, opts: &PlayOptions) -> std::io::Result<Child> {
    if let Some(dir) = &opts.watch_later_dir {
        let _ = std::fs::create_dir_all(dir);
    }
    let mut cmd = command(kind, opts);
    cmd.stdout(Stdio::null());
    cmd.stderr(Stdio::null());
    // Own process group so Ctrl+C in the TUI does not kill the player,
    // while we still retain the Child handle to wait for quit + read position.
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }
    cmd.spawn()
}

fn mpv_command(opts: &PlayOptions, iina_prefix: bool) -> Command {
    let executable = mpv_executable().unwrap_or_else(|| "mpv".into());
    let mut command = Command::new(executable);
    let prefix = if iina_prefix { "--mpv-" } else { "--" };

    command
        .arg(format!("{prefix}autofit=960x540"))
        .arg(format!("{prefix}autofit-larger=640x360"))
        .arg(format!("{prefix}geometry=50%:50%"));

    if let Some(title) = &opts.media_title {
        command.arg(format!("{prefix}force-media-title={title}"));
    }

    if let Some(start) = opts.start_secs.filter(|s| *s > 0.5) {
        // mpv accepts seconds as a float string
        command.arg(format!("{prefix}start={start}"));
    }

    if let Some(dir) = &opts.watch_later_dir {
        let dir_str = dir.to_string_lossy();
        command.arg(format!("{prefix}save-position-on-quit=yes"));
        command.arg(format!("{prefix}watch-later-directory={dir_str}"));
        command.arg(format!("{prefix}write-filename-in-watch-later-config=yes"));
        // Only persist seek position (keeps files tiny / easy to parse).
        command.arg(format!("{prefix}watch-later-options=start"));
    }

    command.arg(&opts.url);

    if !opts.headers.is_empty() {
        let fields = opts
            .headers
            .iter()
            .map(|(name, value)| format!("{name}: {value}"))
            .collect::<Vec<_>>()
            .join(",");
        command.arg(format!("{prefix}http-header-fields={fields}"));
    }
    if let Some(subtitle) = &opts.subtitle {
        if iina_prefix {
            command.arg(format!("--mpv-sub-files={subtitle}"));
        } else {
            command.arg(format!("--sub-file={subtitle}"));
        }
    }

    command
}

#[cfg(target_os = "macos")]
fn iina_command(opts: &PlayOptions) -> Command {
    // Prefer the `iina` CLI so we get a real child we can wait on.
    // `open -a IINA` returns immediately and breaks position tracking.
    if command_exists("iina") {
        let mut command = Command::new("iina");
        command.arg("--no-stdin");
        if let Some(title) = &opts.media_title {
            command.arg(format!("--mpv-force-media-title={title}"));
        }
        if let Some(start) = opts.start_secs.filter(|s| *s > 0.5) {
            command.arg(format!("--mpv-start={start}"));
        }
        if let Some(dir) = &opts.watch_later_dir {
            let dir_str = dir.to_string_lossy();
            let _ = std::fs::create_dir_all(dir);
            command.arg("--mpv-save-position-on-quit=yes");
            command.arg(format!("--mpv-watch-later-directory={dir_str}"));
            command.arg("--mpv-write-filename-in-watch-later-config=yes");
            command.arg("--mpv-watch-later-options=start");
        }
        if !opts.headers.is_empty() {
            let fields = opts
                .headers
                .iter()
                .map(|(name, value)| format!("{name}: {value}"))
                .collect::<Vec<_>>()
                .join(",");
            command.arg(format!("--mpv-http-header-fields={fields}"));
        }
        if let Some(subtitle) = &opts.subtitle {
            command.arg(format!("--mpv-sub-files={subtitle}"));
        }
        command.arg(&opts.url);
        return command;
    }

    // Fallback: open -a (cannot track quit position).
    let mut command = Command::new("open");
    command.arg("-a").arg("IINA").arg("--args");
    let mpv = mpv_command(opts, true);
    command.args(mpv.get_args());
    command
}

#[cfg(not(target_os = "macos"))]
fn iina_command(opts: &PlayOptions) -> Command {
    mpv_command(opts, false)
}

fn vlc_command(opts: &PlayOptions) -> Command {
    let executable = vlc_executable().unwrap_or_else(|| "vlc".into());
    let mut command = Command::new(executable);
    command.arg("--width=960").arg("--height=540");

    if let Some(start) = opts.start_secs.filter(|s| *s > 0.5) {
        // VLC start-time is integer seconds
        command.arg(format!("--start-time={}", start.floor() as u64));
    }

    command.arg(&opts.url);

    for (name, value) in &opts.headers {
        if name.eq_ignore_ascii_case("referer") {
            command.arg(format!("--http-referrer={value}"));
        } else if name.eq_ignore_ascii_case("user-agent") {
            command.arg(format!("--http-user-agent={value}"));
        }
    }
    if let Some(subtitle) = &opts.subtitle {
        command.arg(format!("--sub-file={subtitle}"));
    }

    command
}

fn mpv_executable() -> Option<String> {
    first_executable(&[MPV_WINDOWS, MPV_MACOS], "mpv")
}

fn vlc_executable() -> Option<String> {
    first_executable(&[VLC_WINDOWS, VLC_WINDOWS_X86, VLC_MACOS], "vlc")
}

fn first_executable(paths: &[&str], fallback: &str) -> Option<String> {
    paths
        .iter()
        .find(|path| Path::new(path).exists())
        .map(|path| (*path).to_string())
        .or_else(|| command_exists(fallback).then(|| fallback.to_string()))
}

fn command_exists(command: &str) -> bool {
    let finder = if cfg!(target_os = "windows") {
        "where"
    } else {
        "which"
    };
    Command::new(finder)
        .arg(command)
        .output()
        .is_ok_and(|output| output.status.success())
}
