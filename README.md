<div align="center">

# MovieBox-TUI

**Stream movies, shows, anime, and live TV from your terminal.**

No torrents. No debrid keys. No browser tabs — just a fast TUI and your local player.

[![License](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](#license)
[![Rust](https://img.shields.io/badge/rust-1.85%2B-orange.svg?logo=rust)](#requirements)
[![GitHub](https://img.shields.io/badge/github-wakeupbrk%2FMovieBox--Tui-181717.svg?logo=github)](https://github.com/wakeupbrk/MovieBox-Tui)

<br>

<img src="https://raw.githubusercontent.com/wakeupbrk/MovieBox-Tui/main/assets/screenshots/01-home-blocky.jpg" alt="MovieBox-TUI Home" width="85%">

<sub>
Fork of <a href="https://github.com/mesamirh/MovieBox-Tui">mesamirh/MovieBox-Tui</a> · maintained by <a href="https://github.com/wakeupbrk">@wakeupbrk</a>
</sub>

</div>

---

## Why this fork?

The upstream app is great. This fork adds **free multi-source playback**, **continue watching**, and smoother day-to-day use — built for sharing with friends.

| | Upstream | **This fork** |
|---|:---:|:---:|
| MovieBox catalog | ✅ | ✅ |
| 4KHDHub | ✅ | ✅ |
| **Free** (Archive.org + Cinemeta) | — | ✅ |
| Search **All** providers (ranked) | — | ✅ |
| **Continue Watching** (`Ctrl+W`) | — | ✅ |
| Library on `Ctrl+Z` | varies | ✅ |
| Free OpenSubtitles picker | — | ✅ |
| Clean Free quality / codec / size UI | — | ✅ |

**Share this link:** [github.com/wakeupbrk/MovieBox-Tui](https://github.com/wakeupbrk/MovieBox-Tui)

---

## Quick install (macOS / Linux)

**You need:** [Rust](https://rustup.rs) (1.85+) and a player — **mpv** recommended (also IINA / VLC).

```bash
# Rust (skip if you already have cargo)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source "$HOME/.cargo/env"

# Player (macOS)
brew install mpv

# This fork
cargo install --git https://github.com/wakeupbrk/MovieBox-Tui.git --locked

# Launch
moviebox-tui
```

### Already have the original app?

```bash
brew uninstall moviebox-tui 2>/dev/null
cargo uninstall moviebox-tui 2>/dev/null
cargo install --git https://github.com/wakeupbrk/MovieBox-Tui.git --locked --force
which moviebox-tui   # expect ~/.cargo/bin/moviebox-tui
```

### Update

```bash
cargo install --git https://github.com/wakeupbrk/MovieBox-Tui.git --locked --force
```

### One-liner (same as above)

```bash
curl -fsSL https://raw.githubusercontent.com/wakeupbrk/MovieBox-Tui/main/install.sh | bash
```

<details>
<summary><b>Windows</b></summary>

1. Install [Rust](https://rustup.rs) and a player (mpv or VLC).
2. In PowerShell:

```powershell
cargo install --git https://github.com/wakeupbrk/MovieBox-Tui.git --locked
moviebox-tui
```

</details>

<details>
<summary><b>Uninstall</b></summary>

```bash
cargo uninstall moviebox-tui
```

</details>

---

## Features

### Sources
- **MovieBox** — main catalog & streams  
- **4KHDHub** — extra quality options  
- **Free** — Internet Archive + metadata (no paid key)  
- **All** (`Ctrl+P`) — search every source; results ordered **MovieBox → 4KHDHub → Free**

### Playback
- Local players: **mpv**, IINA, VLC  
- Subtitle language picker (MovieBox + Free via OpenSubtitles)  
- **Continue Watching** — resume the same episode at the minute you left off  
- **Library** — downloaded files (`Ctrl+Z`)

### Terminal UX
- Instant search + slash catalogs (`/movies`, `/shows`, `/anime`, …)  
- Poster images in supported terminals  
- Themes (3D block / ASCII)  
- Live IPTV (`Ctrl+T`) via [iptv-org](https://github.com/iptv-org/iptv)  
- Season downloads with resume

---

## Keyboard map

| Key | Action |
|-----|--------|
| Type | Search |
| `↑` `↓` `←` `→` | Navigate |
| `Enter` | Details / play |
| `o` | Change player |
| `d` | Download episode / season |
| `Ctrl`+`P` | Provider: MovieBox · 4KHDHub · Free · **All** |
| `Ctrl`+`W` | **Continue Watching** |
| `Ctrl`+`Z` | **Library** (downloads) |
| `Ctrl`+`T` | Live TV |
| `?` | Help |
| `Esc` / `q` | Back / quit |

### Slash commands

| Command | What it does |
|---------|----------------|
| `/discover` · `/home` | Trending |
| `/movies` · `/shows` · `/anime` | Category browse |
| `/list` | Live TV channels |
| `/config` | IPTV playlist config |
| `/update` | Check for updates |
| `/toggle-update` | Auto-update check on/off |

---

## Screenshots

<details>
<summary><b>Details · Search · Playback</b></summary><br>
<p align="center">
  <img src="https://raw.githubusercontent.com/wakeupbrk/MovieBox-Tui/main/assets/screenshots/07-movie-details.jpg" alt="Movie Details" width="49%">
  <img src="https://raw.githubusercontent.com/wakeupbrk/MovieBox-Tui/main/assets/screenshots/08-series-details.jpg" alt="Series Details" width="49%">
</p>
<p align="center">
  <img src="https://raw.githubusercontent.com/wakeupbrk/MovieBox-Tui/main/assets/screenshots/06-search-results.jpg" alt="Search" width="49%">
  <img src="https://raw.githubusercontent.com/wakeupbrk/MovieBox-Tui/main/assets/screenshots/11-player-picker.jpg" alt="Player picker" width="49%">
</p>
</details>

<details>
<summary><b>Live TV · Themes · Help</b></summary><br>
<p align="center">
  <img src="https://raw.githubusercontent.com/wakeupbrk/MovieBox-Tui/main/assets/screenshots/09-live-tv-list.jpg" alt="Live TV" width="49%">
  <img src="https://raw.githubusercontent.com/wakeupbrk/MovieBox-Tui/main/assets/screenshots/03-home-3d.jpg" alt="3D theme" width="49%">
</p>
<p align="center">
  <img src="https://raw.githubusercontent.com/wakeupbrk/MovieBox-Tui/main/assets/screenshots/04-global-help.jpg" alt="Help" width="85%">
</p>
</details>

---

## Tips for friends

1. Prefer **mpv** for reliable resume with Continue Watching.  
2. Use **`Ctrl+P` → All** if one source is empty.  
3. Titles that don’t play on MovieBox often still work on **Free**.  
4. Free streams: pick a row with real **size + codec** (not a stale cache — re-open details if the list looks wrong).  
5. First Free play can take a few seconds while Archive indexes load.

Config / continue-watching data lives under:

```text
~/.config/moviebox-tui/
```

---

## Development

```bash
git clone https://github.com/wakeupbrk/MovieBox-Tui.git
cd MovieBox-Tui
cargo run --release
```

Checks before a PR:

```bash
cargo fmt --all
cargo clippy --all-targets --all-features -- -D warnings
cargo build
```

See [CONTRIBUTING.md](CONTRIBUTING.md). Prefer [Conventional Commits](https://www.conventionalcommits.org/).

---

## Credits & legal

- Original project: [mesamirh/MovieBox-Tui](https://github.com/mesamirh/MovieBox-Tui)  
- Live TV lists: [iptv-org/iptv](https://github.com/iptv-org/iptv)  
- Free catalog metadata: [Cinemeta](https://github.com/Stremio/stremio-cinemeta) · streams from the Internet Archive  

> **Disclaimer:** Third-party client. Does not host media; resolves links from public APIs and sites. Personal use only. Not affiliated with MovieBox, Archive.org, or any content operator.

---

<div align="center">

Dual-licensed under [MIT](LICENSE-MIT) or [Apache 2.0](LICENSE-APACHE).<br>
Fork maintained by [**@wakeupbrk**](https://github.com/wakeupbrk) · original by [**@mesamirh**](https://github.com/mesamirh)

⭐ Star the repo if it helps — easiest way to support.

</div>
