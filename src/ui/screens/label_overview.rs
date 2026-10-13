//! Label catalog: two columns, with album/artist tabs and library-scoped covers.
use crate::ui::{layout::centered_rect, theme::theme, RenderState};
use crate::util::truncate_middle;
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Wrap};

pub fn render(frame: &mut Frame, state: &RenderState, area: Rect) {
    let Some(page) = &state.label_overview else {
        return;
    };
    let t = theme();
    let outer = centered_rect(82, 82, area);
    frame.render_widget(Clear, outer);
    let block = Block::default()
        .title(page.label.as_deref().unwrap_or("Record Labels"))
        .title_bottom(" ↑↓ select · Enter open · Tab albums / artists · Esc back ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(t.colors.fg_accent));
    let inner = block.inner(outer);
    frame.render_widget(block, outer);
    let columns =
        Layout::horizontal([Constraint::Percentage(42), Constraint::Percentage(58)]).split(inner);
    let detail_block = Block::default()
        .title(" Label ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(t.colors.border));
    let detail = detail_block.inner(columns[0]);
    frame.render_widget(detail_block, columns[0]);
    let selected_art = if page.label.is_none() {
        None
    } else if page.artists_tab {
        page.artists
            .get(page.index)
            .map(|artist| (artist.rating_key.as_str(), artist.thumb.as_deref()))
    } else {
        page.albums
            .get(page.index)
            .map(|album| (album.rating_key.as_str(), album.thumb.as_deref()))
    };
    let art_h = detail
        .height
        .saturating_sub(8)
        .min(12)
        .min(detail.width / 2);
    let art = Rect::new(
        detail.x + detail.width.saturating_sub(art_h * 2) / 2,
        detail.y,
        art_h * 2,
        art_h,
    );
    if let Some((key, thumb)) = selected_art {
        crate::ui::artwork::render_thumbnail(frame, state, art, key, thumb);
    }
    let metadata = if let Some(label) = &page.label {
        format!("{label}\n\n{} albums · {} artists\n\nFrom record-label tags in this library.\nTab switches albums / artists.", page.albums.len(), page.artists.len())
    } else {
        format!("{} record labels\n\nSelect a label to browse its albums and artists.\n\nIf a label is missing, check the album's tags and refresh the library with F5.", page.labels.len())
    };
    let info_y = if selected_art.is_some() {
        art.bottom().saturating_add(1).min(detail.bottom())
    } else {
        detail.y
    };
    frame.render_widget(
        Paragraph::new(metadata)
            .wrap(Wrap { trim: false })
            .style(Style::default().fg(t.colors.fg_primary)),
        Rect::new(
            detail.x,
            info_y,
            detail.width,
            detail.bottom().saturating_sub(info_y),
        ),
    );
    let list_title = if page.label.is_none() {
        " Record Labels "
    } else if page.artists_tab {
        " [Tab] Artists "
    } else {
        " [Tab] Albums "
    };
    let list_block = Block::default()
        .title(list_title)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(t.colors.border));
    let list = list_block.inner(columns[1]);
    frame.render_widget(list_block, columns[1]);
    let row_height = if page.label.is_none() { 1 } else { 2 };
    let visible = (list.height / row_height).max(1) as usize;
    let first = page
        .scroll_pin
        .unwrap_or_else(|| page.index.saturating_sub(visible.saturating_sub(1)));
    let rows: Vec<_> = (first..page.len().min(first + visible))
        .map(|index| {
            let style = if index == page.index {
                Style::default()
                    .fg(t.colors.selection_text)
                    .bg(t.colors.selection_bar_bg)
            } else {
                Style::default().fg(t.colors.fg_primary)
            };
            if page.label.is_none() {
                ListItem::new(truncate_middle(&page.labels[index], list.width as usize))
                    .style(style)
            } else {
                let (title, subtitle) = if page.artists_tab {
                    let artist = &page.artists[index];
                    let count = page
                        .albums
                        .iter()
                        .filter(|album| {
                            album.parent_rating_key.as_ref() == Some(&artist.rating_key)
                                || album.artist_name().eq_ignore_ascii_case(&artist.title)
                        })
                        .count();
                    (
                        artist.title.as_str(),
                        format!("{count} albums on this label"),
                    )
                } else {
                    let album = &page.albums[index];
                    (
                        album.title.as_str(),
                        format!(
                            "{} · {}",
                            album.artist_name(),
                            album.year.map_or(String::new(), |year| year.to_string())
                        ),
                    )
                };
                ListItem::new(Text::from(vec![
                    Line::styled(
                        format!(
                            "      {}",
                            truncate_middle(title, list.width.saturating_sub(6) as usize)
                        ),
                        style,
                    ),
                    Line::styled(
                        format!(
                            "      {}",
                            truncate_middle(&subtitle, list.width.saturating_sub(6) as usize)
                        ),
                        if index == page.index {
                            style
                        } else {
                            Style::default().fg(t.colors.fg_muted)
                        },
                    ),
                ]))
                .style(style)
            }
        })
        .collect();
    if rows.is_empty() {
        frame.render_widget(
            Paragraph::new("No matching record-label tags in this library"),
            list,
        );
    } else {
        frame.render_widget(List::new(rows), list);
        if page.label.is_some() {
            for (row, index) in (first..page.len().min(first + visible)).enumerate() {
                let (key, thumb) = if page.artists_tab {
                    let artist = &page.artists[index];
                    (artist.rating_key.as_str(), artist.thumb.as_deref())
                } else {
                    let album = &page.albums[index];
                    (album.rating_key.as_str(), album.thumb.as_deref())
                };
                crate::ui::artwork::render_thumbnail(
                    frame,
                    state,
                    Rect::new(
                        list.x,
                        list.y + row as u16 * row_height,
                        list.width.min(4),
                        row_height,
                    ),
                    key,
                    thumb,
                );
            }
        }
    }
    state.hit_regions.borrow_mut().label_overview =
        Some(crate::app::presentation::ArtistOverviewRegions {
            items: Vec::new(),
            tabs: Vec::new(),
            outer,
            list,
            first_index: first,
            row_height,
            tab_button: page.label.as_ref().map(|_| {
                Rect::new(
                    columns[1].x + 1,
                    columns[1].y,
                    columns[1].width.saturating_sub(2).min(20),
                    1,
                )
            }),
            ranking_button: None,
            labels_button: None,
        });
}
