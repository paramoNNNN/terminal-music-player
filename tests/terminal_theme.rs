use ratatui::{backend::TestBackend, style::Color, Terminal};
use textamp::app::state::{CategoryRow, View};
use textamp::app::theme::ThemeName;
use textamp::app::AppState;
use textamp::ui::theme::Theme;

#[test]
fn terminal_defaults_and_palette_survive_rendering_without_rgb_snapshots() {
    let config = textamp::config::Config::default();
    assert_eq!(config.ui.theme, "terminal");
    assert_eq!(
        ThemeName::from_config(&config.ui.theme),
        ThemeName::Terminal
    );
    assert_eq!(
        ThemeName::from_config("solarized-dark"),
        ThemeName::SolarizedDark
    );
    let theme = Theme::new(ThemeName::Terminal);
    assert_eq!(theme.colors.bg_primary, Color::Reset);
    assert_eq!(theme.colors.bg_secondary, Color::Reset);
    assert_eq!(theme.colors.fg_primary, Color::Reset);
    assert_eq!(theme.colors.fg_muted, Color::Reset);
    assert_eq!(theme.colors.border, Color::Reset);
    assert_eq!(theme.colors.transport_bg, Color::Reset);
    assert_ne!(theme.colors.selection_bar_bg, theme.colors.selection_text);

    let mut state = AppState::new();
    state.theme = ThemeName::Terminal;
    for view in [View::Browse, View::NowPlaying, View::Settings, View::Help] {
        state.view = view;
        let mut terminal = Terminal::new(TestBackend::new(120, 40)).unwrap();
        terminal
            .draw(|frame| {
                let _ = textamp::ui::render(frame, &state);
            })
            .unwrap();
        assert_eq!(
            terminal.backend().buffer()[(119, 0)].bg,
            Color::Reset,
            "{view:?} painted over the terminal background"
        );
    }

    // Search and theme previews previously attempted to blend Reset/Indexed as
    // RGB black, making the theme logo disappear. Keep its accents as ANSI IDs.
    state.view = View::Browse;
    state.category_column_focused = true;
    state.category_column_index = state
        .category_rows()
        .iter()
        .position(|row| matches!(row, CategoryRow::Search))
        .unwrap();
    let mut terminal = Terminal::new(TestBackend::new(120, 40)).unwrap();
    terminal
        .draw(|frame| {
            state.apply_render_feedback(textamp::ui::render(frame, &state));
        })
        .unwrap();
    let area = state.hit_regions.search_landing.unwrap();
    let buffer = terminal.backend().buffer();
    let mut accents = 0;
    for y in area.y..area.bottom() {
        for x in area.x..area.right() {
            let cell = &buffer[(x, y)];
            assert!(
                !matches!(cell.fg, Color::Rgb(..)) && !matches!(cell.bg, Color::Rgb(..)),
                "terminal logo painted fixed RGB colors"
            );
            accents += usize::from(cell.fg == theme.colors.fg_accent);
        }
    }
    assert!(
        accents > 10,
        "terminal logo should retain visible palette accents"
    );
}

#[test]
fn terminal_selection_reverses_defaults_instead_of_guessing_palette_contrast() {
    let mut state = AppState::new();
    state.theme = ThemeName::Terminal;
    state.view = View::Queue;
    state.queue.tracks = vec![textamp::library::models::Track {
        title: "Selected song".into(),
        ..Default::default()
    }];
    let mut terminal = Terminal::new(TestBackend::new(120, 40)).unwrap();
    terminal
        .draw(|frame| {
            let _ = textamp::ui::render(frame, &state);
        })
        .unwrap();
    assert!(terminal.backend().buffer().content.iter().any(|cell| cell
        .modifier
        .contains(ratatui::style::Modifier::REVERSED)
        && cell.fg == Color::Reset
        && cell.bg == Color::Reset));
}
