//! Searchable, grouped action menu. Header rows never receive pointer input.
use crate::{
    app::command_palette::entry_group,
    ui::{theme::theme, RenderState as AppState},
};
use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

pub fn render(frame: &mut Frame, state: &AppState, area: Rect) {
    if !state.palette.open {
        return;
    }
    let t = theme();
    let popup_area = centered(area);
    frame.render_widget(Clear, popup_area);
    let title = if state.station_nav.loading {
        " Actions · loading stations… "
    } else {
        " Actions "
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(t.colors.title_focused))
        .title(title)
        .title_bottom(" ↑↓ move · PgUp/PgDn · Enter choose · Esc close ")
        .style(
            Style::default()
                .bg(t.colors.bg_primary)
                .fg(t.colors.fg_primary),
        );
    let inner = block.inner(popup_area);
    frame.render_widget(block, popup_area);
    if inner.height < 4 || inner.width < 4 {
        return;
    }
    let chunks = ratatui::layout::Layout::vertical([
        ratatui::layout::Constraint::Length(1),
        ratatui::layout::Constraint::Length(1),
        ratatui::layout::Constraint::Length(1),
        ratatui::layout::Constraint::Min(1),
    ])
    .split(inner);
    let context = state
        .palette_target_track()
        .map(|track| format!("{} · {}", track.title, track.track_artist()))
        .unwrap_or_else(|| "Selection actions and player commands".into());
    frame.render_widget(
        Paragraph::new(fit(&context, inner.width as usize))
            .style(Style::default().fg(t.colors.fg_muted)),
        chunks[0],
    );
    let query_width = chunks[1].width.saturating_sub(3) as usize;
    let prefix: String = state
        .palette
        .query
        .chars()
        .take(state.palette.cursor)
        .collect();
    let mut skip = 0;
    let mut prefix_width = prefix.width();
    for c in prefix.chars() {
        if prefix_width < query_width {
            break;
        }
        prefix_width = prefix_width.saturating_sub(c.width().unwrap_or(0));
        skip += 1;
    }
    let visible_query: String = state.palette.query.chars().skip(skip).collect();
    let prompt = if state.palette.query.is_empty() {
        "Type to filter actions…".into()
    } else {
        fit(&visible_query, query_width)
    };
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(": ", Style::default().fg(t.colors.fg_accent)),
            Span::styled(
                prompt,
                Style::default().fg(if state.palette.query.is_empty() {
                    t.colors.fg_muted
                } else {
                    t.colors.fg_primary
                }),
            ),
        ])),
        chunks[1],
    );
    frame.set_cursor_position((
        chunks[1].x + 2 + prefix_width.min(query_width.saturating_sub(1)) as u16,
        chunks[1].y,
    ));
    frame.render_widget(
        Paragraph::new(format!(
            "{} actions {}",
            state.palette.matches.len(),
            "─".repeat(inner.width as usize)
        ))
        .style(Style::default().fg(t.colors.border)),
        chunks[2],
    );

    // Flatten section headers and commands once. Drawing and hit testing use
    // this same list, including variable header heights and the visible offset.
    let mut rows: Vec<(Line, Option<usize>)> = Vec::new();
    let mut previous_group = None;
    for (match_idx, &entry_idx) in state.palette.matches.iter().enumerate() {
        let Some(entry) = state.palette.entries.get(entry_idx) else {
            continue;
        };
        let (group, name) = entry_group(entry);
        if state.palette.query.is_empty() && previous_group != Some(group) {
            rows.push((
                Line::styled(
                    fit(name, chunks[3].width as usize),
                    Style::default()
                        .fg(t.colors.fg_accent)
                        .add_modifier(Modifier::BOLD),
                ),
                None,
            ));
            previous_group = Some(group);
        }
        let width = chunks[3].width.saturating_sub(3) as usize;
        let hint = fit(&entry.hint, width / 3);
        let label_width = width.saturating_sub(hint.width() + usize::from(!hint.is_empty()));
        let label = fit(&entry.label, label_width);
        let pad = " ".repeat(width.saturating_sub(label.width() + hint.width()));
        let selected = match_idx == state.palette.selected;
        let style = if selected {
            Style::default()
                .bg(t.colors.bg_selection)
                .fg(t.colors.selection_text)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(t.colors.fg_primary)
        };
        rows.push((
            Line::from(vec![
                Span::raw(if selected { " › " } else { "   " }),
                Span::raw(label),
                Span::raw(pad),
                Span::raw(hint),
            ])
            .style(style),
            Some(match_idx),
        ));
    }
    let height = chunks[3].height as usize;
    let selected_row = rows
        .iter()
        .position(|(_, idx)| *idx == Some(state.palette.selected))
        .unwrap_or(0);
    let offset = selected_row.saturating_sub(height.saturating_sub(1));
    let mut hits = Vec::new();
    for (y, (line, idx)) in rows.into_iter().skip(offset).take(height).enumerate() {
        let rect = Rect::new(chunks[3].x, chunks[3].y + y as u16, chunks[3].width, 1);
        frame.render_widget(Paragraph::new(line), rect);
        if let Some(idx) = idx {
            hits.push((rect, idx));
        }
    }
    if state.palette.matches.is_empty() {
        frame.render_widget(
            Paragraph::new("No matching actions. Try artist, album, details or queue.")
                .style(Style::default().fg(t.colors.fg_muted)),
            chunks[3],
        );
    }
    state.hit_regions.borrow_mut().command_palette =
        Some(crate::app::presentation::CommandPaletteRegions {
            outer: popup_area,
            rows: hits,
        });
}

fn fit(text: &str, width: usize) -> String {
    if text.width() <= width {
        return text.into();
    }
    if width == 0 {
        return String::new();
    }
    let mut used = 0;
    let mut result = String::new();
    for c in text.chars() {
        let w = c.width().unwrap_or(0);
        if used + w > width - 1 {
            break;
        }
        result.push(c);
        used += w;
    }
    result.push('…');
    result
}
fn centered(area: Rect) -> Rect {
    let w = (area.width.saturating_mul(75) / 100)
        .clamp(48, 100)
        .min(area.width.saturating_sub(2));
    let h = 28.min(area.height.saturating_sub(2));
    Rect::new(
        area.x + area.width.saturating_sub(w) / 2,
        area.y + area.height.saturating_sub(h) / 3,
        w,
        h,
    )
}
