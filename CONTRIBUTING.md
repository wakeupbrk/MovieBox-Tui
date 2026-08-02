# Contributing to MovieBox-Tui

Thanks for contributing. Bug reports, ideas, docs, and PRs are welcome on this fork:
**[wakeupbrk/MovieBox-Tui](https://github.com/wakeupbrk/MovieBox-Tui)**.

For large or breaking changes, open an issue first.

## Getting set up

Rust **1.85+** (edition 2024) via [rustup.rs](https://rustup.rs/). Player: **mpv** recommended.

```bash
git clone https://github.com/wakeupbrk/MovieBox-Tui.git
cd MovieBox-Tui

# Optional: track original upstream
git remote add upstream https://github.com/mesamirh/MovieBox-Tui.git

cargo run --release
```

## Project layout

- `src/providers/moviebox/` — MovieBox API client  
- `src/providers/fourkhdhub/` — 4KHDHub  
- `src/providers/free/` — Free (Cinemeta + Archive.org + OpenSubtitles)  
- `src/tui/` — Ratatui UI  
- `src/tui/app.rs` — central event loop (`Action` match)  
- `src/tui/continue_watching.rs` — resume state  

Message-driven: prefer a new `Action` over blocking the UI thread.

## Workflow

1. Branch off `main`:

   ```bash
   git checkout main
   git pull origin main
   git checkout -b feat/short-description
   ```

2. Make your change in small, logical commits.
3. Run the checks below.
4. Push and open a pull request against `main`.

## Before opening a PR

Run these locally. All must pass.

```bash
cargo fmt --all
cargo clippy --all-targets --all-features -- -D warnings
cargo build
```

Guidelines:

- Follow idiomatic Rust and standard `rustfmt` defaults. Don't hand-format.
- Keep the async, message-passing architecture intact.
- Avoid panics on paths that handle network or user input.
- Don't add new dependencies without a good reason. Mention it in the PR if you do.

## Commit messages

Follow [Conventional Commits](https://www.conventionalcommits.org/). Keep the subject concise and in the imperative mood.

Examples:

```
feat: add support for custom mpv arguments
fix: prevent panic when clipboard is unavailable
docs: document /anime discover command
refactor: extract stream resolution into helper
```

Common types: `feat`, `fix`, `refactor`, `docs`, `style`, `perf`, `chore`.

## Pull requests

Keep PRs focused on a single concern. Large PRs mixing unrelated changes may be asked to be split.

In your PR description, explain what changed and why. Link related issues (`Closes #12`) and include screenshots or recordings for anything visible in the UI.

Never commit `target/`, editor settings, or debug dump files.

## License

By contributing, you agree that your contributions will be dual-licensed under the [MIT](LICENSE-MIT) and [Apache-2.0](LICENSE-APACHE) licenses, consistent with the rest of the project.
