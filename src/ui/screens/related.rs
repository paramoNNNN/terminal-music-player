//! Related artists popup overlay.
//!
//! Shows artists related to the selected artist, with their albums
//! grouped by artist, rendered as a centered popup over the previous view.

use crate::app::state::{RelatedArtistGroup, RelatedSource};
use crate::services::NavigationService;
use crate::ui::layout::centered_rect;
use crate::ui::theme::theme;
use crate::ui::RenderState as AppState;
use crate::util::truncate_middle;

use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, Paragraph};

/// Render the related artists popup overlay.
pub fn render(frame: &mut Frame, state: &AppState, area: Rect) {
    let t = theme();

    // Centered popup: 65% wide, 80% tall
    let popup_area = centered_rect(65, 80, area);
    frame.render_widget(Clear, popup_area);

    let title = format!(" related to: {} ", state.related.source_title);
    let block = Block::default()
        .title(title)
        .title_style(Style::default().fg(t.colors.fg_accent))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(t.colors.fg_accent))
        .style(Style::default().bg(t.colors.bg_primary));
    let inner = block.inner(popup_area);
    frame.render_widget(block, popup_area);

    // Register hit regions for mouse handler
    {
        let mut hr = state.hit_regions.borrow_mut();
        hr.related_content = Some(crate::app::presentation::RelatedRegions {
            outer: popup_area,
            inner,
            rows_per_item: 2,
        });
    }

    if state.related.loading {
        let loading = Paragraph::new("Loading related artists...")
            .style(Style::default().fg(t.colors.fg_muted))
            .alignment(Alignment::Center);
        frame.render_widget(loading, inner);
        return;
    }

    if state.related.groups.is_empty() {
        let empty = Paragraph::new("No related artists found")
            .style(Style::default().fg(t.colors.fg_muted))
            .alignment(Alignment::Center);
        frame.render_widget(empty, inner);
        return;
    }

    // Split inner: content list + footer
    let content_height = inner.height.saturating_sub(1);
    let content_area = Rect::new(inner.x, inner.y, inner.width, content_height);
    let footer_area = Rect::new(inner.x, inner.y + content_height, inner.width, 1);

    let selected_idx = state.list_state.related_index;
    let total = flat_count(&state.related.groups);
    let visible_item_count = content_area.height as usize / 2;
    let max_text_width = content_area.width.saturating_sub(2) as usize;

    let scroll_offset = match state.scroll.related {
        Some(pinned) => pinned,
        None => NavigationService::calc_scroll_offset(selected_idx, visible_item_count, total),
    };

    // Artist headers and albums use two-row thumbnails with one shared index
    // per item. Mouse/scrollbar geometry uses the same rows_per_item feedback.
    let mut entries = Vec::new();
    for group in &state.related.groups {
        let suffix = match group.source {
            RelatedSource::Alias => " (alias)",
            RelatedSource::SimilarTag => " (similar)",
            RelatedSource::Navidrome => "",
        };
        entries.push((
            group.artist.rating_key.as_str(),
            group.artist.thumb.as_deref(),
            format!("{}{suffix}", group.artist.title),
            "Artist".to_owned(),
            true,
        ));
        for album in &group.albums {
            entries.push((
                album.rating_key.as_str(),
                album.thumb.as_deref(),
                album.title.clone(),
                album.year.map_or(String::new(), |year| year.to_string()),
                false,
            ));
        }
    }
    let items: Vec<ListItem> = entries
        .iter()
        .enumerate()
        .skip(scroll_offset)
        .take(visible_item_count)
        .map(|(index, (_, _, title, subtitle, is_artist))| {
            let selected = index == selected_idx;
            let fg = if selected {
                t.colors.selection_text
            } else if *is_artist {
                t.colors.fg_accent
            } else {
                t.colors.fg_primary
            };
            let bg = if selected {
                t.colors.selection_bar_bg
            } else {
                t.colors.bg_primary
            };
            let style = Style::default().fg(fg).bg(bg);
            ListItem::new(vec![
                Line::styled(
                    format!(
                        "      {}",
                        truncate_middle(title, max_text_width.saturating_sub(6))
                    ),
                    style,
                ),
                Line::styled(format!("      {subtitle}"), style),
            ])
            .style(style)
        })
        .collect();
    frame.render_widget(List::new(items), content_area);
    for (row, (key, thumb, _, _, _)) in entries
        .iter()
        .skip(scroll_offset)
        .take(visible_item_count)
        .enumerate()
    {
        crate::ui::artwork::render_thumbnail(
            frame,
            state,
            Rect::new(
                content_area.x,
                content_area.y + row as u16 * 2,
                content_area.width.min(4),
                2,
            ),
            key,
            *thumb,
        );
    }

    // Scrollbar + position indicator
    if total > visible_item_count {
        crate::ui::widgets::render_scrollbar(
            frame,
            popup_area,
            total,
            visible_item_count,
            scroll_offset,
            None,
        );

        let footer_pos = format!("{}/{}", selected_idx + 1, total);
        let footer_pos_area = Rect::new(
            popup_area.x + popup_area.width.saturating_sub(footer_pos.len() as u16 + 2),
            popup_area.y + popup_area.height.saturating_sub(1),
            footer_pos.len() as u16 + 1,
            1,
        );
        frame.render_widget(
            Paragraph::new(footer_pos).style(Style::default().fg(t.colors.fg_muted)),
            footer_pos_area,
        );
    }

    // Footer
    let footer_spans = vec![
        Span::styled(" [Esc] ", Style::default().fg(t.colors.shortcut_key)),
        Span::styled("close", Style::default().fg(t.colors.fg_muted)),
    ];
    let footer = Paragraph::new(Line::from(footer_spans));
    frame.render_widget(footer, footer_area);
}

/// Count total flat items in related groups (1 header + N albums per group).
fn flat_count(groups: &[RelatedArtistGroup]) -> usize {
    groups.iter().map(|g| 1 + g.albums.len()).sum()
}
