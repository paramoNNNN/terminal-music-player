use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::{backend::TestBackend, style::Color, Terminal};
use textamp::app::dispatch::handle_core_event;
use textamp::app::event::PlaybackEvent;
use textamp::app::handlers::{dispatch_miller::collect_queue_art, mouse_input};
use textamp::app::state::{ArtworkMode, PlaybackMode, ScrobbleStatus, View};
use textamp::app::{AppState, Event};
use tokio::sync::mpsc;
mod common;

fn fixture() -> AppState {
    let mut state = common::navidrome_state();
    let session = state.sources.active.navidrome().unwrap();
    state.queue.tracks = (0..20)
        .map(|index| {
            session.track(serde_json::from_value(
        serde_json::json!({"id":format!("song-{index}"), "title":format!("Song {index}"),
            "artist":"Fixture artist", "album":"Fixture album", "duration":60,
            "coverArt":format!("cover-{}", index / 2)})
    ).unwrap())
        })
        .collect();
    state.queue.index = Some(0);
    state.playback_mode = PlaybackMode::Queue;
    state.view = View::NowPlaying;
    state.artwork.mode = ArtworkMode::Halfblocks;
    state.playback.request_id = 42;
    state
}
fn render(state: &mut AppState, width: u16, height: u16) -> Terminal<TestBackend> {
    state.terminal_width = width;
    state.terminal_height = height;
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal
        .draw(|frame| state.apply_render_feedback(textamp::ui::render(frame, state)))
        .unwrap();
    terminal
}
fn text(terminal: &Terminal<TestBackend>) -> String {
    terminal
        .backend()
        .buffer()
        .content
        .iter()
        .map(|cell| cell.symbol())
        .collect()
}

#[test]
fn scrobble_confirmation_is_visible_only_after_the_current_playback_succeeds() {
    let mut state = fixture();
    let (tx, _rx) = mpsc::channel(8);
    let track_key = state.current_track().unwrap().rating_key.clone();
    assert!(text(&render(&mut state, 120, 42)).contains("Scrobble pending"));
    state.playback.scrobble_reported = true;
    state.playback.scrobble_status = ScrobbleStatus::Submitting;
    assert!(text(&render(&mut state, 120, 42)).contains("Scrobbling…"));
    for (generation, playback_id, key) in [
        (state.library_generation + 1, 42, track_key.clone()),
        (state.library_generation, 41, track_key.clone()),
        (state.library_generation, 42, "another-track".into()),
    ] {
        handle_core_event(
            Event::for_library(
                generation,
                PlaybackEvent::ScrobbleFinished {
                    playback_id,
                    track_key: key,
                    result: Ok(()),
                },
            ),
            &mut state,
            &tx,
        );
        assert_eq!(state.playback.scrobble_status, ScrobbleStatus::Submitting);
    }
    handle_core_event(
        PlaybackEvent::ScrobbleFinished {
            playback_id: 42,
            track_key: track_key.clone(),
            result: Ok(()),
        }
        .into(),
        &mut state,
        &tx,
    );
    assert!(text(&render(&mut state, 120, 42)).contains("✓ Scrobbled"));
    // Replaying the same song still has a new playback identity.
    state.playback.request_id = 43;
    state.playback.scrobble_status = ScrobbleStatus::Waiting;
    handle_core_event(
        PlaybackEvent::ScrobbleFinished {
            playback_id: 42,
            track_key: track_key.clone(),
            result: Ok(()),
        }
        .into(),
        &mut state,
        &tx,
    );
    assert_eq!(state.playback.scrobble_status, ScrobbleStatus::Waiting);
    handle_core_event(
        PlaybackEvent::ScrobbleFinished {
            playback_id: 43,
            track_key,
            result: Err("fixture rejection".into()),
        }
        .into(),
        &mut state,
        &tx,
    );
    assert!(text(&render(&mut state, 120, 42)).contains("Scrobble failed"));
    state.queue.tracks[0].origin = textamp::library::track::TrackOrigin::Folder {
        source_id: "local".into(),
        path: "/fixture/song.flac".into(),
    };
    assert!(text(&render(&mut state, 120, 42)).contains("Scrobbling unavailable"));
}

#[test]
fn waveform_has_a_blank_row_before_transport_and_short_terminals_still_render() {
    for tall in [false, true] {
        let mut state = fixture();
        state.tall_mode = tall;
        let terminal = render(&mut state, 120, 42);
        let wave = state
            .hit_regions
            .now_playing_content
            .as_ref()
            .unwrap()
            .visualizer_content_area;
        let gap_y = wave.bottom() + 1;
        let transport = state.hit_regions.transport.as_ref().unwrap();
        assert!(gap_y < transport.play_pause.y);
        assert!((0..120).all(|x| terminal.backend().buffer()[(x, gap_y)].symbol() == " "));
    }
    for (width, height) in [(24, 8), (45, 12), (120, 18)] {
        let mut state = fixture();
        render(&mut state, width, height);
        assert!(state.hit_regions.queue_content.is_some());
    }
}

#[test]
fn queue_covers_load_only_visible_rows_and_keep_both_rows_clickable() {
    let mut state = fixture();
    state.scroll.queue = Some(4);
    state.list_state.queue_index = 6;
    render(&mut state, 120, 42);
    let region = state.hit_regions.queue_content.as_ref().unwrap().clone();
    let batch = collect_queue_art(&state);
    assert!(!batch.is_empty());
    let thumb = state.queue.tracks[4].best_thumb().unwrap().to_owned();
    assert_eq!(batch[0], (thumb.clone(), thumb.clone()));
    assert_eq!(batch.iter().filter(|(key, _)| key == &thumb).count(), 1);
    let mut png = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgb8(image::RgbImage::from_pixel(32, 32, image::Rgb([255, 0, 0])))
        .write_to(&mut png, image::ImageFormat::Png)
        .unwrap();
    state
        .artwork
        .insert_grid_art(thumb.clone(), png.into_inner());
    let terminal = render(&mut state, 120, 42);
    let cover = region.track_list_inner;
    assert!(
        (cover.y..cover.y + 2).any(|y| (cover.x..cover.x + 4).any(|x| {
            let cell = &terminal.backend().buffer()[(x, y)];
            cell.fg == Color::Rgb(255, 0, 0) || cell.bg == Color::Rgb(255, 0, 0)
        }))
    );
    for row in 0..2 {
        mouse_input::handle_mouse(
            MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column: cover.x + 1,
                row: cover.y + row,
                modifiers: KeyModifiers::NONE,
            },
            &mut state,
        );
        assert_eq!(state.list_state.queue_index, 4);
        assert_eq!(state.scroll.queue, Some(4));
    }
    assert!(!collect_queue_art(&state)
        .iter()
        .any(|(key, _)| key == &thumb));
    state.artwork.suppress_loads = true;
    assert!(collect_queue_art(&state).is_empty());
    state.artwork.suppress_loads = false;
    state.fullscreen_active = true;
    assert!(collect_queue_art(&state).is_empty());
}

#[test]
fn current_cover_survives_library_focus_and_terminal_activation_changes() {
    let mut state = fixture();
    state.tall_mode = true;
    state.artwork.mode = ArtworkMode::Auto;
    let mut png = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgb8(image::RgbImage::from_pixel(32, 32, image::Rgb([255, 0, 0])))
        .write_to(&mut png, image::ImageFormat::Png)
        .unwrap();
    state.artwork.current_thumb = state
        .current_track()
        .unwrap()
        .best_thumb()
        .map(str::to_owned);
    state.artwork.current_data = Some(png.into_inner());
    textamp::ui::screens::now_playing::init_artwork_renderer(
        ratatui_image::picker::Picker::halfblocks(),
    );
    let (tx, _rx) = mpsc::channel(8);
    for view in [View::Queue, View::Browse, View::NowPlaying] {
        state.view = view;
        for focused in [false, true] {
            handle_core_event(Event::FocusChanged(focused), &mut state, &tx);
            let terminal = render(&mut state, 120, 42);
            assert_eq!(state.terminal_focused, focused);
            let area = state.hit_regions.queue_content.as_ref().unwrap().art_area;
            let buffer = terminal.backend().buffer();
            let cover_text: String = (area.y..area.bottom())
                .flat_map(|y| (area.x..area.right()).map(move |x| buffer[(x, y)].symbol()))
                .collect();
            assert!(!cover_text.contains("No artwork"));
            if !focused {
                assert!(
                    (area.y..area.bottom()).any(|y| (area.x..area.right()).any(|x| {
                        let cell = &buffer[(x, y)];
                        cell.fg == Color::Rgb(255, 0, 0) || cell.bg == Color::Rgb(255, 0, 0)
                    }))
                );
            }
        }
    }
}
