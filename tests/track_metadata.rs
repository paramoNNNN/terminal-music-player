use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseEvent, MouseEventKind};
use ratatui::{backend::TestBackend, Terminal};
use textamp::app::handlers::{key_input, mouse_input};
use textamp::app::state::{BrowseCategory, BrowseColumn, BrowseItem, PlaybackMode, View};
use textamp::app::{Action, AppState};
use textamp::library::models::Album;
mod common;

fn fixture() -> AppState {
    let mut state = common::navidrome_state();
    let song = serde_json::from_value(serde_json::json!({
        "id":"song", "title":"Detailed Song", "artist":"Performer", "albumArtist":"Album Artist", "albumId":"album", "album":"Album", "duration":120,
        "suffix":"flac", "bitRate":950, "bitDepth":24, "samplingRate":96000,
        "channelCount":2, "track":10, "discNumber":2, "playCount":12, "size":12345678,
        "path":"Artist/Album/10 Song.flac", "starred":"2026-10-13T00:00:00Z",
        "userRating":4, "bpm":120, "comment":"A long comment that wraps across multiple rows in the narrow library track details pane",
        "musicBrainzId":"test-recording-id", "moods":["Warm", "Calm"],
        "artists":[{"id":"performer", "name":"Performer"}],
        "releaseDate":{"year":1986, "month":10, "day":27}
    })).unwrap();
    let session = state.sources.active.navidrome().unwrap();
    let track = session.track(song);
    let album = Album {
        rating_key: session.key("album"),
        title: "Album".into(),
        record_labels: vec!["Test Label".into()],
        country: Some("GB".into()),
        release_date: Some("1986-10-27".into()),
        ..Default::default()
    };
    state.library.albums = vec![album];
    state.queue.tracks = vec![track.clone()];
    state.queue.index = Some(0);
    state.set_playback_mode(PlaybackMode::Queue);
    state.artist_nav.columns = vec![BrowseColumn::new_with_tracks(
        "Songs",
        BrowseItem::from_tracks(std::slice::from_ref(&track)),
        vec![track],
    )];
    state.category_column_focused = false;
    state.browse_category = BrowseCategory::Library;
    state.track_pane_open = true;
    state
}

fn render(state: &mut AppState, width: u16, height: u16) -> String {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal
        .draw(|frame| state.apply_render_feedback(textamp::ui::render(frame, state)))
        .unwrap();
    terminal
        .backend()
        .buffer()
        .content
        .iter()
        .map(|cell| cell.symbol())
        .collect()
}

#[test]
fn track_tags_survive_cache_roundtrip_and_details_render_all_audio_properties() {
    let mut state = fixture();
    let track = &state.queue.tracks[0];
    assert_eq!(track.favorited, Some(true));
    assert_eq!(track.metadata_tags["bpm"], "120");
    assert_eq!(track.metadata_tags["artists"], "Performer");
    assert_eq!(track.metadata_tags["releaseDate"], "1986-10-27");
    let cached: textamp::library::models::Track =
        serde_json::from_str(&serde_json::to_string(track).unwrap()).unwrap();
    assert_eq!(cached.metadata_tags, track.metadata_tags);
    assert_eq!(cached.favorited, Some(true));
    let old: textamp::library::models::Track =
        serde_json::from_str(r#"{"ratingKey":"old"}"#).unwrap();
    assert!(old.metadata_tags.is_empty());
    assert_eq!(old.favorited, None);
    state.view = View::Browse;
    let screen = render(&mut state, 170, 100);
    for expected in [
        "Bit rate: 950 kbps",
        "Bit depth: 24 bit",
        "Sample rate: 96000 Hz",
        "Channels: 2",
        "Disc: 2",
        "Play count: 12",
        "File size: 12345678 bytes",
        "Test Label",
        "Country: GB",
        "Bpm: 120",
        "test-recording-id",
        "Warm, Calm",
        "Rating: 4/5",
    ] {
        assert!(screen.contains(expected), "Missing {expected}");
    }
}

#[test]
fn track_details_scroll_without_moving_the_song_selection() {
    let mut state = fixture();
    state.view = View::Browse;
    render(&mut state, 100, 25);
    let pane = state.hit_regions.track_pane.as_ref().unwrap().clone();
    assert!(pane.max_scroll > 0);
    mouse_input::handle_mouse(
        MouseEvent {
            kind: MouseEventKind::ScrollDown,
            column: pane.outer.x + 2,
            row: pane.outer.y + 5,
            modifiers: KeyModifiers::NONE,
        },
        &mut state,
    );
    assert_eq!(state.track_pane_scroll.as_ref().unwrap().1, 3);
    assert_eq!(state.artist_nav.focused().unwrap().selected_index, 0);
    state.track_pane_focused = true;
    key_input::handle_key(
        KeyEvent::new(KeyCode::PageDown, KeyModifiers::NONE),
        &mut state,
        &Default::default(),
    );
    assert!(state.track_pane_scroll.as_ref().unwrap().1 > 3);
}

#[test]
fn now_playing_displays_source_quality_requested_transcode_and_favorite_status() {
    for width in [45, 120] {
        let mut state = fixture();
        state.view = View::NowPlaying;
        state.transcode_kbps = 0;
        let screen = render(&mut state, width, 40);
        assert!(
            screen.contains("950 kbps"),
            "Missing quality at width {width}"
        );
        assert!(screen.contains("♥ Favorited"));
        state.queue.tracks[0].favorited = Some(false);
        assert!(render(&mut state, width, 40).contains("Not favorited"));
        state.transcode_kbps = 320;
        let screen = render(&mut state, width, 40);
        assert!(screen.contains("requested 320 kbps"));
        state.playback.stream_bitrate_limit = Some(128);
        assert!(render(&mut state, width, 40).contains("requested 128 kbps"));
    }
}

#[test]
fn artist_page_menu_targets_the_selected_track_and_is_hidden_in_unrelated_views() {
    let mut state = fixture();
    state.library.artists = vec![textamp::library::models::Artist {
        rating_key: "performer".into(),
        title: "Performer".into(),
        ..Default::default()
    }];
    state.view = View::Queue;
    let entry = textamp::app::command_palette::materialize_entries(&state)
        .into_iter()
        .find(|entry| entry.label == "Go to artist: Performer")
        .unwrap();
    assert!(
        matches!(textamp::app::command_palette::run(entry.command, &mut state).as_slice(), [Action::Browse(textamp::app::action::BrowseAction::OpenArtistOverview { artist_key })] if artist_key == &state.sources.active.navidrome().unwrap().key("performer"))
    );
    state.view = View::Settings;
    assert!(!textamp::app::command_palette::materialize_entries(&state)
        .iter()
        .any(|entry| entry.label.starts_with("Go to artist:")));
}

#[tokio::test]
async fn successful_favorite_changes_update_visible_copies_and_reject_other_libraries() {
    use textamp::app::sources::navidrome::{self, NavAction};
    let mut state = fixture();
    state.active_library = Some("current".into());
    let key = state.queue.tracks[0].rating_key.clone();
    let (tx, _rx) = tokio::sync::mpsc::channel(8);
    let mut audio = textamp::audio::AudioPlayer::new_without_audio();
    let mut config = Default::default();
    for (library_key, expected) in [("old", true), ("current", false)] {
        navidrome::dispatch(
            NavAction::FavoriteSaved {
                library_key: Some(library_key.into()),
                key: key.clone(),
                favorited: false,
            },
            &mut state,
            &mut audio,
            &mut config,
            &tx,
        )
        .await
        .unwrap();
        assert_eq!(state.queue.tracks[0].favorited, Some(expected));
        assert_eq!(
            state.artist_nav.focused().unwrap().tracks[0].favorited,
            Some(expected)
        );
    }
}

#[test]
fn native_play_and_pause_are_idempotent_and_seek_is_bounded() {
    use textamp::app::{action::PlaybackAction, state::PlayStatus};
    use textamp::media::system_controls::{playback_action, Command};
    let mut state = fixture();
    state.playback.duration_ms = 120_000;
    for status in [PlayStatus::Playing, PlayStatus::Buffering] {
        state.playback.status = status;
        assert!(playback_action(Command::Play, &state).is_none());
        assert!(matches!(
            playback_action(Command::Pause, &state),
            Some(PlaybackAction::TogglePlayPause)
        ));
    }
    state.playback.status = PlayStatus::Paused;
    assert!(playback_action(Command::Pause, &state).is_none());
    assert!(matches!(
        playback_action(Command::Play, &state),
        Some(PlaybackAction::TogglePlayPause)
    ));
    assert!(matches!(
        playback_action(Command::Seek(u64::MAX), &state),
        Some(PlaybackAction::Seek(120_000))
    ));
}

#[test]
fn f_targets_the_playing_song_and_text_input_keeps_the_letter() {
    use textamp::app::sources::navidrome::{commands::Command, NavAction};
    use textamp::app::sources::SourceAction;
    let mut state = fixture();
    state.view = View::NowPlaying;
    state.list_state.queue_index = 50;
    let actions = key_input::handle_key(
        KeyEvent::new(KeyCode::Char('f'), KeyModifiers::NONE),
        &mut state,
        &Default::default(),
    );
    assert!(
        matches!(actions.as_slice(), [Action::Source(SourceAction::Navidrome(action))] if matches!(action.as_ref(), NavAction::Command(Command::ToggleFavorite(track)) if track.rating_key == state.queue.tracks[0].rating_key))
    );
    state.view = View::Search;
    key_input::handle_key(
        KeyEvent::new(KeyCode::Char('f'), KeyModifiers::NONE),
        &mut state,
        &Default::default(),
    );
    assert_eq!(state.search.query, "f");
}

#[test]
fn favorites_are_visible_in_browse_queue_search_and_album_track_picker() {
    use textamp::app::state::{
        AdventureDrillLevel, AdventureLauncherState, AdventureStep, SearchFocus, SearchTab,
    };
    use textamp::library::models::SearchResults;
    for (favorited, marker) in [(Some(true), "♥"), (Some(false), ""), (None, "")] {
        let mut state = fixture();
        // Leave the transport empty so a passing assertion must come from the listing.
        let mut track = state.queue.tracks[0].clone();
        track.favorited = favorited;
        track.index = None;
        state.queue.index = None;
        state.track_pane_open = false;
        state.artist_nav.columns[0].items = BrowseItem::from_tracks(std::slice::from_ref(&track));
        state.artist_nav.columns[0].tracks = vec![track.clone()];
        state.view = View::Browse;
        assert!(render(&mut state, 160, 55).contains(&format!("{marker} Detailed Song")));
        state.view = View::Queue;
        state.queue.tracks = vec![track.clone()];
        assert!(render(&mut state, 160, 55).contains(&format!("{marker} Detailed Song")));
        state.view = View::Search;
        state.search.tab = SearchTab::Tracks;
        state.search.results = Some(SearchResults {
            tracks: vec![track.clone()],
            ..Default::default()
        });
        assert!(render(&mut state, 160, 55).contains(&format!("{marker} Detailed Song")));
        state.search.tab = SearchTab::Global;
        assert!(render(&mut state, 160, 55).contains(&format!("{marker} Detailed Song")));
        state.view = View::Similar;
        state.similar.mode = textamp::app::state::SimilarMode::Tracks;
        state.similar.tracks = vec![track.clone()];
        assert!(render(&mut state, 160, 55).contains(&format!("{marker} Detailed Song")));
        state.view = View::Browse;
        state.view = View::Related;
        state.artist_overview = Some(textamp::app::state::ArtistOverviewState {
            rankings: Default::default(),
            from_label_album: false,
            artist: Default::default(),
            albums: vec![],
            tracks: vec![track.clone()],
            album_tracks: vec![track.clone()],
            selected_album: Some(state.library.albums[0].clone()),
            album_loading: false,
            album_error: None,
            tab: textamp::app::state::ArtistOverviewTab::Albums,
            index: 0,
            scroll_pin: None,
            return_view: View::Browse,
        });
        state.artist_nav.columns.clear();
        assert!(render(&mut state, 160, 55).contains(&format!("{marker} Detailed Song")));
        state.artist_overview = None;
        state.view = View::Browse;
        state.popups.adventure_launcher = Some(AdventureLauncherState {
            step: AdventureStep::FindStartTrack,
            query: String::new(),
            results: None,
            focus: SearchFocus::Results,
            item_index: 0,
            loading: false,
            drill: AdventureDrillLevel::AlbumTracks {
                album_key: "album".into(),
                album_title: "Album".into(),
                artist_name: "Performer".into(),
                tracks: vec![track],
            },
            start_track: None,
            end_track: None,
            track_count_input: "10".into(),
            scroll_pin: None,
            search_tab: SearchTab::Tracks,
        });
        assert!(render(&mut state, 160, 55).contains(&format!("{marker} Detailed Song")));
    }
}

#[test]
fn restoring_column_order_keeps_updated_favorite_status() {
    let mut state = fixture();
    let column = &mut state.artist_nav.columns[0];
    let key = column.tracks[0].rating_key.clone();
    column.shuffle();
    column.update_track_favorite(&key, false);
    column.unshuffle();
    assert_eq!(column.tracks[0].favorited, Some(false));
}

#[test]
fn only_favorited_tracks_receive_a_heart() {
    use textamp::ui::widgets::track_list::{favorite_marker, favorite_title};
    let mut state = fixture();
    for status in [Some(false), None] {
        state.queue.tracks[0].favorited = status;
        assert_eq!(favorite_marker(status), "");
        assert_eq!(favorite_title(&state.queue.tracks[0]), "Detailed Song");
        state.view = View::NowPlaying;
        assert!(!render(&mut state, 120, 40).contains('♡'));
    }
    state.queue.tracks[0].favorited = Some(true);
    assert_eq!(favorite_title(&state.queue.tracks[0]), "♥ Detailed Song");
}

#[tokio::test]
async fn full_details_menu_uses_selected_track_and_closes_without_playing() {
    use textamp::app::{action::SystemAction, command_palette, handlers::dispatch_system};
    let mut state = fixture();
    state.view = View::Queue;
    let mut selected = state.queue.tracks[0].clone();
    selected.title = "Selected song".into();
    selected.rating_key = "selected".into();
    state.queue.tracks.push(selected);
    state.list_state.queue_index = 1;
    let entry = command_palette::materialize_entries(&state)
        .into_iter()
        .find(|e| e.label == "View track details")
        .unwrap();
    let mut config = Default::default();
    let (tx, _rx) = tokio::sync::mpsc::channel(8);
    for action in command_palette::run(entry.command, &mut state) {
        let Action::System(action) = action else {
            panic!("Unexpected playback action");
        };
        dispatch_system::dispatch(&tx, &mut config, action, &mut state)
            .await
            .unwrap();
    }
    let popup = state.popups.text.as_ref().unwrap();
    for expected in [
        "Selected song",
        "flac",
        "950 kbps",
        "96000 Hz",
        "24 bit",
        "12345678 bytes",
        "musicBrainzId: test-recording-id",
        "bpm: 120",
    ] {
        assert!(popup.text.contains(expected), "Missing {expected}");
    }
    let request_id = popup.request_id;
    key_input::handle_key(
        KeyEvent::new(KeyCode::Down, KeyModifiers::NONE),
        &mut state,
        &config,
    );
    assert_eq!(state.popups.text.as_ref().unwrap().scroll, 1);
    assert_eq!(state.list_state.queue_index, 1);
    key_input::handle_key(
        KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE),
        &mut state,
        &config,
    );
    dispatch_system::dispatch(
        &tx,
        &mut config,
        SystemAction::TrackDetailsLoaded {
            generation: state.library_generation,
            request_id,
            text: "stale".into(),
        },
        &mut state,
    )
    .await
    .unwrap();
    assert!(state.popups.text.is_none());
    assert_eq!(state.queue.index, Some(0));
}

#[test]
fn menu_groups_navigation_and_keeps_clicks_aligned_after_scrolling() {
    use crossterm::event::MouseButton;
    use textamp::app::command_palette;
    let mut state = fixture();
    state.view = View::Queue;
    command_palette::open(&mut state);
    let screen = render(&mut state, 100, 38);
    assert!(screen.contains("SELECTION"));
    assert!(screen.contains("Go to album: Album"));
    for selected in [0, 12, state.palette.matches.len() - 1] {
        state.palette.selected = selected;
        render(&mut state, 80, 20);
        let regions = state.hit_regions.command_palette.clone().unwrap();
        assert!(regions.rows.iter().any(|(_, idx)| *idx == selected));
        for (rect, idx) in &regions.rows {
            assert!(rect.bottom() <= regions.outer.bottom());
            assert!(*idx < state.palette.matches.len());
        }
    }
    command_palette::open_with_query(&mut state, "codec");
    let screen = render(&mut state, 80, 20);
    assert!(screen.contains("View track details"));
    let (rect, _) = state.hit_regions.command_palette.as_ref().unwrap().rows[0];
    let actions = mouse_input::handle_mouse(
        MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: rect.x + 1,
            row: rect.y,
            modifiers: KeyModifiers::NONE,
        },
        &mut state,
    );
    assert!(
        matches!(actions.as_slice(), [Action::System(textamp::app::action::SystemAction::ShowTrackDetails(track))] if track.title == "Detailed Song")
    );
}

#[test]
fn terminal_media_keys_use_idempotent_playback_actions_even_with_a_menu_open() {
    use crossterm::event::MediaKeyCode as M;
    use textamp::app::{action::PlaybackAction, state::PlayStatus};
    let mut state = fixture();
    state.palette.open = true;
    state.playback.status = PlayStatus::Playing;
    let config = Default::default();
    assert!(key_input::handle_key(
        KeyEvent::new(KeyCode::Media(M::Play), KeyModifiers::NONE),
        &mut state,
        &config
    )
    .is_empty());
    for media in [M::Pause, M::PlayPause] {
        assert!(matches!(
            key_input::handle_key(
                KeyEvent::new(KeyCode::Media(media), KeyModifiers::NONE),
                &mut state,
                &config
            )
            .as_slice(),
            [Action::Playback(PlaybackAction::TogglePlayPause)]
        ));
    }
    assert!(matches!(
        key_input::handle_key(
            KeyEvent::new(KeyCode::Media(M::TrackNext), KeyModifiers::NONE),
            &mut state,
            &config
        )
        .as_slice(),
        [Action::Playback(PlaybackAction::Next)]
    ));
    assert!(state.palette.open);
}

#[tokio::test]
async fn local_details_probe_tags_and_audio_without_starting_playback() {
    use textamp::app::{action::SystemAction, handlers::dispatch_system, Event};
    use textamp::library::{models::Track, FolderLocation, FolderSource};
    let folder = tempfile::tempdir().unwrap();
    let mut wav = b"RIFF\0\0\0\0WAVEfmt ".to_vec();
    wav.extend(16_u32.to_le_bytes());
    wav.extend(1_u16.to_le_bytes());
    wav.extend(1_u16.to_le_bytes());
    wav.extend(44100_u32.to_le_bytes());
    wav.extend(88200_u32.to_le_bytes());
    wav.extend(2_u16.to_le_bytes());
    wav.extend(16_u16.to_le_bytes());
    let title = b"Tagged Song\0";
    let mut info = b"INFOINAM".to_vec();
    info.extend((title.len() as u32).to_le_bytes());
    info.extend(title);
    wav.extend(b"LIST");
    wav.extend((info.len() as u32).to_le_bytes());
    wav.extend(info);
    wav.extend(b"data");
    wav.extend(200_u32.to_le_bytes());
    wav.resize(wav.len() + 200, 0);
    let size = wav.len();
    wav[4..8].copy_from_slice(&((size - 8) as u32).to_le_bytes());
    std::fs::write(folder.path().join("fixture.wav"), wav).unwrap();
    let mut config = textamp::config::Config {
        folder_sources: vec![FolderSource {
            id: "local".into(),
            name: "Fixture".into(),
            location: FolderLocation::Local {
                path: folder.path().into(),
            },
        }],
        ..Default::default()
    };
    let mut state = AppState::new();
    let (tx, mut rx) = tokio::sync::mpsc::channel(8);
    dispatch_system::dispatch(
        &tx,
        &mut config,
        SystemAction::ShowTrackDetails(Box::new(Track::from_folder("local", "fixture.wav"))),
        &mut state,
    )
    .await
    .unwrap();
    assert!(state
        .popups
        .text
        .as_ref()
        .unwrap()
        .text
        .contains("Reading local file tags"));
    let event = tokio::time::timeout(std::time::Duration::from_secs(5), rx.recv())
        .await
        .unwrap()
        .unwrap();
    let Event::Effect(Action::System(action)) = event else {
        panic!("Expected metadata completion");
    };
    dispatch_system::dispatch(&tx, &mut config, action, &mut state)
        .await
        .unwrap();
    let text = &state.popups.text.as_ref().unwrap().text;
    for expected in [
        "Tagged Song",
        "44100 Hz",
        "16 bit",
        "Channels: 1",
        "Container: wav",
        &format!("{size} bytes"),
    ] {
        assert!(text.contains(expected), "Missing {expected}: {text}");
    }
    assert_eq!(
        state.playback.status,
        textamp::app::state::PlayStatus::Stopped
    );
    assert!(state.queue.tracks.is_empty());
}

#[test]
fn folder_actions_target_the_selected_file_and_filter_directories_from_playback() {
    use textamp::app::{action::QueueAction, command_palette, sources::ActiveSource};
    use textamp::library::{
        folder::{FolderColumn, FolderItem, FolderNavigationState},
        models::Track,
    };
    let mut state = AppState::new();
    state.view = View::Browse;
    state.browse_category = BrowseCategory::Folders;
    state.category_column_focused = false;
    state.sources.active = ActiveSource::Folder("fixture".into());
    let mut items = vec![FolderItem::folder("subfolder".into(), "subfolder".into())];
    for path in ["a.wav", "z.wav"] {
        items.push(FolderItem::track(
            path.into(),
            path.into(),
            Track::from_folder("fixture", path).rating_key,
            None,
            None,
            None,
        ));
    }
    let mut column = FolderColumn::new(None, "Files".into(), items);
    column.selected_index = 2;
    let mut nav = FolderNavigationState::for_library("folder:fixture".into());
    nav.columns.push(column);
    state.folder_state = Some(nav);
    let entries = command_palette::materialize_entries(&state);
    let details = entries
        .iter()
        .find(|entry| entry.label == "View track details")
        .unwrap();
    assert!(
        matches!(command_palette::run(details.command.clone(), &mut state).as_slice(), [Action::System(textamp::app::action::SystemAction::ShowTrackDetails(track))] if track.file_name() == Some("z.wav"))
    );
    let play = entries
        .iter()
        .find(|entry| entry.label == "Play track and following")
        .unwrap();
    assert!(
        matches!(command_palette::run(play.command.clone(), &mut state).as_slice(), [Action::Queue(QueueAction::PlayCatalog { tracks, index: 1 })] if tracks.len() == 2 && tracks[0].file_name() == Some("a.wav") && tracks[1].file_name() == Some("z.wav"))
    );
    state.folder_state.as_mut().unwrap().columns[0].selected_index = 0;
    assert!(state.palette_target_track().is_none());
}
