use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{backend::TestBackend, Terminal};
use textamp::app::action::*;
use textamp::app::dispatch::handle_core_event;
use textamp::app::handlers::{dispatch_browse, key_input};
use textamp::app::state::{
    ArtistOverviewState, ArtistOverviewTab, BrowseCategory, BrowseColumn, BrowseItem,
    MillerLayoutMode, View,
};
use textamp::app::{AppState, Event};
use textamp::library::models::{Album, Artist, Media, Track};
use tokio::sync::mpsc;
mod common;

#[test]
fn transcoded_playback_requests_a_supported_codec_and_original_stays_raw() {
    let state = common::navidrome_state();
    let session = state.sources.active.navidrome().unwrap();
    for (bitrate, format) in [(0, "raw"), (128, "mp3"), (320, "mp3")] {
        let url = session.playback_url("song & 1", bitrate).unwrap();
        let params: std::collections::HashMap<_, _> = url.query_pairs().collect();
        assert_eq!(params.get("format").map(|s| s.as_ref()), Some(format));
        assert_eq!(params.get("id").map(|s| s.as_ref()), Some("song & 1"));
        assert_eq!(params.contains_key("maxBitRate"), bitrate > 0);
        if bitrate > 0 {
            assert_eq!(params["maxBitRate"], bitrate.to_string());
        }
    }
}

#[test]
fn provider_tags_survive_normalization_and_old_cache_defaults() {
    let state = common::navidrome_state();
    let session = state.sources.active.navidrome().unwrap();
    let raw = serde_json::from_value(serde_json::json!({
        "id":"album", "name":"Album", "recordLabels":[{"name":"Parlophone"}],
        "releaseDate":{"year":2009,"month":9,"day":9}, "originalReleaseDate":{"year":1964},
        "releaseCountry":"GB", "releaseTypes":["album"], "version":"Remaster"
    }))
    .unwrap();
    let album = session.album(raw);
    assert_eq!(album.record_labels, ["Parlophone"]);
    assert_eq!(album.release_date.as_deref(), Some("2009-09-09"));
    assert_eq!(album.originally_available_at.as_deref(), Some("1964"));
    assert_eq!(album.country.as_deref(), Some("GB"));
    let old: Album = serde_json::from_str(r#"{"ratingKey":"old","title":"Old album"}"#).unwrap();
    assert!(old.record_labels.is_empty());
    let song = serde_json::from_value(serde_json::json!({"id":"song", "title":"Song", "suffix":"flac", "bitRate":900, "bitDepth":24, "samplingRate":96000, "channelCount":2})).unwrap();
    let track = session.track(song);
    assert_eq!(track.media[0].bit_depth, Some(24));
    assert_eq!(track.media[0].sampling_rate, Some(96000));
    assert_eq!(track.media[0].audio_channels, Some(2));
}

fn page() -> ArtistOverviewState {
    let album = Album {
        rating_key: "album".into(),
        title: "A Test Album".into(),
        parent_title: Some("Artist".into()),
        release_date: Some("2020-03".into()),
        originally_available_at: Some("1964".into()),
        record_labels: vec!["Test Records".into()],
        country: Some("GB".into()),
        leaf_count: Some(1),
        ..Default::default()
    };
    let track = Track {
        rating_key: "song".into(),
        parent_rating_key: Some("album".into()),
        title: "A Test Song".into(),
        thumb: Some("cover".into()),
        duration: Some(123_000),
        media: vec![Media {
            audio_codec: Some("flac".into()),
            bitrate: Some(900),
            bit_depth: Some(24),
            sampling_rate: Some(96000),
            ..Default::default()
        }],
        ..Default::default()
    };
    ArtistOverviewState {
        rankings: Default::default(),
        from_label_album: false,
        artist: Artist {
            rating_key: "artist".into(),
            title: "Artist".into(),
            thumb: Some("artist-cover".into()),
            ..Default::default()
        },
        albums: vec![album],
        tracks: vec![track],
        album_tracks: vec![],
        selected_album: None,
        album_loading: false,
        album_error: None,
        tab: ArtistOverviewTab::Albums,
        index: 0,
        scroll_pin: None,
        return_view: View::Browse,
    }
}

#[test]
fn overview_requests_and_renders_album_art_with_track_fallback_and_metadata() {
    let mut state = AppState::new();
    state.view = View::Related;
    state.artist_overview = Some(page());
    let (tx, _rx) = mpsc::channel(8);
    let actions = handle_core_event(Event::Tick, &mut state, &tx);
    assert!(actions.iter().any(|action| matches!(action, Action::System(SystemAction::LoadAlbumArt(batch)) if batch == &vec![("album".into(), "cover".into())])));
    let mut png = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgb8(image::RgbImage::from_pixel(32, 32, image::Rgb([255, 0, 0])))
        .write_to(&mut png, image::ImageFormat::Png)
        .unwrap();
    state
        .artwork
        .insert_grid_art("album".into(), png.into_inner());
    // No terminal query/picker initialization: a halfblocks fallback must work.
    let mut terminal = Terminal::new(TestBackend::new(120, 52)).unwrap();
    terminal
        .draw(|frame| {
            let _ = textamp::ui::render(frame, &state);
        })
        .unwrap();
    let buffer = terminal.backend().buffer();
    let text: String = buffer.content.iter().map(|cell| cell.symbol()).collect();
    for expected in [
        "A Test Album",
        "Test Records",
        "2020-03",
        "1964",
        "GB",
        "900 kbps",
        "24 bit",
        "96000 Hz",
    ] {
        assert!(text.contains(expected), "missing {expected}");
    }
    assert!(
        buffer
            .content
            .iter()
            .any(|cell| cell.fg == ratatui::style::Color::Rgb(255, 0, 0)
                || cell.bg == ratatui::style::Color::Rgb(255, 0, 0)),
        "cover should actually produce the fixture's red pixels"
    );
    for (width, height) in [(20, 8), (40, 12), (80, 24)] {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        terminal
            .draw(|frame| {
                let _ = textamp::ui::render(frame, &state);
            })
            .unwrap();
    }
}

#[test]
fn album_enter_refreshes_details_and_song_enter_opens_now_playing() {
    let mut state = AppState::new();
    state.view = View::Related;
    state.artist_overview = Some(page());
    let config = textamp::config::Config::default();
    let enter = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
    let actions = key_input::handle_key(enter, &mut state, &config);
    assert!(
        matches!(actions.as_slice(), [Action::Browse(BrowseAction::LoadOverviewAlbum { album_key })] if album_key == "album")
    );
    let actions = key_input::handle_key(enter, &mut state, &config);
    assert!(
        matches!(actions.as_slice(), [Action::Queue(QueueAction::PlayCatalog { tracks, index: 0 })] if tracks[0].rating_key == "song")
    );
}

#[tokio::test]
async fn late_album_details_cannot_overwrite_new_selection_and_failures_keep_cached_songs() {
    let mut state = AppState::new();
    state.active_library = Some("fixture".into());
    let mut overview = page();
    overview.selected_album = Some(overview.albums[0].clone());
    overview.album_tracks = overview.tracks.clone();
    overview.album_loading = true;
    state.artist_overview = Some(overview);
    let (tx, _rx) = mpsc::channel(8);
    dispatch_browse::dispatch(
        &tx,
        BrowseAction::OverviewAlbumLoaded {
            library_key: "fixture".into(),
            album_key: "other".into(),
            result: Ok((Box::default(), vec![])),
        },
        &mut state,
    )
    .await
    .unwrap();
    assert_eq!(
        state.artist_overview.as_ref().unwrap().album_tracks.len(),
        1
    );
    dispatch_browse::dispatch(
        &tx,
        BrowseAction::OverviewAlbumLoaded {
            library_key: "fixture".into(),
            album_key: "album".into(),
            result: Err(AsyncError {
                message: "Offline".into(),
            }),
        },
        &mut state,
    )
    .await
    .unwrap();
    let page = state.artist_overview.as_ref().unwrap();
    assert_eq!(page.album_tracks.len(), 1);
    assert_eq!(page.album_error.as_deref(), Some("Offline"));
    assert!(!page.album_loading);
}

#[test]
fn two_pane_browse_keeps_the_focused_track_column_visible_in_both_layouts() {
    for layout in [MillerLayoutMode::Shrinking, MillerLayoutMode::Scrolling] {
        let mut state = AppState::new();
        state.view = View::Browse;
        state.browse_category = BrowseCategory::Library;
        state.miller_layout = layout;
        state.category_column_focused = false;
        let track = page().tracks.remove(0);
        state.library.all_tracks = vec![track.clone()];
        state.artist_nav.columns = vec![
            BrowseColumn::new(
                "artists",
                vec![BrowseItem::Artist {
                    key: "artist".into(),
                    title: "Artist".into(),
                    thumb: None,
                    is_placeholder: false,
                }],
            ),
            BrowseColumn::new(
                "albums",
                vec![BrowseItem::Album {
                    key: "album".into(),
                    title: "Album".into(),
                    thumb: None,
                    year: None,
                    artist: "Artist".into(),
                    is_placeholder: false,
                }],
            ),
            BrowseColumn::new(
                "songs",
                BrowseItem::from_tracks(std::slice::from_ref(&track)),
            ),
        ];
        state.artist_nav.columns[2].tracks = vec![track];
        state.artist_nav.focused_column = 2;
        let mut terminal = Terminal::new(TestBackend::new(120, 40)).unwrap();
        let mut feedback = None;
        terminal
            .draw(|frame| {
                feedback = Some(textamp::ui::render(frame, &state));
            })
            .unwrap();
        let regions = feedback.unwrap().hit_regions;
        let cols = &regions.miller_columns.as_ref().unwrap().columns;
        assert!(
            cols.iter()
                .any(|col| col.col_idx == 2 && col.inner.width > 0),
            "focused track list is hidden in {layout:?}"
        );
    }
}

#[test]
fn stopped_playback_keeps_the_underlying_error_visible() {
    let mut state = AppState::new();
    state.consecutive_playback_errors = 8;
    let (tx, _rx) = mpsc::channel(8);
    handle_core_event(
        textamp::app::event::PlaybackEvent::PlaybackError {
            playback_id: None,
            message: "audio server returned HTTP 403 Forbidden".into(),
        }
        .into(),
        &mut state,
        &tx,
    );
    assert!(state
        .notifications
        .last_error
        .as_ref()
        .unwrap()
        .contains("403 Forbidden"));
}

#[test]
fn overview_and_related_album_rows_request_covers_and_keep_thumbnail_clicks_aligned() {
    use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
    use textamp::app::handlers::mouse_input;
    use textamp::app::state::{RelatedArtistGroup, RelatedSource};
    let mut state = common::navidrome_state();
    let mut overview = page();
    overview.albums[0].thumb = Some("album-cover".into());
    state.related.groups = vec![RelatedArtistGroup {
        artist: overview.artist.clone(),
        albums: overview.albums.clone(),
        source: RelatedSource::Navidrome,
    }];
    state.view = View::Related;
    let (tx, _rx) = mpsc::channel(8);
    let requested = handle_core_event(Event::Tick, &mut state, &tx);
    assert!(requested.iter().any(|action| matches!(action, Action::System(SystemAction::LoadAlbumArt(batch)) if batch.contains(&("artist".into(), "artist-cover".into())) && batch.contains(&("album".into(), "album-cover".into())))));
    let mut terminal = Terminal::new(TestBackend::new(120, 40)).unwrap();
    terminal
        .draw(|frame| state.apply_render_feedback(textamp::ui::render(frame, &state)))
        .unwrap();
    let region = state.hit_regions.related_content.as_ref().unwrap().clone();
    assert_eq!(region.rows_per_item, 2);
    mouse_input::handle_mouse(
        MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: region.inner.x + 1,
            row: region.inner.y + 3,
            modifiers: KeyModifiers::NONE,
        },
        &mut state,
    );
    assert_eq!(
        state.list_state.related_index, 1,
        "The album thumbnail's second row must select the album"
    );
    assert_eq!(state.scroll.related, Some(0));
    state.artist_overview = Some(overview);
    let mut terminal = Terminal::new(TestBackend::new(120, 40)).unwrap();
    terminal
        .draw(|frame| state.apply_render_feedback(textamp::ui::render(frame, &state)))
        .unwrap();
    let regions = state.hit_regions.artist_overview.as_ref().unwrap();
    assert!(
        (regions.list.x..regions.list.x + 4)
            .any(|x| terminal.backend().buffer()[(x, regions.items[0].0.y)]
                .symbol()
                .contains('▧')),
        "Missing covers need a visible placeholder in every album row"
    );
}

#[tokio::test]
async fn labels_browse_cached_albums_artists_and_return_from_album_without_playing() {
    let mut state = common::navidrome_state();
    let fixture = page();
    state.view = View::Browse;
    state.library.artists = vec![fixture.artist.clone()];
    state.library.albums = fixture.albums.clone();
    state.library.all_tracks = fixture.tracks.clone();
    state.library.albums[0]
        .record_labels
        .push(" test records ".into());
    let (tx, _rx) = mpsc::channel(8);
    dispatch_browse::dispatch(&tx, BrowseAction::OpenLabels, &mut state)
        .await
        .unwrap();
    assert_eq!(state.label_overview.as_ref().unwrap().labels.len(), 1);
    let open = key_input::handle_key(
        KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
        &mut state,
        &Default::default(),
    );
    assert!(matches!(
        open.as_slice(),
        [Action::Browse(BrowseAction::OpenLabelOverview { .. })]
    ));
    dispatch_browse::dispatch(
        &tx,
        BrowseAction::OpenLabelOverview {
            label: "TEST RECORDS".into(),
        },
        &mut state,
    )
    .await
    .unwrap();
    assert_eq!(state.label_overview.as_ref().unwrap().albums.len(), 1);
    assert_eq!(state.label_overview.as_ref().unwrap().artists.len(), 1);
    let mut terminal = Terminal::new(TestBackend::new(120, 50)).unwrap();
    terminal
        .draw(|frame| state.apply_render_feedback(textamp::ui::render(frame, &state)))
        .unwrap();
    let region = state.hit_regions.label_overview.clone().unwrap();
    let tab = region.tab_button.unwrap();
    let actions = textamp::app::handlers::mouse_input::handle_mouse(
        crossterm::event::MouseEvent {
            kind: crossterm::event::MouseEventKind::Down(crossterm::event::MouseButton::Left),
            column: tab.x,
            row: tab.y,
            modifiers: KeyModifiers::NONE,
        },
        &mut state,
    );
    assert!(actions.is_empty());
    assert!(state.label_overview.as_ref().unwrap().artists_tab);
    let mut another_album = state.library.albums[0].clone();
    another_album.rating_key = "other-album".into();
    another_album.parent_title = Some("Other Artist".into());
    another_album.parent_rating_key = Some("other-artist".into());
    state.library.albums.insert(0, another_album);
    state.library.artists.insert(
        0,
        Artist {
            rating_key: "other-artist".into(),
            title: "Other Artist".into(),
            ..Default::default()
        },
    );
    state.label_overview.as_mut().unwrap().scroll_pin = Some(0);
    dispatch_browse::refresh_label_overview(&mut state);
    assert_eq!(state.label_overview.as_ref().unwrap().index, 1);
    assert_eq!(state.label_overview.as_ref().unwrap().scroll_pin, Some(0));
    let refresh = key_input::handle_key(
        KeyEvent::new(KeyCode::F(5), KeyModifiers::NONE),
        &mut state,
        &Default::default(),
    );
    assert!(matches!(
        refresh.as_slice(),
        [Action::System(SystemAction::RefreshCategory(_))]
    ));
    let artist = key_input::handle_key(
        KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
        &mut state,
        &Default::default(),
    );
    assert!(
        matches!(artist.as_slice(), [Action::Browse(BrowseAction::OpenArtistOverview { artist_key })] if artist_key == "artist")
    );
    key_input::handle_key(
        KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE),
        &mut state,
        &Default::default(),
    );
    let actions = dispatch_browse::dispatch(
        &tx,
        BrowseAction::OpenLabelAlbum {
            album_key: "album".into(),
        },
        &mut state,
    )
    .await
    .unwrap();
    assert!(matches!(
        actions.as_slice(),
        [Action::Browse(BrowseAction::LoadOverviewAlbum { .. })]
    ));
    assert_eq!(
        state.artist_overview.as_ref().unwrap().album_tracks[0].rating_key,
        "song"
    );
    let back = key_input::handle_key(
        KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE),
        &mut state,
        &Default::default(),
    );
    assert!(back.is_empty());
    assert!(state.artist_overview.is_none());
    assert!(state.label_overview.is_some());
    key_input::handle_key(
        KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE),
        &mut state,
        &Default::default(),
    );
    assert!(state.label_overview.as_ref().unwrap().albums.is_empty());
    assert!(!textamp::app::command_palette::materialize_entries(&state)
        .iter()
        .any(|entry| entry.label == "Go to Artist Page"));
    assert!(state.queue.tracks.is_empty());
}

#[test]
fn most_played_rankings_use_reported_counts_and_visible_order_for_playback() {
    use textamp::app::state::ArtistRanking;
    let mut overview = page();
    overview.tracks[0].view_count = Some(2);
    let mut popular = overview.tracks[0].clone();
    popular.rating_key = "popular".into();
    popular.title = "Popular song".into();
    popular.parent_rating_key = Some("popular-album".into());
    popular.view_count = Some(20);
    overview.tracks.push(popular);
    let mut album = overview.albums[0].clone();
    album.rating_key = "popular-album".into();
    album.title = "Popular album".into();
    album.play_count = Some(50);
    overview.albums.push(album);
    overview.set_ranking(ArtistRanking::MostPlayed);
    assert_eq!(overview.display_albums()[0].rating_key, "popular-album");
    assert_eq!(overview.display_tracks()[0].rating_key, "popular");
    assert_eq!(overview.album_plays(&overview.albums[0]), Some(2));
    overview.tab = ArtistOverviewTab::Songs;
    let mut state = common::navidrome_state();
    state.view = View::Related;
    state.artist_overview = Some(overview);
    assert_eq!(state.palette_target_track().unwrap().rating_key, "popular");
    let play = key_input::handle_key(
        KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
        &mut state,
        &Default::default(),
    );
    assert!(
        matches!(play.first(), Some(Action::Queue(QueueAction::PlayCatalog { tracks, index: 0 })) if tracks[0].rating_key == "popular" && tracks.len() == 2)
    );
    state
        .artist_overview
        .as_mut()
        .unwrap()
        .set_ranking(ArtistRanking::Catalog);
    assert_eq!(
        state.artist_overview.as_ref().unwrap().display_tracks()[0].rating_key,
        "song"
    );
}

#[tokio::test]
async fn community_completion_rejects_other_library_or_page_and_preserves_chart_on_error() {
    use textamp::app::state::ArtistRanking;
    let mut state = common::navidrome_state();
    state.active_library = Some("current".into());
    state.artist_overview_request_id = 5;
    state.artist_overview = Some(page());
    state
        .artist_overview
        .as_mut()
        .unwrap()
        .set_ranking(ArtistRanking::Community);
    let (tx, _rx) = mpsc::channel(8);
    for (library, request_id) in [("old", 5), ("current", 4)] {
        dispatch_browse::dispatch(
            &tx,
            BrowseAction::ArtistCommunityLoaded {
                library_key: library.into(),
                artist_key: "artist".into(),
                request_id,
                result: Ok(page().tracks),
            },
            &mut state,
        )
        .await
        .unwrap();
        assert!(state
            .artist_overview
            .as_ref()
            .unwrap()
            .display_tracks()
            .is_empty());
    }
    dispatch_browse::dispatch(
        &tx,
        BrowseAction::ArtistCommunityLoaded {
            library_key: "current".into(),
            artist_key: "artist".into(),
            request_id: 5,
            result: Ok(page().tracks),
        },
        &mut state,
    )
    .await
    .unwrap();
    let chart = state.artist_overview.as_ref().unwrap();
    assert_eq!(chart.display_tracks().len(), 1);
    assert_eq!(chart.display_albums().len(), 1);
    dispatch_browse::dispatch(
        &tx,
        BrowseAction::ArtistCommunityLoaded {
            library_key: "current".into(),
            artist_key: "artist".into(),
            request_id: 5,
            result: Err(AsyncError {
                message: "Popularity provider unavailable".into(),
            }),
        },
        &mut state,
    )
    .await
    .unwrap();
    assert_eq!(
        state
            .artist_overview
            .as_ref()
            .unwrap()
            .display_tracks()
            .len(),
        1
    );
    assert!(state
        .artist_overview
        .as_ref()
        .unwrap()
        .rankings
        .error
        .is_some());
}

#[test]
fn label_palette_links_use_the_visible_album_and_rank_controls_are_clickable() {
    let mut state = common::navidrome_state();
    state.view = View::Related;
    state.artist_overview = Some(page());
    let entries = textamp::app::command_palette::materialize_entries(&state);
    assert!(entries
        .iter()
        .any(|entry| entry.label == "Browse Record Labels"));
    assert!(entries
        .iter()
        .any(|entry| entry.label == "Go to Label: Test Records"));
    let mut terminal = Terminal::new(TestBackend::new(120, 52)).unwrap();
    terminal
        .draw(|frame| state.apply_render_feedback(textamp::ui::render(frame, &state)))
        .unwrap();
    let button = state
        .hit_regions
        .artist_overview
        .as_ref()
        .unwrap()
        .ranking_button
        .unwrap();
    let actions = textamp::app::handlers::mouse_input::handle_mouse(
        crossterm::event::MouseEvent {
            kind: crossterm::event::MouseEventKind::Down(crossterm::event::MouseButton::Left),
            column: button.x,
            row: button.y,
            modifiers: KeyModifiers::NONE,
        },
        &mut state,
    );
    assert!(matches!(
        actions.as_slice(),
        [Action::Browse(BrowseAction::SelectArtistTab(
            textamp::app::state::ArtistOverviewTab::TopPlayed
        ))]
    ));
}

#[tokio::test]
async fn artist_tabs_are_visible_clickable_and_related_shortcuts_ignore_the_playing_artist() {
    use textamp::library::models::GenreTag;
    let mut state = common::navidrome_state();
    let mut fixture = page();
    fixture.artist.title = "Julian Cope".into();
    fixture.artist.genre = vec![GenreTag {
        tag: "Rock".into(),
        ..Default::default()
    }];
    let neighbor = Artist {
        rating_key: "neighbor".into(),
        title: "Rock neighbor".into(),
        genre: fixture.artist.genre.clone(),
        ..Default::default()
    };
    let playing_artist = Artist {
        rating_key: "playing".into(),
        title: "Kendrick Lamar".into(),
        genre: vec![GenreTag {
            tag: "Hip hop".into(),
            ..Default::default()
        }],
        ..Default::default()
    };
    state.library.artists = vec![fixture.artist.clone(), neighbor, playing_artist];
    state.queue.tracks = vec![Track {
        rating_key: "playing-song".into(),
        title: "Playing song".into(),
        grandparent_rating_key: Some("playing".into()),
        ..Default::default()
    }];
    state.queue.index = Some(0);
    state.set_playback_mode(textamp::app::state::PlaybackMode::Queue);
    state.artist_overview = Some(fixture);
    state.view = View::Related;
    for key in [
        KeyEvent::new(KeyCode::Char('r'), KeyModifiers::NONE),
        KeyEvent::new(KeyCode::Char('r'), KeyModifiers::CONTROL),
    ] {
        let actions = key_input::handle_key(key, &mut state, &Default::default());
        assert!(matches!(
            actions.as_slice(),
            [Action::Browse(BrowseAction::SelectArtistTab(
                textamp::app::state::ArtistOverviewTab::RelatedServer
            ))]
        ));
    }
    let (tx, _rx) = mpsc::channel(8);
    dispatch_browse::dispatch(
        &tx,
        BrowseAction::SelectArtistTab(textamp::app::state::ArtistOverviewTab::RelatedTags),
        &mut state,
    )
    .await
    .unwrap();
    let visible = state.artist_overview.as_ref().unwrap();
    assert_eq!(visible.artist.title, "Julian Cope");
    assert_eq!(visible.rankings.relations.artists.len(), 1);
    assert_eq!(
        visible.rankings.relations.artists[0].artist.rating_key,
        "neighbor"
    );
    assert!(visible.rankings.relations.artists[0]
        .reason
        .contains("rock"));
    assert!(
        state.palette_target_track().is_none(),
        "artist tabs must not expose a hidden song selection"
    );
    let mut terminal = Terminal::new(TestBackend::new(80, 50)).unwrap();
    terminal
        .draw(|frame| state.apply_render_feedback(textamp::ui::render(frame, &state)))
        .unwrap();
    let tabs = &state.hit_regions.artist_overview.as_ref().unwrap().tabs;
    assert_eq!(tabs.len(), 8);
    assert!(tabs
        .iter()
        .all(|(area, _)| area.right() <= 80 && area.bottom() <= 50));
    let button = tabs
        .iter()
        .find(|(_, tab)| *tab == textamp::app::state::ArtistOverviewTab::Community)
        .unwrap()
        .0;
    let action = textamp::app::handlers::mouse_input::handle_mouse(
        crossterm::event::MouseEvent {
            kind: crossterm::event::MouseEventKind::Down(crossterm::event::MouseButton::Left),
            column: button.x,
            row: button.y,
            modifiers: KeyModifiers::NONE,
        },
        &mut state,
    );
    assert!(matches!(
        action.as_slice(),
        [Action::Browse(BrowseAction::SelectArtistTab(
            textamp::app::state::ArtistOverviewTab::Community
        ))]
    ));
}

#[test]
fn release_sections_use_primary_secondary_and_legacy_metadata_without_title_guesses() {
    use textamp::library::catalog::AlbumReleaseGroup::*;
    for (types, expected) in [
        (vec!["album"], Albums),
        (vec!["single"], Singles),
        (vec!["EP"], Eps),
        (vec!["album", "compilation"], Compilations),
        (vec!["album", "live"], Live),
        (vec!["single", "live"], Live),
        (vec!["album; compilation; live"], Live),
        (vec!["soundtrack"], Soundtracks),
        (vec!["album", "remix"], Remixes),
        (vec!["dj-mix"], DjMixes),
        (vec!["spokenword"], Other),
        (vec![], Albums),
    ] {
        let album = Album {
            release_types: types.into_iter().map(String::from).collect(),
            ..Default::default()
        };
        assert_eq!(album.release_group(), expected);
    }
    let legacy: Album = serde_json::from_str(
        r#"{"ratingKey":"legacy","title":"Live and Singles","subtype":"compilation"}"#,
    )
    .unwrap();
    assert_eq!(legacy.release_group(), Compilations);
    let no_tags = Album {
        title: "Live at the Singles Club".into(),
        ..Default::default()
    };
    assert_eq!(
        no_tags.release_group(),
        Albums,
        "titles are not release-type metadata"
    );
}

#[tokio::test]
async fn artist_release_sections_keep_headers_out_of_selection_and_cards_aligned_with_artwork() {
    use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
    use textamp::app::handlers::mouse_input;
    let mut state = common::navidrome_state();
    let mut fixture = page();
    fixture.albums = [
        ("live", "live"),
        ("single", "single"),
        ("album", "album"),
        ("compilation", "compilation"),
        ("ep", "ep"),
    ]
    .into_iter()
    .enumerate()
    .map(|(number, (key, kind))| Album {
        rating_key: key.into(),
        title: format!("Release {number}"),
        parent_rating_key: Some(fixture.artist.rating_key.clone()),
        release_types: vec![kind.into()],
        play_count: Some(number as u64),
        ..Default::default()
    })
    .collect();
    state.library.artists = vec![fixture.artist.clone()];
    state.library.albums = fixture.albums.clone();
    let (tx, _rx) = mpsc::channel(8);
    dispatch_browse::dispatch(
        &tx,
        BrowseAction::OpenArtistOverview {
            artist_key: fixture.artist.rating_key.clone(),
        },
        &mut state,
    )
    .await
    .unwrap();
    let keys = |state: &AppState| {
        state
            .artist_overview
            .as_ref()
            .unwrap()
            .display_albums()
            .iter()
            .map(|a| a.rating_key.clone())
            .collect::<Vec<_>>()
    };
    assert_eq!(
        keys(&state),
        ["album", "ep", "single", "compilation", "live"]
    );
    let mut terminal = Terminal::new(TestBackend::new(120, 50)).unwrap();
    terminal
        .draw(|frame| state.apply_render_feedback(textamp::ui::render(frame, &state)))
        .unwrap();
    let regions = state.hit_regions.artist_overview.clone().unwrap();
    assert_eq!(regions.items.len(), 5);
    for (rect, index) in &regions.items {
        let expected = state.artist_overview.as_ref().unwrap().display_albums()[*index]
            .release_group()
            .label();
        let line: String = (regions.list.x..regions.list.right())
            .map(|x| terminal.backend().buffer()[(x, rect.y - 1)].symbol())
            .collect();
        assert!(line.contains(expected), "{line:?} lacks {expected}");
        assert!(
            (rect.x..rect.x + 4).any(|x| terminal.backend().buffer()[(x, rect.y)]
                .symbol()
                .contains('▧'))
        );
    }
    let target = regions.items[2].0;
    let click = |state: &mut AppState, row| {
        mouse_input::handle_mouse(
            MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column: target.x + 1,
                row,
                modifiers: KeyModifiers::NONE,
            },
            state,
        )
    };
    assert!(click(&mut state, target.y - 1).is_empty());
    assert_eq!(
        state.artist_overview.as_ref().unwrap().index,
        0,
        "section header is not selectable"
    );
    assert!(click(&mut state, target.y + 1).is_empty());
    assert_eq!(state.artist_overview.as_ref().unwrap().index, 2);
    assert_eq!(state.artist_overview.as_ref().unwrap().scroll_pin, Some(0));
    let actions = key_input::handle_key(
        KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
        &mut state,
        &Default::default(),
    );
    assert!(
        matches!(actions.as_slice(), [textamp::app::Action::Browse(BrowseAction::LoadOverviewAlbum {album_key})] if album_key == "single")
    );
    // Return to releases and move to a card beyond a short viewport.
    key_input::handle_key(
        KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE),
        &mut state,
        &Default::default(),
    );
    key_input::handle_key(
        KeyEvent::new(KeyCode::End, KeyModifiers::NONE),
        &mut state,
        &Default::default(),
    );
    assert_eq!(state.artist_overview.as_ref().unwrap().index, 4);
    assert_eq!(state.artist_overview.as_ref().unwrap().scroll_pin, None);
    let mut short = Terminal::new(TestBackend::new(80, 24)).unwrap();
    short
        .draw(|frame| state.apply_render_feedback(textamp::ui::render(frame, &state)))
        .unwrap();
    let shown = state.hit_regions.artist_overview.as_ref().unwrap();
    assert!(shown
        .items
        .iter()
        .any(|(rect, index)| *index == 4 && rect.bottom() <= shown.list.bottom()));
    dispatch_browse::dispatch(
        &tx,
        BrowseAction::SelectArtistTab(ArtistOverviewTab::TopAlbums),
        &mut state,
    )
    .await
    .unwrap();
    assert_eq!(
        keys(&state),
        ["ep", "compilation", "album", "single", "live"],
        "ranked tab retains global play-count ordering"
    );
}

#[tokio::test]
async fn artist_credited_releases_have_an_appears_in_section_and_open_normally() {
    let mut state = common::navidrome_state();
    let fixture = page();
    let mut appearance = fixture.albums[0].clone();
    appearance.rating_key = "appearance".into();
    appearance.parent_rating_key = Some("various-artists".into());
    appearance.parent_title = Some("Various Artists".into());
    appearance.release_types = vec!["album".into(), "compilation".into()];
    let mut credited = fixture.tracks[0].clone();
    credited.parent_rating_key = Some(appearance.rating_key.clone());
    credited.grandparent_rating_key = Some(fixture.artist.rating_key.clone());
    state.library.artists = vec![fixture.artist.clone()];
    state.library.albums = vec![appearance];
    state.library.all_tracks = vec![credited];
    let (tx, _rx) = mpsc::channel(8);
    dispatch_browse::dispatch(
        &tx,
        BrowseAction::OpenArtistOverview {
            artist_key: fixture.artist.rating_key,
        },
        &mut state,
    )
    .await
    .unwrap();
    let page = state.artist_overview.as_ref().unwrap();
    assert_eq!(page.display_albums().len(), 1);
    assert_eq!(
        page.release_section(&page.display_albums()[0]).label(),
        "Appears in"
    );
    let mut terminal = Terminal::new(TestBackend::new(120, 40)).unwrap();
    terminal
        .draw(|frame| state.apply_render_feedback(textamp::ui::render(frame, &state)))
        .unwrap();
    let regions = state.hit_regions.artist_overview.as_ref().unwrap();
    let line: String = (regions.list.x..regions.list.right())
        .map(|x| terminal.backend().buffer()[(x, regions.list.y)].symbol())
        .collect();
    assert!(line.contains("Appears in (1)"));
    let actions = key_input::handle_key(
        KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
        &mut state,
        &Default::default(),
    );
    assert!(
        matches!(actions.as_slice(), [textamp::app::Action::Browse(BrowseAction::LoadOverviewAlbum {album_key})] if album_key == "appearance")
    );
}

#[tokio::test]
async fn individual_featured_credits_populate_artist_appearances_and_joint_albums_stay_owned() {
    use textamp::services::compilations::ArtistIndex;
    let mut state = common::navidrome_state();
    let session = state.sources.active.navidrome().unwrap().clone();
    let artists: Vec<_> = [
        ("cudi", "Kid Cudi"),
        ("mgmt", "MGMT"),
        ("kanye", "Kanye West"),
        ("tyler", "Tyler, The Creator"),
    ]
    .into_iter()
    .map(|(id, name)| {
        session.artist(serde_json::from_value(serde_json::json!({"id":id, "name":name})).unwrap())
    })
    .collect();
    let normalize = |id: &str, credits: serde_json::Value| {
        session.track(serde_json::from_value(serde_json::json!({
        "id":id, "title":id, "albumId":"cudi-album", "artistId":"cudi", "artist":"Kid Cudi feat. MGMT & Kanye West", "artists": credits,
        "albumArtists":[{"id":"cudi","name":"Kid Cudi"}],
        "contributors":[{"role":"composer","artist":{"id":"composer","name":"Unrelated composer"}}]
    })).unwrap())
    };
    let pursuit = normalize(
        "pursuit",
        serde_json::json!([{"id":"cudi","name":"Kid Cudi"},{"id":"mgmt","name":"MGMT"},{"id":"kanye","name":"Kanye West"},{"id":"tyler","name":"Tyler, The Creator"}]),
    );
    assert_eq!(pursuit.artist_credits.len(), 4);
    assert_eq!(pursuit.artist_name(), "Kid Cudi");
    for artist in &artists {
        assert!(pursuit.is_credited_to(artist));
    }
    let wrong_identity = textamp::library::models::Artist {
        rating_key: session.key("different-mgmt"),
        title: "MGMT".into(),
        ..Default::default()
    };
    assert!(
        !pursuit.is_credited_to(&wrong_identity),
        "distinct server IDs must not match by name"
    );
    let index = ArtistIndex::build(&artists[..1], &[], std::slice::from_ref(&pursuit));
    assert!(index
        .track_artists
        .iter()
        .any(|artist| artist.rating_key == session.key("mgmt")));
    assert!(!index
        .track_artists
        .iter()
        .any(|artist| artist.title.contains("feat.") || artist.title == "Unrelated composer"));
    let appearance = session.album(serde_json::from_value(serde_json::json!({"id":"cudi-album","name":"Man on the Moon","artist":"Kid Cudi","artistId":"cudi","year":2009})).unwrap());
    let joint = session.album(serde_json::from_value(serde_json::json!({"id":"joint","name":"Joint release","artist":"Kid Cudi & MGMT","artistId":"cudi","artists":[{"id":"cudi","name":"Kid Cudi"},{"id":"mgmt","name":"MGMT"}]})).unwrap());
    state.library.artists = artists.clone();
    state.library.albums = vec![joint, appearance];
    state.library.all_tracks = vec![pursuit];
    let (tx, _rx) = mpsc::channel(8);
    for artist in &artists[1..] {
        dispatch_browse::dispatch(
            &tx,
            BrowseAction::OpenArtistOverview {
                artist_key: artist.rating_key.clone(),
            },
            &mut state,
        )
        .await
        .unwrap();
        let page = state.artist_overview.as_ref().unwrap();
        assert!(
            page.display_albums()
                .iter()
                .any(|album| album.rating_key == session.key("cudi-album")
                    && page.release_section(album).label() == "Appears in"),
            "{} is missing the credited appearance",
            artist.title
        );
        assert_eq!(page.display_tracks().len(), 1);
        if artist.title == "MGMT" {
            let joint = page
                .display_albums()
                .iter()
                .find(|album| album.rating_key == session.key("joint"))
                .unwrap();
            assert_eq!(page.release_section(joint).label(), "Albums");
        }
    }
    let old: Track =
        serde_json::from_str(r#"{"ratingKey":"legacy","originalTitle":"MGMT"}"#).unwrap();
    assert!(old.artist_credits.is_empty());
    assert!(
        old.is_credited_to(&artists[1]),
        "legacy single-artist caches remain usable"
    );
    let saved = serde_json::to_string(&state.library.all_tracks[0]).unwrap();
    let restored: Track = serde_json::from_str(&saved).unwrap();
    assert!(
        restored.is_credited_to(&artists[1]),
        "structured IDs survive normalized caches"
    );
}

#[test]
fn discography_dates_sort_within_sections_and_refresh_preserves_selected_album() {
    use textamp::library::catalog::ArtistCredit;
    let mut fixture = page();
    fixture.albums = [
        ("unknown", None, None, None),
        ("recent", None, Some("2024-05-20"), None),
        ("reissue", Some("1964-06-26"), Some("2009-09-09"), None),
        ("year-only", None, None, Some(1980)),
        ("later-month", None, Some("2024-06"), None),
        ("invalid", Some("invalid"), Some("2020-01-01"), None),
    ]
    .into_iter()
    .map(|(key, original, release, year)| Album {
        rating_key: key.into(),
        title: key.into(),
        originally_available_at: original.map(String::from),
        release_date: release.map(String::from),
        year,
        ..Default::default()
    })
    .collect();
    fixture.sort_releases();
    assert_eq!(
        fixture
            .albums
            .iter()
            .map(|album| album.rating_key.as_str())
            .collect::<Vec<_>>(),
        [
            "later-month",
            "recent",
            "invalid",
            "year-only",
            "reissue",
            "unknown"
        ]
    );
    let mut state = AppState::new();
    state.library.artists = vec![fixture.artist.clone()];
    state.library.albums = fixture.albums.clone();
    fixture.index = 1;
    fixture.scroll_pin = Some(1);
    let artist = fixture.artist.clone();
    state.artist_overview = Some(fixture);
    state.library.all_tracks = vec![Track {
        parent_rating_key: Some("guest-album".into()),
        artist_credits: vec![ArtistCredit {
            rating_key: Some(artist.rating_key),
            name: artist.title,
        }],
        ..Default::default()
    }];
    state.library.albums.push(Album {
        rating_key: "guest-album".into(),
        title: "Guest release".into(),
        parent_rating_key: Some("other".into()),
        parent_title: Some("Other artist".into()),
        year: Some(1970),
        ..Default::default()
    });
    // Own entries need their album ownership, just as provider-normalized catalogs do.
    let owner = state
        .artist_overview
        .as_ref()
        .unwrap()
        .artist
        .rating_key
        .clone();
    for album in &mut state.library.albums[..6] {
        album.parent_rating_key = Some(owner.clone());
    }
    dispatch_browse::refresh_artist_overview(&mut state);
    let page = state.artist_overview.as_ref().unwrap();
    assert_eq!(page.display_albums()[page.index].rating_key, "recent");
    assert_eq!(page.scroll_pin, Some(1));
    assert_eq!(
        page.display_albums().last().unwrap().rating_key,
        "guest-album"
    );
}

#[test]
fn artist_appearance_loading_is_visible_with_cached_releases_and_clears_after_refresh() {
    let mut state = AppState::new();
    state.view = View::Related;
    let mut fixture = page();
    fixture.albums[0].parent_rating_key = Some(fixture.artist.rating_key.clone());
    state.library.albums = fixture.albums.clone();
    state.artist_overview = Some(fixture);
    state.library_loading = true;
    let draw = |state: &mut AppState| {
        let mut terminal = Terminal::new(TestBackend::new(120, 40)).unwrap();
        terminal
            .draw(|frame| state.apply_render_feedback(textamp::ui::render(frame, state)))
            .unwrap();
        terminal
            .backend()
            .buffer()
            .content
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>()
    };
    let loading = draw(&mut state);
    assert!(loading.contains("Loading releases / Appears in"));
    assert!(
        loading.contains("A Test Album"),
        "cached releases remain usable"
    );
    assert!(loading.contains("Newest first"));
    let selected = state.artist_overview.as_ref().unwrap().artist.clone();
    state.library.all_tracks.push(Track {
        parent_rating_key: Some("guest".into()),
        artist_credits: vec![textamp::library::catalog::ArtistCredit {
            rating_key: Some(selected.rating_key),
            name: selected.title,
        }],
        ..Default::default()
    });
    state.library.albums.push(Album {
        rating_key: "guest".into(),
        title: "Guest release".into(),
        parent_rating_key: Some("someone-else".into()),
        parent_title: Some("Someone Else".into()),
        ..Default::default()
    });
    dispatch_browse::refresh_artist_overview(&mut state);
    state.library_loading = false;
    let loaded = draw(&mut state);
    assert!(!loaded.contains("Loading releases / Appears in"));
    assert!(loaded.contains("Appears in (1)"));
    state.artist_overview.as_mut().unwrap().albums.clear();
    state.library_loading = true;
    let empty_loading = draw(&mut state);
    assert!(empty_loading.contains("Loading releases and artist credits"));
    assert!(!empty_loading.contains("No entries available"));
}
