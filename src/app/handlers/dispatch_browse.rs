//! Browse dispatch handlers: LoadStations, LoadTagList, LoadTagAlbums,
//! RefreshTagView, OpenTrackDetails, OpenInLibrary.

use crate::app::action::{BrowseAction, MillerAction, SystemAction};
use crate::app::state::{BrowseCategory, BrowseColumn, BrowseItem, RefreshCategory, StationColumn};
use crate::app::{Action, AppState, Event};

use anyhow::Result;

use super::helpers;
use tokio::sync::mpsc;

/// Dispatch browse-related actions. Returns follow-up actions.
pub async fn dispatch(
    _event_tx: &mpsc::Sender<Event>,
    action: BrowseAction,
    state: &mut AppState,
) -> Result<Vec<Action>> {
    let mut follow_ups = vec![];

    match action {
        BrowseAction::SelectArtistTab(tab) => {
            use crate::app::state::{ArtistOverviewTab, ArtistRanking};
            let Some(page) = state
                .artist_overview
                .as_mut()
                .filter(|page| page.selected_album.is_none())
            else {
                return Ok(vec![]);
            };
            page.tab = tab;
            page.set_ranking(match tab {
                ArtistOverviewTab::TopAlbums | ArtistOverviewTab::TopPlayed => {
                    ArtistRanking::MostPlayed
                }
                ArtistOverviewTab::Community => ArtistRanking::Community,
                _ => ArtistRanking::Catalog,
            });
            if tab == ArtistOverviewTab::Community
                && !page.rankings.loaded
                && !page.rankings.loading
            {
                follow_ups.push(BrowseAction::LoadArtistCommunity { refresh: false }.into());
            }
            if tab.is_artists() {
                page.rankings.relations = Default::default();
                if tab == ArtistOverviewTab::RelatedTags {
                    let key = page.artist.rating_key.clone();
                    let matches = tag_related_artists(state, &key);
                    let page = state.artist_overview.as_mut().unwrap();
                    page.rankings.relations.artists = matches;
                    page.rankings.relations.source =
                        "Shared genre tags in this library (tag overlap, not audio analysis)"
                            .into();
                } else {
                    follow_ups.push(
                        BrowseAction::LoadArtistRelations {
                            tab,
                            refresh: false,
                        }
                        .into(),
                    );
                }
            }
        }
        BrowseAction::LoadArtistRelations { .. } => {
            if let Some(page) = state.artist_overview.as_mut() {
                page.rankings.relations.error =
                    Some("Related-artist discovery is unavailable for this source".into());
            }
        }
        BrowseAction::ArtistRelationsLoaded {
            library_key,
            artist_key,
            request_id,
            tab,
            source,
            result,
        } => {
            if state.active_library.as_ref() != Some(&library_key)
                || state.artist_overview_request_id != request_id
            {
                return Ok(vec![]);
            }
            if let Some(page) = state
                .artist_overview
                .as_mut()
                .filter(|page| page.artist.rating_key == artist_key && page.tab == tab)
            {
                page.rankings.relations.loading = false;
                page.rankings.relations.source = source;
                match result {
                    Ok(artists) => {
                        page.rankings.relations.artists = artists;
                        page.rankings.relations.error = None;
                    }
                    Err(error) => page.rankings.relations.error = Some(error.message),
                }
            }
        }
        BrowseAction::OpenLabels => {
            let page = label_page(state, None);
            state.artist_overview = None;
            state.label_overview = Some(page);
            state.set_view(crate::app::state::View::Related);
        }
        BrowseAction::OpenLabelOverview { label } => {
            let mut page = label_page(state, Some(label));
            if let Some(previous) = state
                .label_overview
                .as_ref()
                .filter(|_| state.view == crate::app::state::View::Related)
            {
                page.return_view = previous.return_view;
            }
            state.artist_overview = None;
            state.label_overview = Some(page);
            state.set_view(crate::app::state::View::Related);
        }
        BrowseAction::OpenLabelAlbum { album_key } => {
            let Some(album) = state
                .label_overview
                .as_ref()
                .and_then(|page| page.albums.iter().find(|a| a.rating_key == album_key))
                .cloned()
            else {
                return Ok(follow_ups);
            };
            let tracks = state
                .library
                .all_tracks
                .iter()
                .filter(|track| track.parent_rating_key.as_deref() == Some(&album_key))
                .cloned()
                .collect();
            let artist = state
                .library
                .artists
                .iter()
                .chain(&state.library.track_artists)
                .find(|artist| Some(&artist.rating_key) == album.parent_rating_key.as_ref())
                .cloned()
                .unwrap_or_else(|| crate::library::models::Artist {
                    rating_key: album.parent_rating_key.clone().unwrap_or_default(),
                    title: album.artist_name().to_owned(),
                    ..Default::default()
                });
            state.artist_overview_request_id = state.artist_overview_request_id.wrapping_add(1);
            state.artist_overview = Some(crate::app::state::ArtistOverviewState {
                rankings: Default::default(),
                from_label_album: true,
                artist,
                albums: vec![album.clone()],
                tracks: vec![],
                album_tracks: tracks,
                selected_album: Some(album),
                album_loading: true,
                album_error: None,
                tab: crate::app::state::ArtistOverviewTab::Songs,
                index: 0,
                scroll_pin: None,
                return_view: crate::app::state::View::Related,
            });
            follow_ups.push(BrowseAction::LoadOverviewAlbum { album_key }.into());
        }
        BrowseAction::CycleArtistRanking => {
            use crate::app::state::{ArtistOverviewTab, ArtistRanking};
            if let Some(page) = state
                .artist_overview
                .as_ref()
                .filter(|page| page.selected_album.is_none())
            {
                let next = match page.rankings.mode {
                    ArtistRanking::Catalog => ArtistOverviewTab::TopPlayed,
                    ArtistRanking::MostPlayed => ArtistOverviewTab::Community,
                    ArtistRanking::Community => ArtistOverviewTab::Songs,
                };
                follow_ups.push(BrowseAction::SelectArtistTab(next).into());
            }
        }
        BrowseAction::LoadArtistCommunity { .. } => {
            if let Some(page) = state.artist_overview.as_mut() {
                page.rankings.loading = false;
                page.rankings.error =
                    Some("Community popularity is unavailable for this source".into());
            }
        }
        BrowseAction::ArtistCommunityLoaded {
            library_key,
            artist_key,
            request_id,
            result,
        } => {
            if state.active_library.as_ref() != Some(&library_key)
                || state.artist_overview_request_id != request_id
            {
                return Ok(follow_ups);
            }
            if let Some(page) = state
                .artist_overview
                .as_mut()
                .filter(|page| page.artist.rating_key == artist_key)
            {
                page.rankings.loading = false;
                match result {
                    Ok(tracks) => {
                        page.rankings.community_source = tracks
                            .first()
                            .and_then(|track| track.metadata_tags.get("Ranking source"))
                            .cloned()
                            .unwrap_or_else(|| {
                                "Navidrome getTopSongs · upstream agent not reported".into()
                            });
                        page.rankings.community = tracks;
                        page.rankings.loaded = true;
                        page.rankings.error = None;
                        if page.rankings.mode == crate::app::state::ArtistRanking::Community {
                            let pin = page.scroll_pin;
                            let index = page.index;
                            page.set_ranking(crate::app::state::ArtistRanking::Community);
                            page.index = index.min(
                                if page.tab.is_albums() {
                                    page.display_albums().len()
                                } else {
                                    page.display_tracks().len()
                                }
                                .saturating_sub(1),
                            );
                            page.scroll_pin = pin;
                        }
                    }
                    Err(error) => page.rankings.error = Some(error.message),
                }
            }
        }

        BrowseAction::LoadOverviewAlbum { .. } => {
            state.set_error("Album details are unavailable for the active source".into());
        }
        BrowseAction::OverviewAlbumLoaded {
            library_key,
            album_key,
            result,
        } => {
            if state.active_library.as_ref() != Some(&library_key) {
                return Ok(follow_ups);
            }
            let Some(page) = state.artist_overview.as_mut().filter(|page| {
                page.selected_album
                    .as_ref()
                    .is_some_and(|album| album.rating_key == album_key)
            }) else {
                return Ok(follow_ups);
            };
            page.album_loading = false;
            match result {
                Ok((album, mut tracks)) => {
                    let album = *album;
                    tracks.sort_by_key(|track| {
                        (track.parent_index.unwrap_or(1), track.index.unwrap_or(0))
                    });
                    if let Some(cached) = page.albums.iter_mut().find(|a| a.rating_key == album_key)
                    {
                        *cached = album.clone();
                    }
                    page.selected_album = Some(album.clone());
                    page.album_tracks = tracks;
                    page.index = page.index.min(page.album_tracks.len().saturating_sub(1));
                    page.album_error = None;
                    let (index, pin) = (page.index, page.scroll_pin);
                    page.set_ranking(page.rankings.mode);
                    page.index = index;
                    page.scroll_pin = pin;
                    if let Some(cached) = state
                        .library
                        .albums
                        .iter_mut()
                        .find(|a| a.rating_key == album_key)
                    {
                        *cached = album;
                    }
                }
                Err(error) => page.album_error = Some(error.message),
            }
        }
        BrowseAction::OpenArtistOverview { artist_key } => {
            let Some(artist) = state
                .library
                .artists
                .iter()
                .chain(state.library.track_artists.iter())
                .find(|artist| artist.rating_key == artist_key)
                .cloned()
            else {
                state.set_error("Artist is not in the active library cache".into());
                return Ok(follow_ups);
            };
            let (albums, tracks) = artist_catalog(state, &artist);
            if state.view != crate::app::state::View::Related {
                state.label_overview = None;
            }
            state.artist_overview_request_id = state.artist_overview_request_id.wrapping_add(1);
            let return_view = if state.view == crate::app::state::View::Related
                && state.label_overview.is_none()
            {
                state
                    .artist_overview
                    .as_ref()
                    .map(|page| page.return_view)
                    .filter(|view| *view != crate::app::state::View::Related)
                    .unwrap_or(crate::app::state::View::Browse)
            } else {
                state.view
            };
            state.previous_view = Some(state.view);
            state.artist_overview = Some(crate::app::state::ArtistOverviewState {
                rankings: Default::default(),
                from_label_album: false,
                artist,
                albums,
                tracks,
                album_tracks: Vec::new(),
                selected_album: None,
                album_loading: false,
                album_error: None,
                tab: crate::app::state::ArtistOverviewTab::Albums,
                index: 0,
                scroll_pin: None,
                return_view,
            });
            state.artist_overview.as_mut().unwrap().sort_releases();
            state.set_view(crate::app::state::View::Related);
        }
        BrowseAction::StationsLoaded {
            library_key,
            result,
        } => {
            if state.active_library.as_ref() != Some(&library_key) {
                return Ok(follow_ups);
            }
            state.stations_loading = false;
            state.station_nav.loading = false;
            match result {
                Ok(mut stations) => {
                    helpers::append_station_action_items(
                        &mut stations,
                        state.queue.shuffle_undo_queue.is_some(),
                    );
                    state.station_nav.columns.clear();
                    state.station_nav.columns.push(StationColumn::new(
                        None,
                        "stations".to_string(),
                        stations.clone(),
                    ));
                    state.station_nav.focused_column = 0;
                    state.stations = stations;
                }
                Err(error) => {
                    state.set_error(error.message);
                }
            }
        }

        BrowseAction::TagListLoaded {
            library_key,
            section,
            result,
        } => {
            if state.active_library.as_ref() != Some(&library_key) {
                return Ok(follow_ups);
            }
            set_tag_loading(state, section, false);
            match result {
                Ok(items) => {
                    store_tag_list(state, section, items);
                    if state.browse_category == section {
                        follow_ups.push(BrowseAction::RefreshTagView.into());
                    } else {
                        state.tag_nav.loading = false;
                    }
                }
                Err(error) => {
                    state.set_error(error.message);
                    state.tag_nav.loading = false;
                }
            }
        }

        BrowseAction::TagAlbumsLoaded {
            library_key,
            section,
            tag_key,
            tag_title,
            replace_child,
            result,
        } => {
            let selected_tag_matches = state
                .tag_nav
                .columns
                .first()
                .and_then(|column| column.items.get(column.selected_index))
                .is_some_and(
                    |item| matches!(item, BrowseItem::Genre { key, .. } if key == &tag_key),
                );
            if state.active_library.as_ref() != Some(&library_key)
                || state.browse_category != section
                || !selected_tag_matches
            {
                return Ok(follow_ups);
            }
            state.library.right_panel_loading = false;
            match result {
                Ok(mut albums) => {
                    albums.sort_by_cached_key(|album| {
                        album.parent_title.as_deref().unwrap_or("").to_lowercase()
                    });
                    state.library.tag_albums = albums.clone();
                    let items =
                        BrowseItem::from_albums(&albums, &state.library.album_display_artist);
                    let col = BrowseColumn::new(tag_title, items);
                    state.tag_nav.drill_column(col, replace_child);
                }
                Err(error) => {
                    state.set_error(error.message);
                }
            }
        }
        BrowseAction::RefreshTagView => {
            // Populate column 0 of tag_nav with the active section's tag list.
            let section = state.browse_category;
            if !section.is_tag_section() {
                return Ok(follow_ups);
            }
            let list_empty = state.tag_list_for(section).is_empty();
            if list_empty && !is_tag_loading(state, section) {
                follow_ups.push(BrowseAction::LoadTagList(section).into());
                state.tag_nav.loading = true;
                return Ok(follow_ups);
            }
            let items = BrowseItem::from_genres(state.tag_list_for(section));
            state.tag_nav.reset(section.name(), items);
            state.tag_nav.loading = false;
        }
        BrowseAction::OpenTrackDetails => {
            // The pane is a passive viewer of `focused_track()` — it
            // opens but does NOT steal keyboard focus. The user keeps
            // arrow-keying in the tracks column; the pane content
            // updates automatically as the focused track changes.
            //
            // To act on the pane (play, navigate similar tracks),
            // the user explicitly moves focus to it via Right when
            // they're on a Track row that already has the pane open
            // — see the per-category Miller-column handlers. This
            // satisfies the "Up/Down stays in one column, Left/Right
            // crosses columns" rule.
            if let Some(track) = state.focused_track() {
                if let Some(album_key) = track.parent_rating_key.clone() {
                    if !state.artwork.grid_cache.contains_key(&album_key)
                        && !state.artwork.grid_pending.contains(&album_key)
                    {
                        if let Some(thumb) = track.parent_thumb.clone() {
                            follow_ups
                                .push(SystemAction::LoadAlbumArt(vec![(album_key, thumb)]).into());
                        }
                    }
                }
            }
            state.track_pane_open = true;
        }
        BrowseAction::CloseTrackDetails => {
            state.track_pane_open = false;
        }
        BrowseAction::OpenInLibrary {
            artist_key,
            artist_name,
            album_key,
            album_title,
        } => {
            state.track_pane_open = false;
            if let Some(ak) = album_key {
                state.search.pending_album_key = Some(ak);
            }
            if let Some(at) = album_title {
                state.library.selected_album_title = at;
            }
            state.library.selected_artist_name = artist_name;
            state.set_view(crate::app::state::View::Browse);
            state.set_browse_category(crate::app::state::BrowseCategory::Library, false);

            if state.artist_nav.columns.is_empty() {
                let items = state.build_artist_root_items();
                let title = format!("artists ({})", state.library.artists.len());
                state
                    .artist_nav
                    .columns
                    .push(BrowseColumn::new(title, items));
            } else {
                state.artist_nav.columns.truncate(1);
            }
            state.artist_nav.focused_column = 0;

            if let Some(col) = state.artist_nav.columns.first_mut() {
                if let Some(idx) = col.items.iter().position(
                    |item| matches!(item, BrowseItem::Artist { key, .. } if key == &artist_key),
                ) {
                    col.selected_index = idx;
                }
            }

            if let Some(idx) = state
                .library
                .artists
                .iter()
                .position(|a| a.rating_key == artist_key)
            {
                state.list_state.artists_index = idx;
            }

            follow_ups.push(
                MillerAction::LoadArtistAlbumsForMiller {
                    artist_key,
                    replace_child: false,
                }
                .into(),
            );
        }
        _ => anyhow::bail!("Unsupported library operation reached shared handler"),
    }
    Ok(follow_ups)
}

fn set_tag_loading(state: &mut AppState, section: BrowseCategory, val: bool) {
    match section {
        BrowseCategory::AlbumGenres => state.library.album_genres_loading = val,
        BrowseCategory::ArtistGenres => state.library.artist_genres_loading = val,
        BrowseCategory::Moods => state.library.moods_loading = val,
        BrowseCategory::Styles => state.library.styles_loading = val,
        BrowseCategory::Decades => state.library.decades_loading = val,
        BrowseCategory::Years => state.library.years_loading = val,
        BrowseCategory::Collections => state.library.collections_loading = val,
        BrowseCategory::Countries => state.library.countries_loading = val,
        BrowseCategory::Labels => state.library.labels_loading = val,
        BrowseCategory::Formats => state.library.formats_loading = val,
        BrowseCategory::Studios => state.library.studios_loading = val,
        _ => {}
    }
}

fn is_tag_loading(state: &AppState, section: BrowseCategory) -> bool {
    match section {
        BrowseCategory::AlbumGenres => state.library.album_genres_loading,
        BrowseCategory::ArtistGenres => state.library.artist_genres_loading,
        BrowseCategory::Moods => state.library.moods_loading,
        BrowseCategory::Styles => state.library.styles_loading,
        BrowseCategory::Decades => state.library.decades_loading,
        BrowseCategory::Years => state.library.years_loading,
        BrowseCategory::Collections => state.library.collections_loading,
        BrowseCategory::Countries => state.library.countries_loading,
        BrowseCategory::Labels => state.library.labels_loading,
        BrowseCategory::Formats => state.library.formats_loading,
        BrowseCategory::Studios => state.library.studios_loading,
        _ => false,
    }
}

fn store_tag_list(
    state: &mut AppState,
    section: BrowseCategory,
    items: Vec<crate::library::models::Genre>,
) {
    match section {
        BrowseCategory::AlbumGenres => state.library.album_genres = items,
        BrowseCategory::ArtistGenres => state.library.artist_genres = items,
        BrowseCategory::Moods => state.library.moods = items,
        BrowseCategory::Styles => state.library.styles = items,
        BrowseCategory::Decades => state.library.decades = items,
        BrowseCategory::Years => state.library.years = items,
        BrowseCategory::Collections => state.library.collections = items,
        BrowseCategory::Countries => state.library.countries = items,
        BrowseCategory::Labels => state.library.labels = items,
        BrowseCategory::Formats => state.library.formats = items,
        BrowseCategory::Studios => state.library.studios = items,
        _ => {}
    }
}

// Helper exposed for other dispatch handlers that need to know if the
// currently-active tag section's data is already loaded.
pub fn current_tag_section_refresh(_state: &AppState) -> Option<RefreshCategory> {
    RefreshCategory::for_tag_section(_state.browse_category)
}

fn label_page(state: &AppState, label: Option<String>) -> crate::app::state::LabelOverviewState {
    let mut names = std::collections::BTreeMap::new();
    for name in state
        .library
        .albums
        .iter()
        .flat_map(|album| &album.record_labels)
    {
        if !name.trim().is_empty() {
            names
                .entry(name.trim().to_lowercase())
                .or_insert_with(|| name.trim().to_owned());
        }
    }
    let albums: Vec<_> = state
        .library
        .albums
        .iter()
        .filter(|album| {
            label.as_ref().is_some_and(|label| {
                album
                    .record_labels
                    .iter()
                    .any(|name| name.trim().eq_ignore_ascii_case(label.trim()))
            })
        })
        .cloned()
        .collect();
    let artists = state
        .library
        .artists
        .iter()
        .chain(&state.library.track_artists)
        .filter(|artist| {
            albums.iter().any(|album| {
                album.parent_rating_key.as_ref() == Some(&artist.rating_key)
                    || album.artist_name().eq_ignore_ascii_case(&artist.title)
            })
        })
        .fold(
            Vec::<crate::library::models::Artist>::new(),
            |mut artists, artist| {
                if !artists
                    .iter()
                    .any(|existing| existing.rating_key == artist.rating_key)
                {
                    artists.push(artist.clone());
                }
                artists
            },
        );
    crate::app::state::LabelOverviewState {
        labels: names.into_values().collect(),
        label,
        albums,
        artists,
        artists_tab: false,
        index: 0,
        scroll_pin: None,
        return_view: if state.view == crate::app::state::View::Related {
            state
                .artist_overview
                .as_ref()
                .map(|page| page.return_view)
                .filter(|view| *view != crate::app::state::View::Related)
                .unwrap_or(crate::app::state::View::Browse)
        } else {
            state.view
        },
    }
}

/// Match album ownership and individual performer credits within this library.
fn artist_catalog(
    state: &AppState,
    artist: &crate::library::models::Artist,
) -> (
    Vec<crate::library::models::Album>,
    Vec<crate::library::models::Track>,
) {
    let albums: Vec<_> = state
        .library
        .albums
        .iter()
        .filter(|album| {
            album.parent_rating_key.as_deref() == Some(artist.rating_key.as_str())
                || album
                    .artist_credits
                    .iter()
                    .any(|credit| credit.matches(artist))
        })
        .cloned()
        .collect();
    let album_keys: std::collections::HashSet<_> = albums
        .iter()
        .map(|album| album.rating_key.as_str())
        .collect();
    let tracks: Vec<_> = state
        .library
        .all_tracks
        .iter()
        .filter(|track| {
            track.is_credited_to(artist)
                || track
                    .parent_rating_key
                    .as_deref()
                    .is_some_and(|key| album_keys.contains(key))
        })
        .cloned()
        .collect();
    let track_albums: std::collections::HashSet<_> = tracks
        .iter()
        .filter_map(|track| track.parent_rating_key.as_deref())
        .collect();
    let mut known: std::collections::HashSet<_> = albums
        .iter()
        .map(|album| album.rating_key.clone())
        .collect();
    let mut albums = albums;
    for album in &state.library.albums {
        if track_albums.contains(album.rating_key.as_str())
            && known.insert(album.rating_key.clone())
        {
            albums.push(album.clone());
        }
    }
    (albums, tracks)
}

/// Rebuild a visible discography after catalog refresh without moving a pinned
/// selection to a different release when dates or individual credits arrive.
pub fn refresh_artist_overview(state: &mut AppState) {
    let Some(mut page) = state.artist_overview.take() else {
        return;
    };
    if page.from_label_album {
        state.artist_overview = Some(page);
        return;
    }
    let selected = if page.tab.is_artists() {
        None
    } else if page.tab.is_albums() {
        page.display_albums()
            .get(page.index)
            .map(|album| album.rating_key.clone())
    } else {
        page.display_tracks()
            .get(page.index)
            .map(|track| track.rating_key.clone())
    };
    let (index, pin, mode) = (page.index, page.scroll_pin, page.rankings.mode);
    let (albums, tracks) = artist_catalog(state, &page.artist);
    page.albums = albums;
    page.tracks = tracks;
    page.set_ranking(mode);
    page.index = selected
        .and_then(|key| {
            if page.tab.is_albums() {
                page.display_albums()
                    .iter()
                    .position(|album| album.rating_key == key)
            } else {
                page.display_tracks()
                    .iter()
                    .position(|track| track.rating_key == key)
            }
        })
        .unwrap_or(index)
        .min(page.list_len().saturating_sub(1));
    page.scroll_pin = pin.map(|first| first.min(page.list_len().saturating_sub(1)));
    state.artist_overview = Some(page);
}

pub fn refresh_label_overview(state: &mut AppState) {
    let Some(old) = state.label_overview.take() else {
        return;
    };
    let selected = if old.label.is_none() {
        old.labels.get(old.index).cloned()
    } else if old.artists_tab {
        old.artists
            .get(old.index)
            .map(|artist| artist.rating_key.clone())
    } else {
        old.albums
            .get(old.index)
            .map(|album| album.rating_key.clone())
    };
    let mut page = label_page(state, old.label);
    page.return_view = old.return_view;
    page.artists_tab = old.artists_tab;
    page.index = selected
        .and_then(|key| {
            if page.label.is_none() {
                page.labels
                    .iter()
                    .position(|label| label.eq_ignore_ascii_case(&key))
            } else if page.artists_tab {
                page.artists
                    .iter()
                    .position(|artist| artist.rating_key == key)
            } else {
                page.albums.iter().position(|album| album.rating_key == key)
            }
        })
        .unwrap_or(old.index.min(page.len().saturating_sub(1)));
    page.scroll_pin = old
        .scroll_pin
        .map(|pin| pin.min(page.len().saturating_sub(1)));
    state.label_overview = Some(page);
}

fn tag_related_artists(state: &AppState, seed: &str) -> Vec<crate::app::state::ArtistMatch> {
    let mut tags: std::collections::HashMap<String, std::collections::BTreeSet<String>> =
        std::collections::HashMap::new();
    for artist in state
        .library
        .artists
        .iter()
        .chain(&state.library.track_artists)
    {
        tags.entry(artist.rating_key.clone()).or_default().extend(
            artist
                .genre
                .iter()
                .map(|tag| tag.tag.trim().to_lowercase())
                .filter(|tag| !tag.is_empty()),
        );
    }
    for album in &state.library.albums {
        if let Some(key) = &album.parent_rating_key {
            tags.entry(key.clone()).or_default().extend(
                album
                    .genre
                    .iter()
                    .map(|tag| tag.tag.trim().to_lowercase())
                    .filter(|tag| !tag.is_empty()),
            );
        }
    }
    for track in &state.library.all_tracks {
        if let Some(key) = &track.grandparent_rating_key {
            tags.entry(key.clone()).or_default().extend(
                track
                    .genres
                    .iter()
                    .map(|tag| tag.trim().to_lowercase())
                    .filter(|tag| !tag.is_empty()),
            );
        }
    }
    let Some(seed_tags) = tags.get(seed) else {
        return vec![];
    };
    let mut seen = std::collections::HashSet::new();
    let mut artists: Vec<_> = state
        .library
        .artists
        .iter()
        .chain(&state.library.track_artists)
        .filter(|artist| artist.rating_key != seed && seen.insert(artist.rating_key.clone()))
        .filter_map(|artist| {
            let candidate = tags.get(&artist.rating_key)?;
            let shared: Vec<_> = seed_tags.intersection(candidate).cloned().collect();
            if shared.is_empty() {
                return None;
            }
            let overlap = shared.len() as f64 / seed_tags.union(candidate).count() as f64;
            Some((
                overlap,
                crate::app::state::ArtistMatch {
                    artist: artist.clone(),
                    reason: format!(
                        "{:.0}% tag overlap · {}",
                        overlap * 100.0,
                        shared.join(", ")
                    ),
                },
            ))
        })
        .collect();
    artists.sort_by(|a, b| {
        b.0.total_cmp(&a.0)
            .then_with(|| a.1.artist.title.cmp(&b.1.artist.title))
    });
    artists
        .into_iter()
        .take(50)
        .map(|(_, artist)| artist)
        .collect()
}
