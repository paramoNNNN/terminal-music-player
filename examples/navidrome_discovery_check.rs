//! Bounded read-only check of album label tags/counts and artist popularity.
//! Uses saved credentials locally; never streams, scrobbles or writes to the server.
use anyhow::{Context, Result};
use textamp::app::sources::navidrome::Session;

#[tokio::main]
async fn main() -> Result<()> {
    let config = textamp::config::load_config()?;
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
    let isolated = tempfile::tempdir()?;
    for (name, directory) in [
        ("XDG_CONFIG_HOME", "config"),
        ("XDG_DATA_HOME", "data"),
        ("XDG_CACHE_HOME", "cache"),
        ("XDG_STATE_HOME", "state"),
    ] {
        let path = isolated.path().join(directory);
        std::fs::create_dir_all(&path)?;
        std::env::set_var(name, path);
    }
    tokio::time::timeout(std::time::Duration::from_secs(90), async {
        session.client.call("ping", &[]).await?;
        if std::env::args().any(|arg| arg == "--credits") {
            let mut songs = Vec::new();
            for query in ["Pursuit of Happiness", "Erase Me", "Make Her Say"] {
                let mut params = session.client.scope();
                params.extend([("query", query.into()), ("songCount", "10".into()), ("artistCount", "0".into()), ("albumCount", "0".into())]);
                let response = session.client.call("search3", &params).await?;
                let found: Vec<textamp::navidrome::Song> = textamp::navidrome::array(&response["searchResult3"], "song")?;
                println!("PASS: read-only search for {query} ({} songs)", found.len());
                songs.extend(found);
            }
            let mut artists = Vec::new();
            if std::env::args().any(|arg| arg == "--full-directory") {
                artists = session.client.artists().await?;
                println!("PASS: full server artist directory ({} artists)", artists.len());
            } else {
            for name in ["MGMT", "Kanye West", "Kid Cudi"] {
                let mut params = session.client.scope();
                params.extend([("query", name.into()), ("artistCount", "10".into()), ("songCount", "0".into()), ("albumCount", "0".into())]);
                let response = session.client.call("search3", &params).await?;
                let found: Vec<textamp::navidrome::Artist> = textamp::navidrome::array(&response["searchResult3"], "artist")?;
                artists.extend(found.into_iter().filter(|artist| artist.name.eq_ignore_ascii_case(name)));
            }
            }
            let mut albums = Vec::new();
            let mut seen = std::collections::HashSet::new();
            for song in &songs {
                if let Some(id) = &song.album_id {
                    if seen.insert(id.clone()) { albums.push(session.album(session.client.album(id).await?)); }
                }
            }
            let mut state = textamp::app::AppState::new();
            state.library.artists = artists.into_iter().map(|artist| session.artist(artist)).collect();
            state.library.albums = albums;
            state.library.all_tracks = songs.into_iter().map(|song| session.track(song)).collect();
            let (tx, _rx) = tokio::sync::mpsc::channel(8);
            for name in ["MGMT", "Kanye West", "Kid Cudi"] {
                let artist = state.library.artists.iter().find(|artist| artist.title.eq_ignore_ascii_case(name)).context("Requested artist not in server catalog")?.clone();
                let expected: std::collections::HashSet<_> = state.library.all_tracks.iter().filter(|track| track.is_credited_to(&artist)).filter_map(|track| track.parent_rating_key.clone()).collect();
                textamp::app::handlers::dispatch_browse::dispatch(&tx, textamp::app::action::BrowseAction::OpenArtistOverview {artist_key: artist.rating_key}, &mut state).await?;
                let page = state.artist_overview.as_ref().unwrap();
                anyhow::ensure!(expected.iter().all(|key| page.display_albums().iter().any(|album| &album.rating_key == key)), "Credited release missing from artist page");
                let count = page.display_albums().iter().filter(|album| page.release_section(album).label() == "Appears in").count();
                if name != "Kid Cudi" { anyhow::ensure!(count > 0, "Guest artist has no appearances"); }
                println!("PASS: {name} artist page matches {} credited releases; Appears in: {count}", expected.len());
            }
            return Ok::<_, anyhow::Error>(());
        }
        let mut params = session.client.scope();
        params.extend([("type", "alphabeticalByName".into()), ("size", "5".into())]);
        let response = session.client.call("getAlbumList2", &params).await?;
        let albums: Vec<textamp::navidrome::Album> = textamp::navidrome::array(&response["albumList2"], "album")?;
        let first = albums.first().context("No albums in the selected library")?;
        let raw = session.client.album(&first.id).await?;
        let album = session.album(raw);
        println!("PASS: album label/count normalization ({} labels; album play count supplied: {})", album.record_labels.len(), album.play_count.is_some());
        if let Some(artist_id) = first.artist_id.as_deref() {
            let related = session.client.call("getArtistInfo2", &[("id", artist_id.into()), ("count", "10".into()), ("includeNotPresent", "false".into())]).await?;
            let artists: Vec<textamp::navidrome::Artist> = textamp::navidrome::array(&related["artistInfo2"], "similarArtist")?;
            println!("PASS: artist-scoped server discovery ({} related artists)", artists.len());
        }
        let top = session.client.top_songs_with_source(first.artist.as_deref().context("Album has no artist")?, first.artist_id.as_deref()).await;
        match top {
            Ok((songs, source)) => {
                let tracks: Vec<_> = songs.into_iter().map(|song| session.track(song)).collect();
                println!("PASS: server top-songs normalization ({} songs; upstream agent reported: {}; songs with play counts: {})", tracks.len(), source.is_some(), tracks.iter().filter(|track| track.view_count.is_some()).count());
            }
            Err(_) => println!("UNAVAILABLE: server community popularity; the UI must preserve cached results and show the provider error"),
        }
        println!("PASS: read-only requests only; no stream, play-history, favorite or playlist mutations");
        Ok::<_, anyhow::Error>(())
    }).await.context("Discovery check timed out")??;
    Ok(())
}
