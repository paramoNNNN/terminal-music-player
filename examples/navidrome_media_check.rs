//! Bounded live diagnosis using saved credentials. Never scrobbles or writes to
//! the server. --native-muted explicitly opts into silent native audio output.
use anyhow::{Context, Result};
use std::time::Duration;
use textamp::app::action::*;
use textamp::app::dispatch::{dispatch_action, handle_core_event};
use textamp::app::sources::{navidrome::Session, ActiveSource};
use textamp::app::state::{ArtistOverviewTab, View};
use textamp::app::{AppState, Event};
use textamp::audio::{AudioEvent, AudioPlayer};
use tokio::sync::mpsc;

fn main() -> Result<()> {
    // Read secrets before isolating every directory used by the diagnostic.
    let mut config = textamp::config::load_config()?;
    let selection = config
        .default_navidrome
        .as_ref()
        .context("No saved Navidrome selection")?;
    let source = config
        .navidrome_sources
        .iter()
        .find(|source| source.id == selection.source_id)
        .context("Saved account missing")?
        .clone();
    let password = textamp::library::credentials::load_source("navidrome", &source.id)?
        .context("Saved credentials missing")?;
    let client = textamp::navidrome::Client::new(
        &source,
        password.as_str().into(),
        selection.folder.clone(),
    )?;
    let session = Session {
        source,
        client,
        extensions: Default::default(),
        extensions_verified: false,
    };
    let root = std::env::temp_dir().join(format!("textamp-live-media-{}", uuid::Uuid::new_v4()));
    for (key, subdir) in [
        ("XDG_CONFIG_HOME", "config"),
        ("XDG_DATA_HOME", "data"),
        ("XDG_CACHE_HOME", "cache"),
        ("XDG_STATE_HOME", "state"),
    ] {
        let path = root.join(subdir);
        std::fs::create_dir_all(&path)?;
        std::env::set_var(key, path);
    }
    let native = std::env::args().any(|arg| arg == "--native-muted");
    let args: Vec<_> = std::env::args().collect();
    let query = args
        .windows(2)
        .find(|pair| pair[0] == "--query")
        .map(|pair| pair[1].clone());
    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(async move {
        tokio::time::timeout(
            Duration::from_secs(240),
            check(session, &mut config, native, query),
        )
        .await
        .context("Live check timed out")?
    })
}

async fn check(
    session: Session,
    config: &mut textamp::config::Config,
    native: bool,
    query: Option<String>,
) -> Result<()> {
    session.client.call("ping", &[]).await?;
    println!("PASS: saved server authentication");
    let response = session
        .client
        .call(
            "getAlbumList2",
            &[("type", "alphabeticalByName".into()), ("size", "1".into())],
        )
        .await?;
    let albums: Vec<textamp::navidrome::Album> =
        textamp::navidrome::array(&response["albumList2"], "album")?;
    let album = albums.into_iter().next().context("No albums")?;
    let requested_song = if let Some(query) = query {
        let response = session
            .client
            .call(
                "search3",
                &[
                    ("query", query.clone()),
                    ("songCount", "50".into()),
                    ("albumCount", "0".into()),
                    ("artistCount", "0".into()),
                ],
            )
            .await?;
        let songs: Vec<textamp::navidrome::Song> =
            textamp::navidrome::array(&response["searchResult3"], "song")?;
        Some(
            songs
                .iter()
                .find(|song| song.title.eq_ignore_ascii_case(&query))
                .or_else(|| songs.first())
                .context("No matching songs")?
                .clone(),
        )
    } else {
        None
    };
    let raw = session
        .client
        .album(
            requested_song
                .as_ref()
                .and_then(|song| song.album_id.as_deref())
                .unwrap_or(&album.id),
        )
        .await?;
    let song = requested_song
        .or_else(|| raw.song.first().cloned())
        .context("Album has no songs")?;
    println!(
        "Sample source format: {}",
        song.suffix.as_deref().unwrap_or("not supplied")
    );
    let artist = session
        .client
        .artist(raw.artist_id.as_deref().context("Album has no artist ID")?)
        .await?;
    let artist_key = session.key(&artist.id);
    let mut state = AppState::new();
    state.view = View::Browse;
    state.active_library = Some("live-media-check".into());
    state.library.artists = vec![session.artist(artist)];
    state.library.albums = vec![session.album(raw.clone())];
    state.library.all_tracks = raw
        .song
        .into_iter()
        .map(|song| session.track(song))
        .collect();
    state.sources.active = ActiveSource::Navidrome(Box::new(session.clone()));
    let mut no_audio = AudioPlayer::new_without_audio();
    let (tx, mut rx) = mpsc::channel(100);
    dispatch_action(
        BrowseAction::OpenArtistOverview { artist_key }.into(),
        &mut state,
        &mut no_audio,
        config,
        &tx,
    )
    .await?;
    let actions = textamp::app::handlers::key_input::handle_key(
        crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Enter,
            crossterm::event::KeyModifiers::NONE,
        ),
        &mut state,
        config,
    );
    for action in actions {
        dispatch_action(action, &mut state, &mut no_audio, config, &tx).await?;
    }
    while state
        .artist_overview
        .as_ref()
        .is_some_and(|page| page.album_loading)
    {
        let event = tokio::time::timeout(Duration::from_secs(35), rx.recv())
            .await?
            .context("Event channel closed")?;
        for action in handle_core_event(event, &mut state, &tx) {
            dispatch_action(action, &mut state, &mut no_audio, config, &tx).await?;
        }
    }
    let page = state
        .artist_overview
        .as_ref()
        .context("Overview did not open")?;
    anyhow::ensure!(
        page.album_error.is_none(),
        "Album details failed: {:?}",
        page.album_error
    );
    anyhow::ensure!(
        page.tab == ArtistOverviewTab::Songs && !page.album_tracks.is_empty(),
        "Album did not open a visible song list"
    );
    let selected = page.selected_album.as_ref().context("Album not selected")?;
    println!(
        "PASS: artist → album navigation; {} songs; label={}, release date={}, country={}",
        page.album_tracks.len(),
        !selected.record_labels.is_empty(),
        selected.release_date.is_some(),
        selected.country.is_some()
    );
    let (cover_key, _) = page.artwork().context("Sample album has no cover ID")?;
    let cover_key = cover_key.to_owned();
    for action in handle_core_event(Event::Tick, &mut state, &tx) {
        dispatch_action(action, &mut state, &mut no_audio, config, &tx).await?;
    }
    while !state.artwork.grid_cache.contains_key(&cover_key)
        && state.artwork.grid_failures.is_empty()
    {
        let event = tokio::time::timeout(Duration::from_secs(35), rx.recv())
            .await?
            .context("Event channel closed")?;
        for action in handle_core_event(event, &mut state, &tx) {
            dispatch_action(action, &mut state, &mut no_audio, config, &tx).await?;
        }
    }
    let bytes = state
        .artwork
        .grid_cache
        .get(&cover_key)
        .context("Cover download failed")?;
    let image = image::load_from_memory(bytes)?;
    state.artwork.mode = textamp::app::state::ArtworkMode::Halfblocks;
    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(120, 52))?;
    terminal.draw(|frame| {
        let _ = textamp::ui::render(frame, &state);
    })?;
    let rendered = terminal
        .backend()
        .buffer()
        .content
        .iter()
        .filter(|cell| matches!(cell.symbol(), "▀" | "▄"))
        .count();
    anyhow::ensure!(
        rendered > 100,
        "Cover did not produce colored terminal cells"
    );
    println!(
        "PASS: real cover {}×{} decoded and rendered in the album pane",
        image.width(),
        image.height()
    );
    if native {
        let mut player = AudioPlayer::new()?;
        player.set_volume(0.0);
        // First reproduce the previous request, then check the fixed request and raw audio.
        let urls = [
            (
                "server-default 320 kbps (previous behavior)",
                session.client.url(
                    "stream",
                    &[("id", song.id.clone()), ("maxBitRate", "320".into())],
                )?,
            ),
            (
                "explicit MP3 320 kbps",
                session.playback_url(&song.id, 320)?,
            ),
            ("original raw audio", session.playback_url(&song.id, 0)?),
        ];
        for (label, url) in urls {
            println!("CHECK: {label}");
            let (audio_tx, mut audio_rx) = mpsc::channel(100);
            player.play_url_with_headers(
                url.as_str(),
                Default::default(),
                None,
                audio_tx,
                session.client.http(),
            )?;
            let outcome = tokio::time::timeout(Duration::from_secs(30), async {
                loop {
                    if player.position().is_some_and(|position| position >= Duration::from_secs(1)) { return Ok(()); }
                    tokio::select! {
                        event = audio_rx.recv() => if let Some(AudioEvent::Error { message, .. }) = event { return Err(anyhow::anyhow!("{message}")); },
                        _ = tokio::time::sleep(Duration::from_millis(25)) => {}
                    }
                }
            }).await.unwrap_or_else(|_| Err(anyhow::anyhow!("Native decoding did not advance within 30 seconds")));
            player.stop();
            match outcome {
                Ok(()) => println!("PASS: {label}: muted native decoding advanced"),
                Err(error) if label.starts_with("server-default") => {
                    println!("REPRODUCED: {label}: {error}")
                }
                Err(error) => return Err(error.context(label)),
            }
        }
    }
    println!("PASS: no scrobbles or server write operations sent; runtime data isolated");
    Ok(())
}
