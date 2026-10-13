use image::{DynamicImage, RgbaImage};
use ratatui::{backend::TestBackend, layout::Rect, Terminal};
use ratatui_image::{
    protocol::{kitty::Kitty, Protocol},
    Image,
};

fn transfer(tmux: bool) -> String {
    let image = DynamicImage::ImageRgba8(RgbaImage::from_fn(500, 500, |x, y| {
        image::Rgba([x as u8, y as u8, (x ^ y) as u8, 255])
    }));
    let area = Rect::new(0, 0, 50, 25);
    let protocol = Protocol::Kitty(Kitty::new(image, area, 12345, tmux).unwrap());
    let mut terminal = Terminal::new(TestBackend::new(50, 25)).unwrap();
    terminal
        .draw(|frame| frame.render_widget(Image::new(&protocol), area))
        .unwrap();
    terminal.backend().buffer()[(0, 0)].symbol().to_owned()
}

#[test]
fn large_pixel_cover_uses_bounded_tmux_messages_without_changing_image_data() {
    let tmux = transfer(true);
    let native = transfer(false);
    assert!(
        tmux.len() > 1_000_000,
        "Use a real cover-sized transfer that exceeds tmux limits"
    );
    let mut remaining = tmux.as_str();
    let mut unwrapped = String::new();
    let mut count = 0;
    while let Some(body) = remaining.strip_prefix("\x1bPtmux;") {
        let bytes = body.as_bytes();
        let mut end = 0;
        while end + 1 < bytes.len() {
            if bytes[end] == 0x1b {
                if bytes[end + 1] == 0x1b {
                    end += 2;
                    continue;
                }
                if bytes[end + 1] == b'\\' {
                    break;
                }
            }
            end += 1;
        }
        assert!(
            end < 8192,
            "One passthrough message exceeds tmux's safe buffer size: {end}"
        );
        let chunk = body[..end].replace("\x1b\x1b", "\x1b");
        assert!(chunk.starts_with("\x1b_G") && chunk.ends_with("\x1b\\"));
        unwrapped.push_str(&chunk);
        remaining = &body[end + 2..];
        count += 1;
    }
    assert!(
        count > 300,
        "Each graphics chunk needs its own passthrough envelope"
    );
    unwrapped.push_str(remaining);
    assert_eq!(
        unwrapped, native,
        "All pixels, dimensions, continuation markers and placeholders must survive"
    );
}

#[test]
fn preview_and_thumbnail_keep_independent_terminal_image_ids() {
    let mut picker = ratatui_image::picker::Picker::halfblocks();
    picker.set_protocol_type(ratatui_image::picker::ProtocolType::Kitty);
    textamp::ui::artwork::init_grid_renderer(picker);
    textamp::ui::artwork::set_grid_artwork_mode(textamp::app::state::ArtworkMode::Auto);
    textamp::ui::artwork::clear_grid_cache();
    let mut encoded = std::io::Cursor::new(Vec::new());
    DynamicImage::new_rgb8(500, 500)
        .write_to(&mut encoded, image::ImageFormat::Png)
        .unwrap();
    let bytes = encoded.into_inner();
    let mut terminal = Terminal::new(TestBackend::new(80, 30)).unwrap();
    terminal
        .draw(|frame| {
            assert!(textamp::ui::artwork::render_grid_image(
                frame,
                Rect::new(0, 0, 40, 20),
                "same-album",
                &bytes
            ));
            assert!(textamp::ui::artwork::render_grid_image(
                frame,
                Rect::new(45, 0, 4, 2),
                "same-album",
                &bytes
            ));
        })
        .unwrap();
    let id = |x| {
        terminal.backend().buffer()[(x, 0)]
            .symbol()
            .split("i=")
            .nth(1)
            .unwrap()
            .split(',')
            .next()
            .unwrap()
            .to_owned()
    };
    assert_ne!(
        id(0),
        id(45),
        "A thumbnail must not replace the preview's pixel data"
    );
}

#[test]
fn inverse_terminal_selection_keeps_kitty_thumbnail_cells_unchanged() {
    use textamp::app::state::{ArtistOverviewState, ArtistOverviewTab, View};
    use textamp::library::models::{Album, Artist};
    let mut picker = ratatui_image::picker::Picker::halfblocks();
    picker.set_protocol_type(ratatui_image::picker::ProtocolType::Kitty);
    textamp::ui::artwork::init_grid_renderer(picker);
    let mut state = textamp::app::AppState::new();
    state.view = View::Related;
    state.theme = textamp::app::theme::ThemeName::Terminal;
    state.artist_overview = Some(ArtistOverviewState {
        rankings: Default::default(),
        from_label_album: false,
        artist: Artist::default(),
        albums: vec![Album {
            rating_key: "selected-cover".into(),
            title: "Selected album".into(),
            thumb: Some("cover".into()),
            ..Default::default()
        }],
        tracks: vec![],
        album_tracks: vec![],
        selected_album: None,
        album_loading: false,
        album_error: None,
        tab: ArtistOverviewTab::Albums,
        index: 0,
        scroll_pin: None,
        return_view: View::Browse,
    });
    let mut encoded = std::io::Cursor::new(Vec::new());
    DynamicImage::new_rgb8(500, 500)
        .write_to(&mut encoded, image::ImageFormat::Png)
        .unwrap();
    state
        .artwork
        .grid_cache
        .insert("selected-cover".into(), encoded.into_inner());
    let mut terminal = Terminal::new(TestBackend::new(120, 40)).unwrap();
    terminal
        .draw(|frame| state.apply_render_feedback(textamp::ui::render(frame, &state)))
        .unwrap();
    let area = state.hit_regions.artist_overview.as_ref().unwrap().items[0].0;
    let cell = &terminal.backend().buffer()[(area.x, area.y)];
    assert!(cell.symbol().contains('\u{10EEEE}'));
    assert!(
        !cell.modifier.contains(ratatui::style::Modifier::REVERSED),
        "Selection styling must not change Kitty image-ID colors"
    );
}
