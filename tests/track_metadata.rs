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
        .find(|entry| entry.label == "Go to Artist Page")
        .unwrap();
    assert!(
        matches!(textamp::app::command_palette::run(entry.command, &mut state).as_slice(), [Action::Browse(textamp::app::action::BrowseAction::OpenArtistOverview { artist_key })] if artist_key == "performer")
    );
    state.view = View::Settings;
    assert!(!textamp::app::command_palette::materialize_entries(&state)
        .iter()
        .any(|entry| entry.label == "Go to Artist Page"));
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
