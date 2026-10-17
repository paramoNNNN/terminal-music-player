# MacBook continuation handoff

Updated: 2026-10-12. Read this and `MVP.md` before working on this checkout.

## Immediate task

Get the existing MVP built and running in the MacBook's terminal, test native
audio and navigation, fix any macOS blockers, and record the results. Continue
with the existing implementation; the source audit and extend-versus-new
decision have already been made. Do not restart with a new player or rewrite.

The user wants terminal use on their NixOS machine and MacBook. Windows, mobile,
a graphical interface and general-purpose installers are outside scope.

## Repository and implementation

- GitHub repo: https://github.com/paramoNNNN/terminal-music-player
- Working branch: `master`. The initial MVP commit is `4464619`.
- The user authorized publishing this repository as a public fork on 2026-10-15,
  with an explicit AI-modification notice. Never push to bergmayer's upstream repo. A fresh clone may not have an `upstream` remote; one is not
  needed to build or test.
- Based on textamp commit `0d794c7a207992a95ad3ff2880e5c0e8713fa9db`, Unlicense.
- Rust, Ratatui/Crossterm, Tokio, native Rodio/CPAL audio. This implementation
  does not use mpv; the original proposed mpv architecture was not selected.
- Added `flake.nix`, `flake.lock`, `run`, `MVP.md`, and NixOS test portability
  fixes. Existing search, library browsing, playlists, queues and playback come
  from textamp. See the architecture rules in `AGENTS.md`.
- The NixOS development checkout was `/home/taha/projects/terminal-music-player`.
  Use the actual checkout path on the Mac; do not copy Linux paths or binaries.

## Build and launch on the Mac

1. Inspect `git status`, architecture (`uname -m`), macOS version (`sw_vers`),
   and available tools (`command -v nix cargo rustc`). Preserve unrelated changes.
2. Prefer the pinned Nix environment if Nix is already installed:

   ```sh
   nix develop
   cargo build --locked --release --bin textamp
   ./target/release/textamp
   ```

   Without Nix, use the installed Rust toolchain and Xcode command line tools,
   then the same Cargo build command. Inspect missing dependencies and fix the
   concrete build issue. Do not replace a working toolchain or change system
   configuration unnecessarily. If prerequisites require user interaction,
   explain the exact missing prerequisite.
3. `./run` builds on first launch using Nix if available, otherwise Cargo.
   It reuses an existing executable, so explicitly rebuild after source changes
   or pulling runtime changes. A Linux executable cannot be used on macOS.
4. `nix run .` is an alternative package build, but only its Linux build plan
   was checked; completing a full Nix package build is still outstanding.

## Connection and keyboard setup

- Navidrome URL: `https://navi.paranas.ir`.
- Press **`,` (comma)** to open Settings → Libraries → Add library. The user
  reported that F2 controls brightness. Do not make changing keyboard settings
  a prerequisite; comma works, and Fn+F2 is another option.
- Enter the username/password locally in the TUI. Do not ask the user to send
  credentials in chat, place them in shell arguments, or print saved credentials.
- Use Ctrl+F for search, arrows/Enter to browse, Space for pause/resume,
  `<` / `>` for previous/next, Shift+Left/Right for seeking, and Ctrl+Q to quit.
- Native macOS config/private credentials/logs default to
  `~/Library/Application Support/textamp`; caches to `~/Library/Caches/textamp`.
  Respect XDG overrides. Do not overwrite personal configuration for tests.

## Validation to perform

Start with build, isolated automated checks, and a silent native audio fixture.
In the development shell, use a subshell to keep test paths out of normal app
launches:

```sh
(
  test_root=$(mktemp -d)
  export XDG_CONFIG_HOME="$test_root/config"
  export XDG_DATA_HOME="$test_root/data"
  export XDG_CACHE_HOME="$test_root/cache"
  export XDG_STATE_HOME="$test_root/state"
  cargo test --locked --lib
)
cargo fmt --all -- --check
cargo run --locked --release --example audio_smoke -- --stream
```

The audio example deliberately opens the native output device and streams silent
generated audio from a loopback fixture. It checks pause-preserving forward seek,
backward seek, completion and cancellation. Do not use `--audible` unless asked.
Python 3 is needed for Linux AirPlay unit fixtures; the Nix shell supplies it.

For runtime functionality changes, run the full checks required by `AGENTS.md`,
including `cargo test` and Clippy. The previous full release integration build
was stopped because LTO made each separate test executable expensive; prefer
normal test/dev builds for routine verification. Do not claim the full suite
passed based on the focused unit run.

Launch the terminal UI on the Mac, verify comma opens Settings, and check clean
exit/terminal restoration. After the user signs in locally, test search,
artist/album navigation, playlist browsing, queue operations and an original
audio stream. Check paused/resumed playback, skips, seeks, volume and recovery
after a failed stream. Audible listening and personal playback/scrobbling should
be deliberate user tests; automated mutation tests belong on the loopback fixture.
Do not run playlist/favorite/rating/history mutation tests against the real server.

If login is unavailable, complete build, fixture audio and UI work, then report
authenticated server checks as pending. An existing read-only diagnostic is
`cargo run --example navidrome_check`, using saved credentials; inspect it before
running and use explicit opt-in for any native playback check. Do not use its
`--add` stdin password path when TUI setup is available.

Record actual macOS version/architecture, terminal, commands, failures/fixes and
remaining limitations in `MVP.md`. A successful cross-build or flake evaluation
does not establish native playback support.

## What was verified on NixOS

- Linux release build succeeded without warnings; UI startup, Settings and clean
  exit worked with isolated XDG state.
- Silent native HTTP fixture streaming passed forward/backward seek, paused
  seek, completion and cancellation.
- Unit suite: 196 passed, 0 failed, 1 ignored live Wikimedia test.
- Formatting, whitespace and launcher shell syntax checks passed.
- Linux and Apple Silicon macOS Nix definitions evaluated; Linux package dry run
  passed. Full Nix package build, full integration suite and Clippy were not
  completed.
- An unauthenticated read-only API probe returned Navidrome 0.64.2, protocol
  1.16.1, OpenSubsonic enabled, and the expected missing-username error.
- Authenticated use of the user's server, its optional extensions, transcoding,
  metadata completeness and all native Mac behavior were not verified.

## Server constraints and later feature work

The user already has Navidrome and a working remote AudioMuse-AI service. Reuse
them. Do not deploy analyzers, trigger scans, or change server configuration.
Direct textamp AudioMuse integration expects `/api/sync`; verify compatibility
with the deployed service. Navidrome's advertised `sonicSimilarity` extension is
another possible route. Do not assume either capability from the version alone.

The NixOS server configuration uses AudioMuse plugin v10 and agent order
`audiomuseai,lastfm,listenbrainz`. Provider priority does not offer an independent
per-request metadata recommendation switch. In particular, `getSimilarSongs2`
may return AudioMuse results and must not be mislabeled as Metadata Similarity.

After Mac MVP validation, the original requested features remain:

1. One search field covering tracks, artists, albums and playlists. Verify the
   existing categories against the real library before claiming completeness.
2. Rich artist/album pages with biography, top tracks, discography, related music
   and available notes. Show missing-data states and explain ranking sources;
   there is no universal critical-ranking endpoint for "best albums."
3. A discovery home with recently added albums, playlists, daily suggestions,
   recently played artists and persistent history. Label client-only history;
   daily suggestions should remain stable for the user's local day.
4. Every track/artist/album radio action must offer explicit **Sonic Similarity**
   and **Metadata Similarity**, using independent providers. Display the mode
   and actual source, retain that mode through refill, and preserve the old queue
   on failure/empty results. Never silently switch modes. Distinguish album-seeded
   track mixes from complete-album radio. This selector is not implemented yet.
5. Verify original and transcoded playback/seek behavior against advertised server
   capabilities. Native controls now include macOS integration and Linux MPRIS;
   see the latest `MVP.md` verification record for tested behavior and remaining
   manual desktop checks. Keep native callbacks outside the app reducers.

Prioritize a working, tested Mac MVP before expanding this backlog. Report what
was actually tested. Do not claim Plexamp parity. Keep application work in this
repository, preserve credentials and unrelated changes, and follow the current
user's directions about committing and pushing subsequent work.
