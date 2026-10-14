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
on this MacBook is recorded below; the full Nix package build remains unverified.

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

### MacBook validation — 2026-10-12

- Machine: Apple Silicon (`arm64`), macOS **15.1.1** (24B91). The shell reported
  `TERM=xterm-ghostty`; automated terminal checks used a PTY with the Apple
  Terminal artwork fallback, rather than verifying Ghostty's graphics protocol.
- Xcode command line tools and Nix **2.34.8** were available. Rust was not on
  the normal PATH; the existing pinned Nix development shell supplied
  `rustc 1.98.1` and `cargo 1.98.0`. No separate Rust or mpv installation was needed.
- `nix develop --command cargo build --locked --release --bin textamp` passed
  with no Rust compiler warnings. `file target/release/textamp` confirmed a native
  ARM64 Mach-O executable. The first release build took about six minutes;
  `./run` now reuses the executable without entering Nix.
- The first Nix attempt failed on TLS/cache downloads and then bootstrap
  downloads. A retry succeeded. The user reported this behavior while using a
  VPN; its exact cause was not established, and network settings were not changed.
- The inherited `LC_ALL=C.UTF-8` produced Bash locale warnings on macOS. New
  verification processes used `LC_ALL=en_US.UTF-8`; Cargo still replayed cached
  shell diagnostics from the initial release compilation. These were separate
  from Rust compiler warnings. If the terminal has that locale, use
  `export LC_ALL=en_US.UTF-8` before building or launching.
- `cargo test --locked --lib`: **189 passed, 0 failed, 1 ignored** (the live
  Wikimedia test). `cargo fmt --all -- --check` passed. App tests used temporary
  XDG configuration, data, cache and state directories.
- `cargo run --locked --release --example audio_smoke -- --stream` passed
  using generated, muted loopback audio through CoreAudio on the current default
  output. It verified forward/backward seek, preservation of pause and position,
  natural completion and cancellation. This was a silent transport check, not
  an audible listening test or playback from the personal server.
- The local-folder PTY smoke passed startup, filename navigation, **comma**
  opening Settings, add/cancel connection forms, settings navigation, per-library
  cache display, the library switcher, Ctrl+Q exit and terminal restoration.
  It used a temporary copy of `tests/fixtures/folder_tui_smoke.py` with its first
  F2 input replaced by comma, plus `--browse-only`. Personal settings were untouched.
- No application source changes were needed. The full integration suite, Clippy,
  full Nix package build, audible listening, authenticated Navidrome browsing and
  server playback remain unverified on this Mac. Sign in locally through
  **`,` → Libraries → Add library** to complete the personal-server checks.

### Artwork, album navigation and live media validation — 2026-10-13

- Fixed mismatched artwork request/render identities in artist/album overviews,
  added track-cover fallback when album art is absent, and use halfblocks inside
  tmux or after a failed terminal graphics probe. Invalid cached image payloads
  are refetched; failed covers have a 30-second retry delay and a visible state.
- Album selection refreshes details through the source boundary without dropping
  cached songs on failure. The pane shows available release/original dates,
  labels, country, edition/types, genres, moods, duration and source audio specs.
  Song order follows disc/track numbers. Enter on a song opens Now Playing.
  Two-column browsing keeps the focused track list visible alongside details.
- The playback log contained `Decode error: Unrecognized format`. The saved
  setting was 320 kbps with no requested format. Bitrate-limited playback now
  requests MP3 explicitly; original quality still requests raw audio. Repeated
  playback errors retain the underlying cause in the error dialog and error log.
- At the user's explicit request, `navidrome_media_check --native-muted` tested
  the real saved server using bounded reads and muted CoreAudio. The initial
  album sample passed all three playback variants. On the exact failing track,
  `Went to See the Gypsy (demo)`, and `A Hard Day's Night` from the screenshot,
  the previous server-default transcode request stalled for 30 seconds; explicit
  MP3 at 320 kbps and original FLAC both advanced through native playback.
- The real album pages returned labels and release dates, but no country. Their
  covers downloaded, decoded and rendered in the album pane at 500×500 and
  500×495. Artist → album navigation loaded 77 and 13 songs respectively. These
  were application action/render checks plus muted native decoding, not an
  interactive visual check of the user's running tmux session.
- The live diagnostic reads credentials locally before isolating its XDG runtime
  directories. It sends no scrobbles, reports or server mutation calls. Automated
  regressions remain in `tests/`; temporary runtime data stays outside the repo.
- The full normal test suite passed: 189 unit tests, all integration suites
  (including seven overview regressions and the provider album-detail fixture),
  and eight doc tests. Live fixture/GUI tests remain intentionally ignored.
- Test linking initially exhausted disk space. Only generated `target/debug`
  build artifacts were removed, then rebuilt with debug symbols and incremental
  caching disabled for this verification run. Personal data was not changed.
- Final `cargo fmt --all -- --check`, strict all-target Clippy, the repeated full
  test suite, and `cargo build --locked --release --bin textamp` passed. The final
  optimized build took 4m57s with no Rust compiler warnings. Nix replayed its
  existing Bash locale diagnostics. The installed copy was not replaced.

### Terminal colors, metadata and macOS controls — 2026-10-13

- Added a terminal-following theme using default foreground/background and ANSI
  palette colors. It preserves terminal transparency and palette changes. New
  configurations default to it; the user's saved `ui.theme` was changed to
  `terminal` at their request, preserving every other setting and credentials.
- Removed the previous unconditional halfblock override in tmux. Auto artwork
  again uses the detected Kitty/iTerm2/Sixel pixel protocol; halfblocks remain
  available explicitly or when the terminal probe fails. The user's interactive
  tmux graphics display was not visually inspected in this verification run.
- The Library track pane retains additional OpenSubsonic tags, including credits,
  BPM, comments, MusicBrainz IDs, moods, dates and ratings. It displays available
  audio properties, file information and cached album release details. Values
  wrap, with mouse-wheel and focused PgUp/PgDn scrolling independent of track
  selection. Existing serialized caches remain readable; F5 refreshes new tags.
- Now Playing shows source codec/bitrate and favorite status. Requested MP3
  bitrate limits are identified separately and captured at stream start.
  Confirmed favorite changes update visible song copies, scoped to the library.
  Unknown favorite status is explicit for older caches/unsupported sources.
- Added `Go to Artist Page` to the palette/right-click context menu for selected
  songs, albums and artists. Performer-only cache entries can open an overview
  with their songs and album appearances. Favorites ordering was left unchanged
  after the user clarified the existing second play button works correctly.
- Added native macOS MediaPlayer metadata and remote commands with AppKit kept
  on the process main thread and the terminal runtime on a worker. Commands route
  through normal playback reducers; Play/Pause are idempotent and commands for
  an older track are discarded. Stop/exit clears native Now Playing metadata.
  No application window or Dock icon is created.
- The opt-in `system_media_check --native` fixture passed native title/artist/
  album, duration, elapsed time, playback rate, playing/paused state, native Pause
  callback routing and stop cleanup. It uses synthetic metadata without audio
  devices, saved credentials or server requests. Actual menu-bar appearance and
  physical media-key behavior still need an interactive user check.
- New framework dependency fetching hit Cargo TLS failures. The integration
  instead uses the already cached objc2/Foundation bindings and public framework
  APIs; locked offline builds succeeded without changing network settings.
- Full isolated suite: **460 passed, 0 failed, 4 ignored**, including six track
  metadata/control regressions and the terminal-theme regression. Formatting,
  strict all-target Clippy and whitespace checks passed. The native ARM64 release
  build passed in **3m18s with zero Rust compiler warnings**; cached Nix shell
  locale diagnostics remain unrelated to Rust compilation.
- Both development and optimized binaries passed the isolated local-folder PTY
  smoke: comma Settings, navigation, add/cancel, cache pages, F3 switcher,
  Ctrl+Q exit and terminal restoration. The existing smoke now forces a redraw
  before timing out on text reused by differential terminal rendering.
- The installed copy in `~/Applications` was not replaced. No commit or push.

### Favorite shortcut, terminal contrast and full-pixel covers — 2026-10-13

- F toggles the selected song's favorite status, or the playing song in Now
  Playing/full-screen. Search and other text inputs retain F for typing. The
  source adapter reads the current server status before writing; failed requests
  preserve the displayed status. Automated star/unstar tests use loopback only.
- Terminal-theme metadata, borders and secondary labels use the terminal's
  normal foreground. Selected text reverses terminal defaults for contrast on
  both light and dark palettes. Native image cells are excluded from that text
  styling so their Kitty image-ID colors remain intact.
- Found a size-dependent tmux transport defect in ratatui-image 9.0.0: the whole
  Kitty transfer was wrapped in one passthrough message. Small thumbnails could
  work while full-size covers exceeded tmux's input buffer and disappeared.
  Vendored the locked MIT crate (168 KiB, license preserved) and backported the
  upstream per-chunk passthrough fix. This preserves full pixel data and requires
  no server or terminal configuration changes. TEXTAMP-PATCH.md records the
  source and the removal condition for a future dependency upgrade.
- Grid artwork protocols now include render dimensions in cache identity. A
  small thumbnail cannot resize/replace the same album's large preview image.
- Added cover thumbnails to overview album rows, related artist portraits and
  albums, similar albums, search tabs/global results and adventure album/search
  lists. Recent/favorite album collections now honor the library's cover-display
  preference. Missing images have visible placeholders. Background downloads
  use bounded visible windows and the existing library-scoped artwork cache.
- Related artist/album rows use two rows each. Mouse selection, scroll pinning
  and scrollbar calculations use that same geometry. Help, shortcut hints and
  README document F and the rendering behavior.
- Final isolated full suite passed: **467 passed, 0 failed, 4 ignored**. Formatting
  and strict all-target Clippy passed. Regressions verify a 500×500 image transfer
  exceeds 1 MiB overall while every tmux message stays below 8 KiB; unwrapping
  reconstructs the native transfer byte-for-byte. Other regressions cover
  independent preview/thumbnail IDs, selected Kitty cells, readable terminal
  selection, related-row artwork requests/clicks and loopback favorite writes.
- Removed 33 obsolete generated test executables from the previous validation
  run to recover 2.77 GiB for builds. Current artifacts and personal files were
  preserved. The development binary passed isolated comma Settings/navigation/
  shutdown/terminal-restoration checks. Interactive visual confirmation in the
  user's Ghostty/tmux session remains a user check, not an automated test claim.
- Final optimized ARM64 build succeeded in **7m28s with zero Rust compiler
  warnings**. Cached Nix locale diagnostics remain separate. The release binary
  passed the isolated terminal startup, comma Settings, navigation, Ctrl+Q and
  terminal-restoration smoke. No installed binary was replaced, committed or
  pushed.

### Favorite markers in track lists and pickers — 2026-10-13

- Added shared favorite markers: ♥ favorited, ♡ not favorited, ? unavailable
  (including old caches or sources without favorite support). Track titles and
  source keys remain unchanged; indicators are presentation only.
- Markers appear in Miller track columns, artist/album songs, search tabs/global
  results, similar songs, adventure search/album-track pickers, queue, track
  details, transport, folder tracks and shared track-list widgets. Scrolling
  titles in Miller/queue keep the heart fixed ahead of the title.
- Successful source favorite updates also refresh adventure picker/result copies
  and saved column ordering, so restoring a shuffled/sorted column preserves the
  current favorite state. Existing library identity checks remain in place.
- README and shared help document the marker legend. Rendering regressions cover
  all three favorite states in Library, queue, search/global, similar songs,
  artist/album overview and the adventure album-track picker; another regression
  verifies restoring column order retains a favorite change.
- Final isolated full suite: **469 passed, 0 failed, 4 ignored**. Formatting,
  strict all-target Clippy and whitespace checks passed. Optimized ARM64 macOS
  build succeeded in **10m22s with zero Rust compiler warnings**. The release
  binary passed the isolated folder navigation, comma Settings, library manager,
  add/cancel form, cache display, F3 switcher, Ctrl+Q and terminal-restoration
  smoke. No personal server mutations, installed binary replacement, commit or
  push were performed.

### Record labels, artist rankings and favorite-only hearts — 2026-10-13

- Added Browse Record Labels to the palette/right-click menu and contextual
  Go to Label entries for albums and songs with record-label metadata. Label
  pages keep two columns, with album/artist tabs, artwork, keyboard/mouse
  navigation and album-song drill-down. Opening a page does not play anything.
  F5 refreshes label metadata through the existing catalog task; rebuilding the
  page preserves selected identity and the mouse-pinned viewport. Backing out
  clears hidden album/artist context before returning to the label list.
- Artist-page R (also clickable/in the palette) cycles Catalog, Most Played and
  Community; Tab switches albums/songs. Personal song rankings use reported
  play counts. Album rankings use the reported album count or explicitly labeled
  cached track-play totals. Visible ranking order is also the playback/queue
  command target order. Album-track playback remains in disc/track order.
- Community popularity uses the server's getTopSongs operation, optionally by
  artist ID when that extension is advertised. Results retain server rank order,
  are scoped to the selected library and cached per library with weekly/F5
  refresh. Cached startup comes first; failures retain the chart. Completions
  check library, operation and artist-page identity. Community album rows follow
  first appearance in ranked songs; they are not a separate top-albums chart or
  review ranking. No independent review-site/Last.fm client connection was added.
- Only favorited songs receive a heart. Non-favorited and unknown-status songs
  have no favorite icon; Now Playing retains plain-text status when needed.
- README/help explain the controls, ranking provenance and Navidrome Sonic versus
  direct AudioMuse: an OpenSubsonic API route versus an analysis service which
  can also implement that route through a plugin. Album/track artwork jobs now
  follow the actual visible overview tab.
- Final isolated full suite: **475 passed, 0 failed, 4 ignored**. Formatting,
  strict all-target Clippy and whitespace checks passed. Final optimized ARM64
  build succeeded in **4m29s with zero Rust compiler warnings**. The final binary
  passed isolated folder navigation, comma Settings, library/add/cancel/cache
  controls, F3 switcher, Ctrl+Q and terminal restoration.
- A bounded read-only check against the saved real Navidrome connection returned
  **58 community top songs**, which normalized successfully. The sampled album
  supplied **zero record labels and no album play count**; the UI requires actual
  label tags and labels song-play aggregation rather than inventing album counts.
  The diagnostic source lives in examples/navidrome_discovery_check.rs, reads
  credentials locally, isolates XDG paths and never streams or mutates favorites,
  playlists or play history. Loopback fixtures cover failed popularity requests,
  caching, scope, ranking order, stale responses and label navigation/refresh.
- No installed binary replacement, commit or push was performed.

### Catalog queue confirmation and artist discovery tabs — 2026-10-13

- Catalog song activation now uses a shared PlayCatalog action: replace the queue
  in visible order, start at the selection and open the combined Now Playing
  screen. Artist rankings, similar songs, search results, Miller lists, track-pane
  Play Track++ and play-all rows use this path. Explicit single-track prepend,
  play-next and append remain separate operations.
- The queue remembers its initial catalog order. Add/remove/reorder operations
  require confirmation before the next catalog replacement; ordinary playback
  progression and favorite metadata updates do not. Restored queues are protected.
  Confirmation defaults to No and does not interrupt playback or change views.
  A library switch or newer queue edit invalidates a captured confirmation.
- Artist pages expose clickable, wrapping numbered tabs: Albums, Songs, Top
  albums, Top plays, Top server, Tags, Sonic and Related. Tab/Shift+Tab and 1–8
  select them. R targets the viewed artist's server-related artists; Ctrl+M uses
  songs from that artist for Sonic discovery, never the playing song's artist.
  Tags ranks shared genre-tag overlap; Sonic aggregates unique song matches from
  up to three seed songs. Related artist portraits open their artist pages.
- Server discovery is cached per library, artist and mode, with weekly/F5 refresh,
  task leases, scoped completion checks and retained usable data on failure.
  Explicit Sonic mode uses the existing Navidrome Sonic/direct AudioMuse adapter,
  with no silent genre-tag fallback. Related identifies getArtistInfo2 and does
  not assume which upstream agent or similarity algorithm the server selected.
- Top server retains getTopSongs order, shows an upstream source when reported,
  and displays available ranking metrics. Personal play counts/ratings are labeled
  as personal. Missing source identity or counts are shown honestly. The separate
  Top albums tab uses personal counts; there is no fabricated community album chart.
- Down stays within the Now Playing song list at its boundary, including empty,
  one-song and radio queues. Ctrl+N or clicking the visualizer moves focus below;
  Ctrl+U returns to queue controls. Updated shortcut hints, help and README.
- Isolated full suite: **480 passed, 0 failed, 4 ignored**. Formatting and strict
  all-target Clippy passed. Tests are in the project, including catalog_playback,
  artist_overview, input_focus, palette_routes and the Navidrome loopback fixture.
- Read-only live Navidrome validation returned **2 related artists** and **58 top
  songs**. The top songs supplied **no upstream agent name or play counts**; the
  UI reports those as unavailable rather than claiming Last.fm/global statistics.
  The sampled album again supplied no label or album count. No stream, scrobble,
  favorite, rating or playlist write was made by this diagnostic.
- Final optimized macOS build succeeded in **4m19s with zero Rust compiler
  warnings**. The final executable passed isolated folder startup/navigation,
  comma Settings, library manager, add/cancel connection form, per-library cache
  display, F3 switcher, Ctrl+Q and terminal restoration. Whitespace checks passed.
  No installed binary replacement, commit or push was performed. Run `./run` to
  use this checkout's rebuilt executable.

### Artist discography release sections and appearances — 2026-10-13

- Artist Albums tabs now separate populated release-type sections: Albums, EPs,
  Singles, Compilations, Live, Soundtracks, Remixes, DJ mixes and Other releases.
  Supplied secondary types (such as live or compilation) take precedence over a
  primary album/single type. Legacy compilation metadata remains supported;
  untagged releases remain Albums, with no guessing from release titles.
- Appears in separates credited releases whose album artist differs from the
  viewed artist. It uses the existing credited-song catalog matching and opens
  the release normally. Original catalog order is stable inside each section;
  Top albums retains global personal-play-count ranking.
- Nonselectable section headings show counts and repeat at the top when scrolling
  into a section. Cards keep their artwork and full two-line click areas.
  Keyboard navigation skips headings; the selected album stays visible in short
  viewports. Clicking pins the first visible album until keyboard navigation.
- Added project regression coverage for primary/secondary/legacy release tags,
  header rendering, artwork and mouse alignment, album activation, small-screen
  scrolling, ranked ordering and credited appearances. All 16 artist-overview
  tests passed; strict all-target Clippy passed. README, help and layout comments
  describe the sections. Full-suite/release results follow below.
- Final discography-only verification: **483 passed, 0 failed, 4 ignored**;
  formatting and strict all-target Clippy passed. Optimized macOS build succeeded
  in **6m58s with zero Rust warnings**, and isolated terminal smoke checks passed.
  The user then requested the compact Now Playing layout below.

### Compact waveform in Now Playing — 2026-10-13

- Now Playing/Queue gives the main list most vertical space. The bottom waveform
  strip uses one fifth of available height, capped at six rows including borders,
  and adapts to short terminals and the Library split layout.
- Removed visualization tabs from the normal combined screen; it always renders
  waveform without changing the remembered full-screen mode. F7 retains artwork
  and all six full-screen visualizations. Mouse waveform seeking works even when
  a different full-screen visualization was last selected.
- Up returns directly from the waveform to the queue. Updated focus handling,
  shortcut hints, help, README and layout comments. Regression tests cover
  compact dimensions, split layouts, waveform-only rendering, preserved viewer
  mode, provider consistency, seeking and full-screen visualization controls.
- Final combined validation: **484 passed, 0 failed, 4 ignored**; formatting,
  strict all-target Clippy and whitespace checks passed. Optimized macOS build
  succeeded in **9m34s with zero Rust compiler warnings**. The final executable
  passed isolated startup/navigation, Settings/library/add/cancel/cache controls,
  F3, Ctrl+Q and terminal restoration. Run `./run` for the rebuilt checkout.
  No installed binary replacement, personal server writes, commit or push.

### Chronological artist releases and individual guest credits — 2026-10-13

- Artist release sections now sort oldest to newest by original release date,
  falling back to release date and tagged year. Partial dates are supported;
  missing dates sort last, with deterministic title/identity ties. Top albums
  retains its personal-play-count ranking.
- Fixed missing guest appearances: Navidrome supplied Kid Cudi, MGMT and Ratatat
  as individual artists for Pursuit of Happiness, but the app matched only the
  joined display string. Normalization now preserves each performer identity;
  artist pages and the track-artist index match individual credits. Structured
  artist IDs survive caches; legacy single-artist metadata remains usable.
  Composer-only contributors do not become performer appearances.
- Joint album-artist credits count as ownership, keeping those releases out of
  Appears in. Structured album artist names also correct the track's album-artist
  display when the legacy albumArtist string is absent.
- Old catalog snapshots trigger a background credit-schema refresh while showing
  cached data first. Failed refreshes retain the usable cache. Installing refreshed
  metadata rebuilds a visible discography while preserving selected identity and
  pinned viewport. Added coverage for cache upgrade, guest IDs, joint ownership,
  comma-containing artist names, chronology and refreshed-page selection.
- Read-only real-server checks initially timed out on getArtists during downtime.
  After the user restarted the server, the full retry returned **712 artists**.
  Artist-page reducers verified MGMT's appearance on the release containing Pursuit
  of Happiness and Kanye West's appearances on releases containing Erase Me and
  Make Her Say. The sampled Kid Cudi releases correctly remained owned releases.
  These counts cover the queried samples, not the full library discographies.
- Final isolated suite: **487 passed, 0 failed, 4 ignored**. Formatting, strict
  all-target Clippy and whitespace checks passed. Optimized macOS build succeeded
  in **4m31s with zero Rust compiler warnings**, and final terminal smoke tests
  passed. The diagnostic is examples/navidrome_discovery_check.rs --credits
  --full-directory; it reads saved credentials privately and never streams or
  writes favorites, ratings, playlists or history. No installed replacement,
  commit or push. Restart with ./run for the rebuilt checkout.

### Newest-first releases and appearance loading feedback — 2026-10-13

- Per user correction, artist release sections now sort **newest to oldest**;
  original/release-date/year precedence is unchanged and undated releases stay
  last. Top albums continues to use personal-play-count ranking.
- Artist album lists show a persistent "Loading releases / Appears in…" border
  indicator while the active catalog refresh is running. Existing cached releases
  remain selectable, with no extra list rows or viewport changes. Empty lists
  show "Loading releases and artist credits…" rather than "No entries available."
  The loading indicator clears with the actual catalog completion, including a
  failed refresh; normal app error feedback remains available for that failure.
- Regression coverage verifies cached content remains visible while loading,
  appearance sections arrive after refreshed credits, loading feedback clears,
  empty loading catalogs show progress, newest-first dates and preserved selected
  identity. All 19 artist-overview tests passed. Updated README/help/layout text.
- Final isolated full suite: **488 passed, 0 failed, 4 ignored**. Formatting,
  strict all-target Clippy and whitespace checks passed. Optimized macOS build
  succeeded in **6m05s with zero Rust compiler warnings**; final terminal smoke
  checks passed. Restart with ./run for this build. No installed replacement,
  personal-server writes, commit or push.

### Commit preparation and Nix configuration guide — 2026-10-13

- README now documents consuming the existing default flake package from NixOS,
  Home Manager or nix-darwin, private SSH input access, local-checkout inputs and
  lock updates. Credentials remain configured locally in the TUI.
- Evaluating packages.x86_64-linux.default.drvPath succeeded with the pinned
  flake. This validates the Linux package expression; a full Linux/NixOS package
  build was not run on this Mac. Latest Rust/runtime validation remains the
  488-test suite and zero-warning release/terminal checks recorded above.
- Commit includes the project source, tests, diagnostic examples and vendored
  ratatui-image patch. Its upstream LICENSE is preserved verbatim, including its
  existing trailing blank line. Project-source whitespace checks passed.

### GitHub binary releases and private Nix cache — 2026-10-14

- Added pinned GitHub Actions workflows for native x86_64/ARM64 Linux/macOS
  release archives and for the exact existing Nix flake package. Tagged native
  releases publish only after all four platform test/build/smoke jobs pass;
  manual runs produce artifacts without publishing. Nix cache builds run on
  master pushes/tags/manual dispatch when a cache name is configured.
- The Nix workflow uses repository variable CACHIX_CACHE and secret
  CACHIX_AUTH_TOKEN, builds/tests and smoke-checks the flake output, then pushes
  its runtime closure. No public-cache default or embedded token was added.
  Nix package checks use the debug profile (with debug symbols/incremental off)
  so every test executable does not incur release LTO; the app remains optimized.
- Native archives contain binary, licenses, build revision/platform/version and
  SHA-256 metadata plus a checksum sidecar. Added authenticated gh downloader
  and atomic installer. It resolves latest to one tag, checks archive/binary
  digests and architecture, rejects symlink destinations, and leaves existing
  installations untouched on failure. NixOS uses the flake/cache route.
- Eight isolated Python fixture tests passed for all archive platforms, checksum
  and download failures, atomic replacement, native platform selection, NixOS
  routing and symlink rejection. Actionlint/shellcheck passed; Nix evaluated the
  Linux package's debug check profile. README and docs/binary-distribution.md
  contain full cache, private-token, release and NixOS/Home Manager setup.
- User has used other NUR packages but owns no nur.nix: documented that a private
  flake input is sufficient, with specialArgs/extraSpecialArgs examples. No NUR
  registration, public repository or source-input override is required.
- Native macOS release build passed with no Rust warnings; the actual binary
  passed archive verification and the isolated browse-only terminal smoke test.
  Final workflow lint and macOS package derivation evaluation passed. Initial
  validation hit disk exhaustion and a terminated compiler; generated build
  artifacts were cleaned and verification retried with one Cargo job. Native CI
  uses that limit; Nix CI builds one derivation with two cores.
- Final serial validation passed: cargo test (488 passed, zero failed, four
  ignored), cargo fmt --all -- --check and cargo clippy --all-targets -- -D
  warnings. Eight release-tool fixture tests also passed. Other-platform builds
  and actual cache substitution remain to be verified by GitHub CI after setup.
- User authorized committing and pushing the distribution changes to origin.
  GitHub CLI is unauthenticated locally; Git uses the existing SSH remote.
  GitHub settings, Cachix account/token setup and release tagging still require
  setup; no personal binary replacement was performed here.
