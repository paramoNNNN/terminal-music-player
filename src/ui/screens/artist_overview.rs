//! Two-pane artist and album overview.

use crate::app::state::{ArtistOverviewTab, ArtistRanking};
use crate::ui::layout::centered_rect;
use crate::ui::theme::theme;
use crate::ui::widgets::track_list::favorite_title;
use crate::ui::RenderState;
use crate::util::truncate_middle;
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Wrap};

pub fn render(frame: &mut Frame, state: &RenderState, area: Rect) {
    let t = theme();
    let Some(page) = state.artist_overview.as_ref() else {
        return;
    };
    let outer = centered_rect(82, 82, area);
    frame.render_widget(Clear, outer);
    let title = state
        .artist_overview
        .as_ref()
        .map(|page| {
            page.selected_album.as_ref().map_or_else(
                || format!(" {} ", page.artist.title),
                |album| format!(" {} · {} ", page.artist.title, album.title),
            )
        })
        .unwrap_or_else(|| " Artist ".into());
    let block = Block::default()
        .title(title)
        .title_bottom(
            " ↑↓ select · Enter open/play list · F favorite · Alt+N next · Alt+E append · Esc back ",
        )
        .title_style(Style::default().fg(t.colors.fg_accent))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(t.colors.fg_accent));
    let inner = block.inner(outer);
    frame.render_widget(block, outer);
    // Visible tabs wrap on narrow terminals; their exact rectangles drive mouse selection.
    let mut tabs = Vec::new();
    let mut x = inner.x;
    let mut y = inner.y;
    if page.selected_album.is_none() {
        for (number, tab) in ArtistOverviewTab::ALL.iter().enumerate() {
            let label = format!(" {} {} ", number + 1, tab.label());
            let width = (label.len() as u16).min(inner.width);
            if x > inner.x && x.saturating_add(width) > inner.right() {
                x = inner.x;
                y = y.saturating_add(1);
            }
            if y >= inner.bottom() {
                break;
            }
            let rect = Rect::new(x, y, width, 1);
            let style = if page.tab == *tab {
                Style::default()
                    .fg(t.colors.selection_text)
                    .bg(t.colors.selection_bar_bg)
                    .bold()
            } else {
                Style::default().fg(t.colors.fg_primary)
            };
            frame.render_widget(Paragraph::new(label).style(style), rect);
            tabs.push((rect, *tab));
            x = x.saturating_add(width);
        }
        y = y.saturating_add(1);
    }
    let controls = Rect::new(
        inner.x,
        y.min(inner.bottom()),
        inner.width,
        u16::from(y < inner.bottom()),
    );
    let control_text = "Tab/Shift+Tab cycle · R related · L labels · F5 refresh";
    frame.render_widget(
        Paragraph::new(control_text).style(Style::default().fg(t.colors.fg_accent)),
        controls,
    );
    let body_y = controls.bottom().min(inner.bottom());
    let body = Rect::new(
        inner.x,
        body_y,
        inner.width,
        inner.bottom().saturating_sub(body_y),
    );
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(42), Constraint::Percentage(58)])
        .split(body);
    let left_block = Block::default()
        .title(if page.selected_album.is_some() {
            " Album "
        } else {
            " Artist "
        })
        .borders(Borders::ALL)
        .border_style(Style::default().fg(t.colors.border));
    let left = left_block.inner(columns[0]);
    frame.render_widget(left_block, columns[0]);

    let album = page.preview_album();
    let selected_track = (page.tab.is_songs() && page.selected_album.is_none())
        .then(|| page.display_tracks().get(page.index))
        .flatten();
    let selected_track_heading = selected_track.map(favorite_title);
    let selected_artist = page
        .tab
        .is_artists()
        .then(|| page.rankings.relations.artists.get(page.index))
        .flatten();
    let (heading, subtitle) = if let Some(found) = selected_artist {
        (found.artist.title.as_str(), found.reason.clone())
    } else if let Some(album) = album {
        (album.title.as_str(), album.artist_name().to_owned())
    } else if let Some(track) = selected_track {
        (
            selected_track_heading.as_deref().unwrap_or_default(),
            format!("{} · {}", track.track_artist(), track.album_name()),
        )
    } else {
        (
            page.artist.title.as_str(),
            format!("{} albums · {} songs", page.albums.len(), page.tracks.len()),
        )
    };
    // Reserve room for metadata, keep the cover approximately square in cells,
    // and clamp every rectangle to the panel even at tiny terminal sizes.
    let art_height = left
        .height
        .saturating_sub(11)
        .min(12)
        .min(left.width.saturating_sub(2) / 2);
    let art_width = art_height.saturating_mul(2);
    let art = Rect::new(
        left.x + left.width.saturating_sub(art_width) / 2,
        left.y,
        art_width,
        art_height,
    );
    if art_height > 0 {
        let artwork = page.artwork();
        let cached = artwork.and_then(|(key, thumb)| {
            state
                .artwork
                .grid_cache
                .get(key)
                .or_else(|| state.artwork.grid_cache.get(thumb))
                .map(|data| (key, data))
        });
        let rendered = cached.is_some_and(|(key, data)| {
            crate::ui::artwork::render_grid_image(frame, art, key, data)
        });
        if !rendered {
            let message = if cached.is_some() {
                "Artwork could not be displayed"
            } else if artwork.is_some_and(|(key, _)| state.artwork.grid_failures.contains_key(key))
            {
                "Artwork unavailable; retrying shortly"
            } else if artwork.is_some() {
                "Loading artwork…"
            } else {
                "No artwork supplied"
            };
            frame.render_widget(
                Paragraph::new(message)
                    .alignment(Alignment::Center)
                    .style(Style::default().fg(t.colors.fg_muted)),
                art,
            );
        }
    }
    let info_y = art
        .bottom()
        .saturating_add(u16::from(art_height > 0))
        .min(left.bottom());
    let mut info = vec![
        Line::from(Span::styled(
            truncate_middle(heading, left.width.saturating_sub(2) as usize),
            Style::default()
                .fg(t.colors.fg_primary)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(Span::styled(
            subtitle,
            Style::default().fg(t.colors.fg_muted),
        )),
        Line::default(),
    ];
    if let Some(album) = album {
        let tracks: Vec<_> = if page.selected_album.is_some() {
            page.album_tracks.iter().collect()
        } else {
            page.tracks
                .iter()
                .filter(|track| track.parent_rating_key.as_deref() == Some(&album.rating_key))
                .collect()
        };
        for (label, value) in album_metadata(album, &tracks) {
            info.push(Line::from(vec![
                Span::styled(format!("{label}: "), Style::default().fg(t.colors.fg_muted)),
                Span::styled(value, Style::default().fg(t.colors.fg_primary)),
            ]));
        }
    } else if let Some(track) = selected_track {
        info.push(Line::from(format!(
            "Length: {}",
            crate::util::format_duration(track.duration_ms())
        )));
        if page.tab == ArtistOverviewTab::Community {
            info.push(Line::from(format!("Rank: {}", page.index + 1)));
            info.push(Line::from(format!(
                "Source: {}",
                if page.rankings.community_source.is_empty() {
                    "Navidrome getTopSongs · upstream agent not reported"
                } else {
                    &page.rankings.community_source
                }
            )));
            for metric in ranking_metrics(track) {
                info.push(Line::from(metric));
            }
        }
        if !track.genres.is_empty() {
            info.push(Line::from(format!("Genres: {}", track.genres.join(", "))));
        }
    } else if !page.artist.genre.is_empty() {
        info.push(Line::from(format!(
            "Genres: {}",
            page.artist
                .genre
                .iter()
                .map(|tag| tag.tag.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        )));
    }
    if page.tab.is_artists() {
        info.push(Line::from(format!("Seed: {}", page.artist.title)));
        info.push(Line::from(format!(
            "Source: {}",
            page.rankings.relations.source
        )));
        if let Some(error) = &page.rankings.relations.error {
            info.push(Line::from(format!("Discovery unavailable: {error}")));
        }
    }
    if page.selected_album.is_none() && !page.tab.is_artists() {
        info.push(Line::from(match page.rankings.mode {
            ArtistRanking::Catalog => "Ranking: Catalog order",
            ArtistRanking::MostPlayed => "Ranking: Your server play counts",
            ArtistRanking::Community => "Ranking: Server top songs (agent-selected)",
        }));
        if page.rankings.mode == ArtistRanking::Community {
            info.push(Line::from(format!(
                "Source: {}",
                if page.rankings.community_source.is_empty() {
                    "Navidrome getTopSongs · upstream agent not reported"
                } else {
                    &page.rankings.community_source
                }
            )));
            info.push(Line::from("F5 refreshes community popularity"));
            if page.rankings.loading {
                info.push(Line::from("Loading community popularity…"));
            }
            if let Some(error) = &page.rankings.error {
                info.push(Line::from(format!("Popularity unavailable: {error}")));
            }
        }
    }
    if page.album_loading {
        info.push(Line::from("Refreshing album details…"));
    }
    if let Some(error) = &page.album_error {
        info.push(Line::from(format!(
            "Refresh failed; showing cached details: {error}"
        )));
    }
    frame.render_widget(
        Paragraph::new(info).wrap(Wrap { trim: false }),
        Rect::new(
            left.x + u16::from(left.width > 0),
            info_y,
            left.width.saturating_sub(2),
            left.bottom().saturating_sub(info_y),
        ),
    );

    let content = page.display_tracks();
    let albums_mode = page.selected_album.is_none() && page.tab.is_albums();
    let list_title = if page.selected_album.is_some() {
        " Songs · Enter plays in shown order "
    } else {
        match page.tab {
            ArtistOverviewTab::Albums => " Releases · By type · Newest first ",
            ArtistOverviewTab::Songs => " Songs · Catalog order ",
            ArtistOverviewTab::TopAlbums => " Most played albums · Your counts ",
            ArtistOverviewTab::TopPlayed => " Top songs · Your play counts ",
            ArtistOverviewTab::Community => " Top songs · Navidrome getTopSongs ",
            ArtistOverviewTab::RelatedTags => " Related artists · Shared genre tags ",
            ArtistOverviewTab::RelatedSonic => " Related artists · Audio similarity ",
            ArtistOverviewTab::RelatedServer => " Related artists · Server-selected agent ",
        }
    };
    let mut list_block = Block::default()
        .title(list_title)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(t.colors.border));
    // The cached list remains usable while catalog refresh fills guest credits.
    // Keep progress in the border so it stays visible even in long discographies.
    if albums_mode && state.library_loading {
        list_block = list_block.title_bottom(" Loading releases / Appears in… ");
    }
    let list = list_block.inner(columns[1]);
    frame.render_widget(list_block, columns[1]);
    let row_height = 2u16;
    let grouped = albums_mode && page.tab == ArtistOverviewTab::Albums;
    let (first, album_rows) = if grouped {
        grouped_album_rows(page, page.index, page.scroll_pin, list.height)
    } else {
        (
            page.scroll_pin.unwrap_or_else(|| {
                page.index
                    .saturating_sub((list.height / row_height).max(1) as usize - 1)
            }),
            Vec::new(),
        )
    };
    let visible = if grouped {
        album_rows.len()
    } else {
        (list.height / row_height).max(1) as usize
    };
    let items: Vec<_> = if grouped {
        album_rows
            .iter()
            .map(|(index, y, header)| {
                (
                    Rect::new(
                        list.x,
                        list.y + y + u16::from(*header),
                        list.width,
                        row_height.min(list.height.saturating_sub(*y + u16::from(*header))),
                    ),
                    *index,
                )
            })
            .collect()
    } else {
        (first..page.list_len().min(first + visible))
            .enumerate()
            .map(|(row, index)| {
                (
                    Rect::new(
                        list.x,
                        list.y + row as u16 * row_height,
                        list.width,
                        row_height.min(list.height.saturating_sub(row as u16 * row_height)),
                    ),
                    index,
                )
            })
            .collect()
    };
    let rows: Vec<ListItem> = if page.tab.is_artists() {
        page.rankings
            .relations
            .artists
            .iter()
            .enumerate()
            .skip(first)
            .take(visible)
            .map(|(index, found)| {
                let style = if index == page.index {
                    Style::default()
                        .fg(t.colors.selection_text)
                        .bg(t.colors.selection_bar_bg)
                } else {
                    Style::default().fg(t.colors.fg_primary)
                };
                ListItem::new(Text::from(vec![
                    Line::styled(format!("      {}", found.artist.title), style),
                    Line::styled(format!("      {}", found.reason), style),
                ]))
                .style(style)
            })
            .collect()
    } else if albums_mode {
        page.display_albums()
            .iter()
            .enumerate()
            .skip(first)
            .take(visible)
            .map(|(index, album)| {
                let style = if index == page.index {
                    Style::default()
                        .fg(t.colors.selection_text)
                        .bg(t.colors.selection_bar_bg)
                } else {
                    Style::default().fg(t.colors.fg_primary)
                };
                let mut lines = vec![
                    Line::from(Span::styled(format!("      {}", album.title), style)),
                    Line::from(Span::styled(
                        format!(
                            "      {} · {} tracks{}",
                            album.year.map_or(String::new(), |year| year.to_string()),
                            album.track_count(),
                            if page.rankings.mode == ArtistRanking::MostPlayed {
                                page.album_plays(album)
                                    .map(|count| {
                                        format!(
                                            " · {count} {}plays",
                                            if album.play_count.is_some() {
                                                "album "
                                            } else {
                                                "track "
                                            }
                                        )
                                    })
                                    .unwrap_or_else(|| " · Play count unavailable".into())
                            } else {
                                String::new()
                            }
                        ),
                        Style::default().fg(t.colors.fg_muted),
                    )),
                ];
                if grouped && album_rows[index - first].2 {
                    let section = page.release_section(album);
                    let count = page
                        .display_albums()
                        .iter()
                        .filter(|other| page.release_section(other) == section)
                        .count();
                    lines.insert(
                        0,
                        Line::styled(
                            format!(" {} ({count})", section.label()),
                            Style::default().fg(t.colors.fg_accent).bold(),
                        ),
                    );
                }
                ListItem::new(Text::from(lines))
            })
            .collect()
    } else {
        content
            .iter()
            .enumerate()
            .skip(first)
            .take(visible)
            .map(|(index, track)| {
                let style = if index == page.index {
                    Style::default()
                        .fg(t.colors.selection_text)
                        .bg(t.colors.selection_bar_bg)
                } else {
                    Style::default().fg(t.colors.fg_primary)
                };
                ListItem::new(Text::from(vec![
                    Line::from(Span::styled(
                        format!("      {}", favorite_title(track)),
                        style,
                    )),
                    Line::from(Span::styled(
                        if page.selected_album.is_some() {
                            format!(
                                "    {} · {}",
                                track.track_artist(),
                                crate::util::format_duration(track.duration_ms())
                            )
                        } else {
                            format!(
                                "      {}{}",
                                track.album_name(),
                                if page.tab == ArtistOverviewTab::Community {
                                    let metrics = ranking_metrics(track);
                                    if metrics.is_empty() {
                                        " · Metrics not supplied".into()
                                    } else {
                                        format!(
                                            " · {}",
                                            metrics
                                                .into_iter()
                                                .take(2)
                                                .collect::<Vec<_>>()
                                                .join(" · ")
                                        )
                                    }
                                } else if page.rankings.mode == ArtistRanking::MostPlayed {
                                    track
                                        .view_count
                                        .map(|count| format!(" · {count} plays"))
                                        .unwrap_or_else(|| " · Play count unavailable".into())
                                } else {
                                    String::new()
                                }
                            )
                        },
                        Style::default().fg(t.colors.fg_muted),
                    )),
                ]))
            })
            .collect()
    };
    if rows.is_empty() {
        frame.render_widget(
            Paragraph::new(
                if page.rankings.relations.loading && page.tab.is_artists() {
                    "Loading related artists…"
                } else if page.tab.is_artists() && page.rankings.relations.error.is_some() {
                    "Related artists unavailable · F5 retries"
                } else if page.tab.is_artists() {
                    "No matching artists in this library"
                } else if albums_mode && state.library_loading {
                    "Loading releases and artist credits…"
                } else if page.album_loading {
                    "Loading songs…"
                } else if page.rankings.loading {
                    "Loading community popularity…"
                } else if page.rankings.error.is_some() {
                    "Popularity unavailable · F5 retries"
                } else if page.rankings.mode == ArtistRanking::Community {
                    "No community top songs matched this library"
                } else {
                    "No entries available"
                },
            ),
            list,
        );
    } else {
        frame.render_widget(List::new(rows), list);
        if page.tab.is_artists() {
            for (row, found) in page
                .rankings
                .relations
                .artists
                .iter()
                .skip(first)
                .take(visible)
                .enumerate()
            {
                crate::ui::artwork::render_thumbnail(
                    frame,
                    state,
                    Rect::new(
                        list.x,
                        list.y + row as u16 * row_height,
                        list.width.min(4),
                        row_height,
                    ),
                    &found.artist.rating_key,
                    found.artist.thumb.as_deref(),
                );
            }
        } else if albums_mode {
            for (row, album) in page
                .display_albums()
                .iter()
                .skip(first)
                .take(visible)
                .enumerate()
            {
                let area = if grouped {
                    let (_, y, header) = album_rows[row];
                    Rect::new(
                        list.x,
                        list.y + y + u16::from(header),
                        list.width.min(4),
                        row_height,
                    )
                } else {
                    Rect::new(
                        list.x,
                        list.y + row as u16 * row_height,
                        list.width.min(4),
                        row_height,
                    )
                };
                let thumb = album.thumb.as_deref().or_else(|| {
                    page.tracks
                        .iter()
                        .find(|track| track.parent_rating_key.as_deref() == Some(&album.rating_key))
                        .and_then(crate::library::models::Track::best_thumb)
                });
                crate::ui::artwork::render_thumbnail(frame, state, area, &album.rating_key, thumb);
            }
        } else {
            for (row, track) in content.iter().skip(first).take(visible).enumerate() {
                crate::ui::artwork::render_thumbnail(
                    frame,
                    state,
                    Rect::new(
                        list.x,
                        list.y + row as u16 * row_height,
                        list.width.min(4),
                        row_height,
                    ),
                    &track.rating_key,
                    track.best_thumb(),
                );
            }
        }
    }
    state.hit_regions.borrow_mut().artist_overview =
        Some(crate::app::presentation::ArtistOverviewRegions {
            items,
            outer,
            ranking_button: tabs
                .iter()
                .find(|(_, tab)| *tab == ArtistOverviewTab::TopPlayed)
                .map(|(rect, _)| *rect),
            tabs,
            tab_button: None,
            labels_button: None,
            list,
            first_index: first,
            row_height,
        });
}

/// Fit whole album cards with nonselectable section headings. Repeat the section
/// heading when scrolling into its middle; mouse pins keep the first album fixed.
fn grouped_album_rows(
    page: &crate::app::state::ArtistOverviewState,
    selected: usize,
    pin: Option<usize>,
    height: u16,
) -> (usize, Vec<(usize, u16, bool)>) {
    let albums = page.display_albums();
    let layout = |first: usize| {
        let mut rows = Vec::new();
        let mut y = 0u16;
        for index in first..albums.len() {
            let header = height >= 3
                && (index == first
                    || page.release_section(&albums[index - 1])
                        != page.release_section(&albums[index]));
            let needed = 2 + u16::from(header);
            if y.saturating_add(needed) > height {
                break;
            }
            rows.push((index, y, header));
            y += needed;
        }
        rows
    };
    let mut first =
        pin.unwrap_or_else(|| selected.saturating_sub((height / 2).saturating_sub(1) as usize));
    let mut rows = layout(first);
    if pin.is_none() {
        while first < selected && rows.last().is_none_or(|row| row.0 < selected) {
            first += 1;
            rows = layout(first);
        }
    }
    (first, rows)
}

fn album_metadata(
    album: &crate::library::models::Album,
    tracks: &[&crate::library::models::Track],
) -> Vec<(&'static str, String)> {
    let mut rows = Vec::new();
    let release = album
        .release_date
        .clone()
        .or_else(|| album.year.map(|year| year.to_string()))
        .or_else(|| album.originally_available_at.clone());
    rows.push(("Released", release.unwrap_or_else(|| "Not supplied".into())));
    if let Some(original) = &album.originally_available_at {
        if album.release_date.as_ref() != Some(original) {
            rows.push(("Original release", original.clone()));
        }
    }
    rows.push((
        "Label",
        if album.record_labels.is_empty() {
            "Not supplied".into()
        } else {
            album.record_labels.join(", ")
        },
    ));
    rows.push((
        "Country",
        album
            .country
            .clone()
            .unwrap_or_else(|| "Not supplied".into()),
    ));
    if let Some(version) = &album.version {
        rows.push(("Edition", version.clone()));
    }
    if !album.release_types.is_empty() {
        rows.push(("Release type", album.release_types.join(", ")));
    }
    if !album.genre.is_empty() {
        rows.push((
            "Genres",
            album
                .genre
                .iter()
                .map(|tag| tag.tag.as_str())
                .collect::<Vec<_>>()
                .join(", "),
        ));
    }
    if !album.mood.is_empty() {
        rows.push((
            "Moods",
            album
                .mood
                .iter()
                .map(|tag| tag.tag.as_str())
                .collect::<Vec<_>>()
                .join(", "),
        ));
    }
    let duration = album
        .duration
        .unwrap_or_else(|| tracks.iter().map(|track| track.duration_ms()).sum());
    rows.push((
        "Tracks",
        album
            .leaf_count
            .map_or_else(|| tracks.len().to_string(), |count| count.to_string()),
    ));
    if duration > 0 {
        rows.push(("Length", crate::util::format_duration(duration)));
    }
    let media: Vec<_> = tracks.iter().flat_map(|track| &track.media).collect();
    let codecs: std::collections::BTreeSet<_> = media
        .iter()
        .filter_map(|media| media.audio_codec.as_deref().or(media.container.as_deref()))
        .map(str::to_uppercase)
        .collect();
    if !codecs.is_empty() {
        rows.push(("Format", codecs.into_iter().collect::<Vec<_>>().join(", ")));
    }
    for (label, values, unit) in [
        (
            "Bit rate",
            media
                .iter()
                .filter_map(|media| media.bitrate)
                .collect::<Vec<_>>(),
            "kbps",
        ),
        (
            "Bit depth",
            media.iter().filter_map(|media| media.bit_depth).collect(),
            "bit",
        ),
        (
            "Sample rate",
            media
                .iter()
                .filter_map(|media| media.sampling_rate)
                .collect(),
            "Hz",
        ),
    ] {
        if let (Some(min), Some(max)) = (values.iter().min(), values.iter().max()) {
            rows.push((
                label,
                if min == max {
                    format!("{min} {unit}")
                } else {
                    format!("{min}–{max} {unit}")
                },
            ));
        }
    }
    rows
}

/// Only label returned popularity metrics; playCount and userRating are personal.
pub fn ranking_metrics(track: &crate::library::models::Track) -> Vec<String> {
    let mut metrics = Vec::new();
    if let Some(count) = track.view_count {
        metrics.push(format!("Your plays: {count}"));
    }
    for (key, value) in &track.metadata_tags {
        let name = key.to_ascii_lowercase();
        if name == "ranking source" {
            continue;
        }
        if name.contains("listen")
            || name.contains("popular")
            || name.contains("score")
            || name.contains("rank")
            || name.contains("globalplay")
            || name.contains("communityplay")
            || name.contains("lastfmplay")
            || name.contains("rating")
        {
            metrics.push(format!(
                "{}: {value}",
                if matches!(name.as_str(), "rating" | "user rating" | "userrating") {
                    "Your rating"
                } else {
                    key
                }
            ));
        }
    }
    metrics
}
