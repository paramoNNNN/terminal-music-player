# Terminal music MVP

This project extends textamp at upstream commit
`0d794c7a207992a95ad3ff2880e5c0e8713fa9db` (Unlicense).
The target is terminal use on the user's NixOS machine and MacBook.
No server deployment or AudioMuse analysis job is needed.

## Run on NixOS or a Mac with Nix

Run `./run` from this directory. It builds on first launch using Nix when
available, then launches the terminal player. To rebuild after source changes:

```sh
nix develop
cargo build --locked --release --bin textamp
./target/release/textamp
```

Alternatively, `nix run .` builds the packaged application. Its build plan has
been evaluated; the complete Nix package build has not been run. The flake supports
Intel and Apple Silicon Macs, plus x86-64 and ARM Linux. Native macOS validation
is still required; declaring a system in the flake does not establish support.

On a Mac without Nix, install Rust and the Xcode command line tools, then run
`cargo build --locked --release --bin textamp`. Playback uses the application's
native audio backend; mpv is not required.

## Connect and use

1. Launch in a terminal on the machine where you want sound.
2. Press `,` (comma) or F2 → Libraries → Add library, choose Navidrome, and enter `https://navi.paranas.ir`,
   username and password locally. Comma avoids brightness/function-key conflicts; Fn+F2 also works.
   Credentials are stored in separate private files by textamp.
3. Use Ctrl+F to search, arrows and Enter to browse/select, Space to pause,
   `<` / `>` for previous/next, and Shift+Left/Right to seek.
4. F1 shows help; `:` opens the command palette; Ctrl+Q quits.
5. For AudioMuse, configure the existing service through the library's Analysis
   page only if needed. Direct integration expects `/api/sync`; compatibility
   with the user's deployed service must be checked. The Navidrome sonic
   extension provides another analysis path when advertised by the server.

## MVP boundary

The existing implementation provides server connection, library search,
artist/album browsing, playlists, queue, local playback, recently added/played
lists, biographies and optional sonic discovery. This is an MVP base, not full
acceptance of the original handover. Live functionality must be checked against
the user's server after local sign-in.

The requested explicit Sonic Similarity / Metadata Similarity choice for every
track, artist and album seed is not yet implemented. In particular,
`getSimilarSongs2` may use AudioMuse under the user's agent configuration, so its
results must not be presented as an independently verified metadata provider.
Daily recommendations, complete cross-client history and OS media controls are
not acceptance claims for this MVP.

## Source audit and decision

| Candidate | Pinned revision | Findings |
| --- | --- | --- |
| textamp | `0d794c7a207992a95ad3ff2880e5c0e8713fa9db` | Current source has Navidrome and AudioMuse modules, Unix/macOS native playback, protected credential files, async task cancellation and focused tests. LICENSE is Unlicense. Older web README describes a retired Plex implementation. |
| mattn/nd | `5b6e4757de100718a86ca766606201d66ff9f0c9` | Smaller Go/Bubble Tea client with mpv, artist details and song search. README declares MIT but this checkout has no LICENSE file. Unified category search and AudioMuse integration would require additional work; README recommends command-line passwords. |

Decision: reuse the pinned textamp source and add reproducible Nix setup rather
than rebuild working navigation/audio code. The full catalog cache and lack of
the requested dual recommendation selector remain gaps to assess after the MVP.

Keep automated tests isolated using XDG directories. Do not point mutation tests
at the user's server. See AGENTS.md and README.md for upstream verification rules.

## Verification record

- Linux release build passed without compiler warnings. The terminal UI opened,
  displayed library setup, and exited cleanly using isolated XDG directories.
- Native streamed-audio smoke test passed on this machine using silent generated
  loopback fixture audio: pause-preserving forward seek, backward seek, natural
  completion and cancellation.
- Unit suite: **196 passed, 0 failed, 1 ignored** (the ignored test makes live
  Wikimedia requests). NixOS test fixtures were fixed to resolve Python and
  core utilities through the environment instead of assuming `/usr/bin` paths.
- `cargo fmt --all -- --check`, `git diff --check`, and launcher shell syntax passed.
- Linux and Apple Silicon macOS Nix package definitions evaluated successfully;
  the Linux package build plan passed its dry run. Full Nix packaging, the full
  integration suite and Clippy have not been completed. The initial full release
  integration build was stopped because it compiles many separately optimized
  test binaries; the focused unit suite and native streaming smoke were used.
- Unauthenticated read-only API probe at `https://navi.paranas.ir` succeeded:
  Navidrome `0.64.2`, protocol `1.16.1`, OpenSubsonic enabled. The server correctly
  rejected the request without a username. Login, library contents, sonic
  extensions and playback from that server remain unverified until local sign-in.
- Native macOS build and playback remain unverified.
