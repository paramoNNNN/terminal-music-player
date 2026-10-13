//! Related view key handling.

use crate::app::action::*;
use crossterm::event::{self, KeyCode};

use super::super::helpers::navigation::related_flat_resolve;
use crate::app::state::View;
use crate::app::Action;
use crate::app::AppState;

/// Handle Related view keys.
pub(in crate::app::handlers) fn handle_related_keys(
    key: event::KeyEvent,
    state: &mut AppState,
) -> Vec<Action> {
    if state.artist_overview.is_some() {
        return handle_artist_overview_keys(key, state);
    }
    if state.label_overview.is_some() {
        return handle_label_overview_keys(key, state);
    }
    match key.code {
        KeyCode::Esc => {
            let target = state.previous_view.take().unwrap_or(View::Browse);
            vec![NavigationAction::SetView(target).into()]
        }
        KeyCode::F(1) => vec![NavigationAction::SetView(View::Help).into()],

        KeyCode::Up => {
            state.scroll.related = None;
            vec![DataAction::ListUp.into()]
        }
        KeyCode::Down => {
            state.scroll.related = None;
            vec![DataAction::ListDown.into()]
        }
        KeyCode::PageUp => {
            state.scroll.related = None;
            vec![DataAction::ListPageUp.into()]
        }
        KeyCode::PageDown => {
            state.scroll.related = None;
            vec![DataAction::ListPageDown.into()]
        }
        KeyCode::Home => {
            state.scroll.related = None;
            vec![DataAction::ListTop.into()]
        }
        KeyCode::End => {
            state.scroll.related = None;
            vec![DataAction::ListBottom.into()]
        }

        KeyCode::Enter => activate_related_item(state),

        // Alphabet jumping: jump to first artist starting with that letter
        KeyCode::Char(c) if c.is_ascii_alphabetic() && key.modifiers.is_empty() => {
            let letter_lower = c.to_ascii_lowercase();
            let mut offset = 0;
            for group in state.related.groups.iter() {
                if group
                    .artist
                    .title
                    .chars()
                    .next()
                    .map(|ch| ch.to_ascii_lowercase() == letter_lower)
                    .unwrap_or(false)
                {
                    state.list_state.related_index = offset;
                    state.scroll.related = None;
                    break;
                }
                offset += 1 + group.albums.len();
            }
            vec![]
        }

        _ => vec![],
    }
}

fn handle_artist_overview_keys(key: event::KeyEvent, state: &mut AppState) -> Vec<Action> {
    use crate::app::state::ArtistOverviewTab;
    match key.code {
        KeyCode::Char('r' | 'R') => super::get_related_action(state),
        KeyCode::Char(number @ '1'..='8') => vec![BrowseAction::SelectArtistTab(
            ArtistOverviewTab::ALL[(number as u8 - b'1') as usize],
        )
        .into()],
        KeyCode::Char('l' | 'L') => {
            let label = state
                .artist_overview
                .as_ref()
                .and_then(|page| page.preview_album())
                .filter(|album| album.record_labels.len() == 1)
                .map(|album| album.record_labels[0].clone());
            vec![label
                .map_or(BrowseAction::OpenLabels, |label| {
                    BrowseAction::OpenLabelOverview { label }
                })
                .into()]
        }
        KeyCode::Esc | KeyCode::Backspace | KeyCode::Left => {
            let Some(page) = state.artist_overview.as_mut() else {
                return vec![];
            };
            if page.from_label_album {
                state.artist_overview = None;
                return vec![];
            }
            if page.selected_album.is_some() {
                page.selected_album = None;
                page.album_loading = false;
                page.album_error = None;
                page.album_tracks.clear();
                page.tab = ArtistOverviewTab::Albums;
                page.index = 0;
                page.scroll_pin = None;
                return vec![];
            }
            let target = page.return_view;
            state.artist_overview = None;
            vec![NavigationAction::SetView(target).into()]
        }
        KeyCode::Tab | KeyCode::BackTab => {
            let Some(page) = state
                .artist_overview
                .as_ref()
                .filter(|page| page.selected_album.is_none())
            else {
                return vec![];
            };
            let current = ArtistOverviewTab::ALL
                .iter()
                .position(|tab| *tab == page.tab)
                .unwrap_or(0);
            let next = if key.code == KeyCode::BackTab {
                (current + ArtistOverviewTab::ALL.len() - 1) % ArtistOverviewTab::ALL.len()
            } else {
                (current + 1) % ArtistOverviewTab::ALL.len()
            };
            vec![BrowseAction::SelectArtistTab(ArtistOverviewTab::ALL[next]).into()]
        }
        KeyCode::Up | KeyCode::Down | KeyCode::Home | KeyCode::End => {
            let Some(page) = state.artist_overview.as_mut() else {
                return vec![];
            };
            let total = page.list_len();
            page.index = match key.code {
                KeyCode::Up => page.index.saturating_sub(1),
                KeyCode::Down => (page.index + 1).min(total.saturating_sub(1)),
                KeyCode::Home => 0,
                KeyCode::End => total.saturating_sub(1),
                _ => page.index,
            };
            page.scroll_pin = None;
            vec![]
        }
        KeyCode::Right => {
            if state
                .artist_overview
                .as_ref()
                .is_some_and(|page| page.tab.is_albums())
            {
                activate_artist_overview(state)
            } else {
                vec![]
            }
        }
        KeyCode::Enter => activate_artist_overview(state),
        _ => vec![],
    }
}

pub(in crate::app::handlers) fn activate_artist_overview(state: &mut AppState) -> Vec<Action> {
    use crate::app::state::ArtistOverviewTab;
    let Some(page) = state.artist_overview.as_mut() else {
        return vec![];
    };
    if page.tab.is_artists() {
        return page
            .rankings
            .relations
            .artists
            .get(page.index)
            .map(|found| {
                vec![BrowseAction::OpenArtistOverview {
                    artist_key: found.artist.rating_key.clone(),
                }
                .into()]
            })
            .unwrap_or_default();
    }
    if page.selected_album.is_none() && page.tab.is_albums() {
        let Some(album) = page.display_albums().get(page.index).cloned() else {
            return vec![];
        };
        page.album_tracks = page
            .tracks
            .iter()
            .filter(|track| track.parent_rating_key.as_deref() == Some(&album.rating_key))
            .cloned()
            .collect();
        page.album_tracks
            .sort_by_key(|track| (track.parent_index.unwrap_or(1), track.index.unwrap_or(0)));
        let album_key = album.rating_key.clone();
        page.selected_album = Some(album);
        page.album_loading = true;
        page.album_error = None;
        page.tab = ArtistOverviewTab::Songs;
        page.index = 0;
        page.scroll_pin = None;
        return vec![BrowseAction::LoadOverviewAlbum { album_key }.into()];
    }
    if page.display_tracks().get(page.index).is_none() {
        return vec![];
    }
    vec![QueueAction::PlayCatalog {
        tracks: page.display_tracks().to_vec(),
        index: page.index,
    }
    .into()]
}

/// Activate the currently highlighted related item (Enter or second click).
/// Artists: navigate to artist in library. Albums: navigate to album in library.
pub(in crate::app::handlers) fn activate_related_item(state: &mut AppState) -> Vec<Action> {
    let idx = state.list_state.related_index;
    let resolved = related_flat_resolve(&state.related.groups, idx);
    let Some((group_idx, is_header, album_idx)) = resolved else {
        return vec![];
    };

    // Clone data we need before mutating state
    let (artist_key, artist_name, album_key, album_title) = {
        let group = match state.related.groups.get(group_idx) {
            Some(g) => g,
            None => return vec![],
        };
        let ak = group.artist.rating_key.clone();
        let an = group.artist.title.clone();
        if is_header {
            (ak, an, None, None)
        } else {
            let album = match group.albums.get(album_idx) {
                Some(a) => a,
                None => return vec![],
            };
            // For synthetic alias albums, use the album's parent artist key
            let effective_artist_key = if ak.starts_with("alias:") {
                album.parent_rating_key.clone().unwrap_or(ak.clone())
            } else {
                ak.clone()
            };
            (
                effective_artist_key,
                an,
                Some(album.rating_key.clone()),
                Some(album.title.clone()),
            )
        }
    };

    // Synthetic alias artist header — navigate to source artist instead
    let nav_artist_key = if artist_key.starts_with("alias:") {
        state.related.source_key.clone()
    } else {
        artist_key
    };

    state.set_view(View::Browse);
    state.set_browse_category(crate::app::state::BrowseCategory::Library, false);
    state.library.selected_artist_name = artist_name;

    if let Some(album_key_val) = album_key {
        state.search.pending_album_key = Some(album_key_val);
        state.library.selected_album_title = album_title.unwrap_or_default();
    }

    if let Some(pos) = state.artist_nav.columns.first().and_then(|col| {
        col.items
            .iter()
            .position(|i| i.key() == nav_artist_key.as_str())
    }) {
        if let Some(col) = state.artist_nav.columns.first_mut() {
            col.selected_index = pos;
        }
    }
    state.artist_nav.focused_column = 0;
    state.artist_nav.truncate_right();
    vec![MillerAction::LoadArtistAlbumsForMiller {
        artist_key: nav_artist_key,
        replace_child: false,
    }
    .into()]
}

fn handle_label_overview_keys(key: event::KeyEvent, state: &mut AppState) -> Vec<Action> {
    let Some(page) = state.label_overview.as_mut() else {
        return vec![];
    };
    match key.code {
        KeyCode::Esc | KeyCode::Backspace | KeyCode::Left => {
            if page.label.is_some() {
                page.label = None;
                page.albums.clear();
                page.artists.clear();
                page.index = 0;
                page.scroll_pin = None;
                page.artists_tab = false;
                return vec![];
            }
            let target = page.return_view;
            state.label_overview = None;
            vec![NavigationAction::SetView(target).into()]
        }
        KeyCode::Tab if page.label.is_some() => {
            page.artists_tab = !page.artists_tab;
            page.index = 0;
            page.scroll_pin = None;
            vec![]
        }
        KeyCode::Up
        | KeyCode::Down
        | KeyCode::Home
        | KeyCode::End
        | KeyCode::PageUp
        | KeyCode::PageDown => {
            let max = page.len().saturating_sub(1);
            page.index = match key.code {
                KeyCode::Up => page.index.saturating_sub(1),
                KeyCode::Down => (page.index + 1).min(max),
                KeyCode::Home => 0,
                KeyCode::End => max,
                KeyCode::PageUp => page.index.saturating_sub(10),
                _ => (page.index + 10).min(max),
            };
            page.scroll_pin = None;
            vec![]
        }
        KeyCode::Enter | KeyCode::Right => activate_label_overview(state),
        _ => vec![],
    }
}

pub(in crate::app::handlers) fn activate_label_overview(state: &mut AppState) -> Vec<Action> {
    let Some(page) = state.label_overview.as_ref() else {
        return vec![];
    };
    let action = if page.label.is_none() {
        page.labels
            .get(page.index)
            .map(|label| BrowseAction::OpenLabelOverview {
                label: label.clone(),
            })
    } else if page.artists_tab {
        page.artists
            .get(page.index)
            .map(|artist| BrowseAction::OpenArtistOverview {
                artist_key: artist.rating_key.clone(),
            })
    } else {
        page.albums
            .get(page.index)
            .map(|album| BrowseAction::OpenLabelAlbum {
                album_key: album.rating_key.clone(),
            })
    };
    action.map(|action| vec![action.into()]).unwrap_or_default()
}
