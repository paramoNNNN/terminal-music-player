//! Artist discovery always takes its seeds from the visible artist page.
use super::*;
use crate::app::sources::navidrome::recommendations::{self, Recommendations};
use crate::app::state::{ArtistMatch, ArtistOverviewTab};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
struct Discovery {
    artists: Vec<ArtistMatch>,
    source: String,
}

pub fn intercept(
    action: &Action,
    state: &mut AppState,
    tx: &mpsc::Sender<Event>,
    session: &Session,
) -> Option<Vec<Action>> {
    let Action::Browse(BrowseAction::LoadArtistRelations { tab, refresh }) = action else {
        return None;
    };
    let tab = *tab;
    let page = state
        .artist_overview
        .as_mut()
        .filter(|page| page.tab == tab)?;
    if !matches!(
        tab,
        ArtistOverviewTab::RelatedServer | ArtistOverviewTab::RelatedSonic
    ) {
        return None;
    }
    let artist_key = page.artist.rating_key.clone();
    let artist_name = page.artist.title.clone();
    let mut tracks = page.tracks.clone();
    tracks.sort_by_key(|track| std::cmp::Reverse(track.view_count.unwrap_or(0)));
    let seeds: Vec<_> = tracks
        .into_iter()
        .take(3)
        .map(|track| track.rating_key)
        .collect();
    page.rankings.relations.loading = true;
    page.rankings.relations.error = None;
    let artists = state
        .library
        .artists
        .iter()
        .chain(&state.library.track_artists)
        .cloned()
        .collect::<Vec<_>>();
    let request_id = state.artist_overview_request_id;
    let library_key = state.active_library.clone().unwrap_or_default();
    let api = Recommendations::new(state, session);
    let available = recommendations::available(state);
    let refresh = *refresh;
    let session = session.clone();
    let source = if tab == ArtistOverviewTab::RelatedSonic {
        "Audio similarity from this artist's songs · Navidrome Sonic / configured AudioMuse fallback"
    } else { "Navidrome getArtistInfo2 · upstream agent not reported (may use metadata or sonic analysis)" }.to_owned();
    let ticket = session
        .cache_store()
        .map(|store| store.ticket(&format!("artist-discovery:{tab:?}:{artist_key}")));
    if let Ok(ticket) = &ticket {
        match ticket.read::<Discovery>(crate::library::cache::REFRESH_INTERVAL) {
            Ok(Some(mut hit)) => {
                hit.value.artists.retain(|found| {
                    artists
                        .iter()
                        .any(|artist| artist.rating_key == found.artist.rating_key)
                });
                if !refresh && !hit.stale {
                    return Some(vec![BrowseAction::ArtistRelationsLoaded {
                        library_key,
                        artist_key,
                        request_id,
                        tab,
                        source: hit.value.source,
                        result: Ok(hit.value.artists),
                    }
                    .into()]);
                }
                let page = state.artist_overview.as_mut().unwrap();
                page.rankings.relations.artists = hit.value.artists;
                page.rankings.relations.source = hit.value.source;
            }
            Err(error) => tracing::warn!("Read artist discovery cache: {error}"),
            _ => {}
        }
    }
    spawn(state, tx, "overview-relations", async move {
        let result = async {
            let ticket = ticket?;
            let found = if tab == ArtistOverviewTab::RelatedServer {
                let response = session
                    .client
                    .call(
                        "getArtistInfo2",
                        &[
                            ("id", session.id(&artist_key)?),
                            ("count", "50".into()),
                            ("includeNotPresent", "false".into()),
                        ],
                    )
                    .await?;
                let related: Vec<crate::navidrome::Artist> =
                    crate::navidrome::array(&response["artistInfo2"], "similarArtist")?;
                related
                    .into_iter()
                    .filter_map(|artist| {
                        artists
                            .iter()
                            .find(|known| known.rating_key == session.key(&artist.id))
                            .cloned()
                    })
                    .filter(|artist| artist.rating_key != artist_key)
                    .map(|artist| ArtistMatch {
                        artist,
                        reason: "Server-related artist · similarity method not reported".into(),
                    })
                    .collect()
            } else {
                anyhow::ensure!(
                    available,
                    "Sonic artist discovery requires Navidrome sonicSimilarity or direct AudioMuse"
                );
                anyhow::ensure!(
                    !seeds.is_empty(),
                    "No songs for this artist are cached; refresh the library first"
                );
                let count = seeds.len();
                let batches = futures::future::try_join_all(seeds.into_iter().map(|seed| {
                    let api = &api;
                    let session = &session;
                    async move { api.similar(&session.id(&seed)?, true).await }
                }))
                .await?;
                let mut matches: std::collections::HashMap<
                    String,
                    (
                        crate::library::models::Artist,
                        std::collections::HashSet<String>,
                    ),
                > = std::collections::HashMap::new();
                for track in batches.into_iter().flatten() {
                    if let Some(artist) = artists
                        .iter()
                        .find(|artist| {
                            Some(&artist.rating_key) == track.grandparent_rating_key.as_ref()
                        })
                        .or_else(|| {
                            artists.iter().find(|artist| {
                                artist.title.eq_ignore_ascii_case(track.track_artist())
                            })
                        })
                    {
                        if artist.rating_key == artist_key
                            || artist.title.eq_ignore_ascii_case(&artist_name)
                        {
                            continue;
                        }
                        matches
                            .entry(artist.rating_key.clone())
                            .or_insert_with(|| (artist.clone(), Default::default()))
                            .1
                            .insert(track.rating_key);
                    }
                }
                let mut matches: Vec<_> = matches.into_values().collect();
                matches.sort_by(|a, b| {
                    b.1.len()
                        .cmp(&a.1.len())
                        .then_with(|| a.0.title.cmp(&b.0.title))
                });
                matches
                    .into_iter()
                    .take(50)
                    .map(|(artist, songs)| ArtistMatch {
                        artist,
                        reason: format!(
                            "{} unique matching songs · {count} seed songs",
                            songs.len()
                        ),
                    })
                    .collect()
            };
            let snapshot = Discovery {
                artists: found,
                source: source.clone(),
            };
            if let Err(error) = ticket.write(&snapshot) {
                tracing::warn!("Cache artist discovery: {error}");
            }
            Ok(snapshot.artists)
        }
        .await
        .map_err(failure);
        Ok(vec![BrowseAction::ArtistRelationsLoaded {
            library_key,
            artist_key,
            request_id,
            tab,
            source,
            result,
        }
        .into()])
    });
    Some(vec![])
}
