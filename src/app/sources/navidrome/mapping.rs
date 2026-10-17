use super::Session;
use crate::library::catalog::{Album, Artist, GenreTag, Playlist};
use crate::library::track::TrackOrigin;
use crate::library::track::{Media, MediaPart, Track};
use crate::navidrome::models as n;

fn clean(s: String) -> String {
    crate::util::sanitize_display_text(&s).into_owned()
}
impl Session {
    pub fn artist(&self, a: n::Artist) -> Artist {
        Artist {
            rating_key: self.key(&a.id),
            key: self.key(&a.id),
            title: clean(a.name),
            thumb: a.cover_art.map(|s| self.key(&s)),
            ..Default::default()
        }
    }
    pub fn album(&self, a: n::Album) -> Album {
        let mut seen = std::collections::HashSet::new();
        let names: Vec<_> = if a.genres.is_empty() {
            a.genre.into_iter().collect()
        } else {
            a.genres.into_iter().map(|g| g.name).collect()
        };
        let genres: Vec<_> = names
            .into_iter()
            .map(|s| clean(s).trim().to_owned())
            .filter(|s| !s.is_empty() && seen.insert(s.clone()))
            .map(|tag| GenreTag {
                tag,
                ..Default::default()
            })
            .collect();
        Album {
            artist_credits: a
                .artists
                .iter()
                .map(|artist| crate::library::catalog::ArtistCredit {
                    rating_key: Some(self.key(&artist.id)),
                    name: clean(artist.name.clone()),
                })
                .collect(),
            play_count: a.play_count,
            release_date: a.release_date.as_ref().and_then(n::ItemDate::display),
            record_labels: a
                .record_labels
                .into_iter()
                .map(|label| clean(label.name))
                .collect(),
            country: a.country.map(clean),
            version: a.version.map(clean),
            subtype: (a.is_compilation
                || a.release_types
                    .iter()
                    .any(|kind| kind.eq_ignore_ascii_case("compilation")))
            .then(|| "compilation".to_owned()),
            mood: a
                .moods
                .into_iter()
                .map(|tag| GenreTag {
                    tag: clean(tag),
                    ..Default::default()
                })
                .collect(),
            release_types: a.release_types.into_iter().map(clean).collect(),
            originally_available_at: a
                .original_release_date
                .as_ref()
                .or(a.release_date.as_ref())
                .and_then(n::ItemDate::display),
            rating_key: self.key(&a.id),
            key: self.key(&a.id),
            title: clean(a.name),
            parent_title: a.artist.map(clean),
            parent_rating_key: a.artist_id.map(|s| self.key(&s)),
            thumb: a.cover_art.map(|s| self.key(&s)),
            year: a.year,
            leaf_count: a.song_count,
            duration: a.duration.map(|s| s.saturating_mul(1000)),
            genre: genres,
            ..Default::default()
        }
    }
    pub fn track(&self, t: n::Song) -> Track {
        let artist_credits = t
            .extra_tags
            .get("artists")
            .and_then(serde_json::Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|artist| {
                let name = clean(artist.get("name")?.as_str()?.to_owned());
                if name.trim().is_empty() {
                    return None;
                }
                Some(crate::library::catalog::ArtistCredit {
                    rating_key: artist
                        .get("id")
                        .and_then(serde_json::Value::as_str)
                        .filter(|id| !id.is_empty())
                        .map(|id| self.key(id)),
                    name,
                })
            })
            .collect();
        let album_artist = t.album_artist.clone().or_else(|| {
            t.extra_tags
                .get("albumArtists")
                .and_then(display_tag)
                .filter(|value| !value.is_empty())
        });
        let mut seen = std::collections::HashSet::new();
        let genres = t
            .genres
            .iter()
            .map(|g| g.name.clone())
            .chain(t.genre.clone())
            .map(|name| clean(name).trim().to_owned())
            .filter(|name| !name.is_empty() && seen.insert(name.clone()))
            .collect();
        let mut metadata_tags = std::collections::BTreeMap::new();
        for (key, value) in &t.extra_tags {
            // Transport flags and alternate stream encodings are not music tags.
            if matches!(
                key.as_str(),
                "isVideo"
                    | "type"
                    | "contentType"
                    | "transcodedContentType"
                    | "transcodedSuffix"
                    | "isDir"
            ) {
                continue;
            }
            if let Some(value) = display_tag(value) {
                metadata_tags.insert(clean(key.clone()), value);
            }
        }
        let favorited = Some(t.starred.is_some());
        if let Some(value) = t.starred {
            metadata_tags.insert("Favorited".into(), clean(value));
        }
        if let Some(value) = t.user_rating {
            metadata_tags.insert("Rating".into(), format!("{value}/5"));
        }
        Track {
            artist_credits,
            favorited,
            metadata_tags,
            view_count: t.play_count,
            parent_index: t.disc_number,
            origin: TrackOrigin::Navidrome {
                source_id: self.source.id.clone(),
                song_id: t.id.clone(),
            },
            rating_key: self.key(&t.id),
            key: self.key(&t.id),
            title: clean(t.title),
            genres,
            parent_title: t.album.map(clean),
            parent_rating_key: t.album_id.map(|s| self.key(&s)),
            grandparent_title: album_artist.or_else(|| t.artist.clone()).map(clean),
            grandparent_rating_key: t.artist_id.map(|s| self.key(&s)),
            original_title: t.artist.map(clean),
            thumb: t.cover_art.map(|s| self.key(&s)),
            index: t.track,
            year: t.year,
            duration: t.duration.map(|s| s.saturating_mul(1000)),
            media: vec![Media {
                bit_depth: t.bit_depth,
                sampling_rate: t.sampling_rate,
                audio_channels: t.channel_count,
                container: t.suffix.clone(),
                audio_codec: t.suffix,
                bitrate: t.bit_rate,
                part: vec![MediaPart {
                    file: t.path,
                    size: t.size,
                    ..Default::default()
                }],
                ..Default::default()
            }],
            ..Default::default()
        }
    }
    pub fn playlist(&self, p: n::Playlist) -> Playlist {
        Playlist {
            rating_key: self.key(&p.id),
            key: self.key(&p.id),
            title: clean(p.name),
            playlist_type: "audio".into(),
            composite: p.cover_art.map(|s| self.key(&s)),
            duration: p.duration.map(|s| s.saturating_mul(1000)),
            leaf_count: p.song_count,
            added_at: None,
            updated_at: None,
            smart: false,
        }
    }
}

// Preserve additional OpenSubsonic music tags without displaying raw JSON or
// leaking stream URLs into the details pane. Arrays of credits use their names.
fn display_tag(value: &serde_json::Value) -> Option<String> {
    let text = match value {
        serde_json::Value::String(value) => clean(value.clone()),
        serde_json::Value::Number(value) => value.to_string(),
        serde_json::Value::Bool(value) => value.to_string(),
        serde_json::Value::Array(values) => values
            .iter()
            .filter_map(display_tag)
            .collect::<Vec<_>>()
            .join(", "),
        serde_json::Value::Object(values) => {
            if let Some(name) = values.get("name") {
                return display_tag(name);
            }
            if let Some(date) = values.get("year").and_then(serde_json::Value::as_u64) {
                let mut date = format!("{date:04}");
                if let Some(month) = values.get("month").and_then(serde_json::Value::as_u64) {
                    date.push_str(&format!("-{month:02}"));
                    if let Some(day) = values.get("day").and_then(serde_json::Value::as_u64) {
                        date.push_str(&format!("-{day:02}"));
                    }
                }
                date
            } else {
                values
                    .iter()
                    .filter_map(|(key, value)| {
                        display_tag(value).map(|value| format!("{}: {value}", clean(key.clone())))
                    })
                    .collect::<Vec<_>>()
                    .join(", ")
            }
        }
        serde_json::Value::Null => return None,
    };
    (!text.trim().is_empty()).then_some(text)
}
