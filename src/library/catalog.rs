//! Shared catalog metadata. Wire field names preserve existing cache/config compatibility;
//! transports convert into these models and keep response envelopes in their own modules.
use crate::util::serde_helpers::from_str_or_num_opt;
use serde::{Deserialize, Serialize};

/// Artist metadata.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Artist {
    #[serde(default)]
    pub rating_key: String,
    #[serde(default)]
    pub key: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub thumb: Option<String>,
    #[serde(default)]
    pub art: Option<String>,
    #[serde(default, rename = "Genre")]
    pub genre: Vec<GenreTag>,
    #[serde(default)]
    pub country: Option<String>,
    #[serde(default)]
    pub added_at: Option<i64>,
    /// Provider biography/summary.
    #[serde(default)]
    pub summary: Option<String>,
    /// "Similar" metadata tags from server (artist detail only).
    #[serde(default, rename = "Similar")]
    pub similar: Vec<GenreTag>,
}

/// Discography sections derived from supplied release types, never guessed titles.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum AlbumReleaseGroup {
    Albums,
    Eps,
    Singles,
    Compilations,
    Live,
    Soundtracks,
    Remixes,
    DjMixes,
    Other,
    AppearsIn,
}
impl AlbumReleaseGroup {
    pub fn label(self) -> &'static str {
        match self {
            Self::Albums => "Albums",
            Self::Eps => "EPs",
            Self::Singles => "Singles",
            Self::Compilations => "Compilations",
            Self::Live => "Live",
            Self::Soundtracks => "Soundtracks",
            Self::Remixes => "Remixes",
            Self::DjMixes => "DJ mixes",
            Self::Other => "Other releases",
            Self::AppearsIn => "Appears in",
        }
    }
}

/// Individual performer identity, independent of a joined display-artist string.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArtistCredit {
    #[serde(default)]
    pub rating_key: Option<String>,
    #[serde(default)]
    pub name: String,
}
impl ArtistCredit {
    pub fn matches(&self, artist: &Artist) -> bool {
        self.rating_key.as_deref() == Some(&artist.rating_key)
            || ((self.rating_key.is_none() || artist.rating_key.starts_with("track_artist:"))
                && self.name.eq_ignore_ascii_case(&artist.title))
    }
}

/// Album metadata.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Album {
    #[serde(default)]
    pub artist_credits: Vec<ArtistCredit>,
    #[serde(default)]
    pub play_count: Option<u64>,
    #[serde(default)]
    pub release_date: Option<String>,
    #[serde(default)]
    pub record_labels: Vec<String>,
    #[serde(default)]
    pub country: Option<String>,
    #[serde(default)]
    pub release_types: Vec<String>,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default, rename = "Mood")]
    pub mood: Vec<GenreTag>,
    #[serde(default)]
    pub originally_available_at: Option<String>,
    #[serde(default)]
    pub rating_key: String,
    #[serde(default)]
    pub key: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub parent_title: Option<String>,
    #[serde(default)]
    pub parent_rating_key: Option<String>,
    #[serde(default, deserialize_with = "from_str_or_num_opt")]
    pub year: Option<u16>,
    #[serde(default)]
    pub thumb: Option<String>,
    #[serde(default, rename = "Genre")]
    pub genre: Vec<GenreTag>,
    #[serde(default, deserialize_with = "from_str_or_num_opt")]
    pub leaf_count: Option<u32>,
    #[serde(default, deserialize_with = "from_str_or_num_opt")]
    pub duration: Option<u64>,
    #[serde(default, deserialize_with = "from_str_or_num_opt")]
    pub added_at: Option<i64>,
    #[serde(default, deserialize_with = "from_str_or_num_opt")]
    pub loudness_analysis_version: Option<u32>,
    #[serde(default, deserialize_with = "from_str_or_num_opt")]
    pub last_viewed_at: Option<i64>,
    /// Provider album subtype (e.g., "compilation").
    #[serde(default)]
    pub subtype: Option<String>,
}

impl Album {
    /// Discography chronology: original release, release date, then tagged year.
    /// Missing dates sort last; partial dates precede later dates in that year.
    pub fn release_sort_date(&self) -> Option<(u16, u8, u8)> {
        for value in [
            self.originally_available_at.as_deref(),
            self.release_date.as_deref(),
        ]
        .into_iter()
        .flatten()
        {
            let mut parts = value.split('-');
            let Some(year) = parts
                .next()
                .and_then(|part| part.parse::<u16>().ok())
                .filter(|year| *year > 0)
            else {
                continue;
            };
            let month = parts
                .next()
                .and_then(|part| part.parse::<u8>().ok())
                .filter(|month| (1..=12).contains(month))
                .unwrap_or(0);
            let day = parts
                .next()
                .and_then(|part| part.parse::<u8>().ok())
                .filter(|day| (1..=31).contains(day))
                .unwrap_or(0);
            return Some((year, month, day));
        }
        self.year.filter(|year| *year > 0).map(|year| (year, 0, 0))
    }

    /// Secondary types take precedence over the primary album/single/EP type.
    /// Untagged releases stay in Albums for compatibility with older catalogs.
    pub fn release_group(&self) -> AlbumReleaseGroup {
        use AlbumReleaseGroup::*;
        let types: Vec<_> = self
            .release_types
            .iter()
            .flat_map(|value| value.split([',', ';', '/']))
            .chain(self.subtype.iter().map(String::as_str))
            .map(|value| value.trim().to_ascii_lowercase())
            .filter(|value| !value.is_empty())
            .collect();
        let has = |kind: &str| types.iter().any(|value| value == kind);
        if has("live") {
            Live
        } else if has("soundtrack") {
            Soundtracks
        } else if has("remix") {
            Remixes
        } else if has("dj-mix") || has("dj mix") || has("djmix") {
            DjMixes
        } else if has("compilation") || self.is_compilation_candidate() {
            Compilations
        } else if has("ep") {
            Eps
        } else if has("single") {
            Singles
        } else if types.is_empty() || has("album") {
            Albums
        } else {
            Other
        }
    }

    /// Get track count (leaf_count in server API).
    pub fn track_count(&self) -> u32 {
        self.leaf_count.unwrap_or(0)
    }

    /// Whether this album is a candidate for being a compilation.
    /// Returns true if the subtype is "compilation" or the artist name
    /// matches common compilation artist names.
    pub fn is_compilation_candidate(&self) -> bool {
        if self
            .subtype
            .as_deref()
            .is_some_and(|kind| kind.eq_ignore_ascii_case("compilation"))
        {
            return true;
        }
        let name = self.artist_name().trim().to_lowercase();
        name == "various artists" || name == "various"
    }

    /// Get artist name (handles None and empty string).
    pub fn artist_name(&self) -> &str {
        match self.parent_title.as_deref() {
            Some(s) if !s.is_empty() => s,
            _ => "Unknown Artist",
        }
    }
}

/// Genre tag.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GenreTag {
    #[serde(default)]
    pub id: Option<u64>,
    #[serde(default)]
    pub tag: String,
}

/// Playlist metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Playlist {
    pub rating_key: String,
    pub key: String,
    pub title: String,
    #[serde(rename = "type")]
    pub playlist_type: String,
    #[serde(default)]
    pub composite: Option<String>,
    #[serde(default)]
    pub duration: Option<u64>,
    #[serde(default)]
    pub leaf_count: Option<u32>,
    #[serde(default)]
    pub added_at: Option<i64>,
    #[serde(default)]
    pub updated_at: Option<i64>,
    #[serde(default)]
    pub smart: bool,
}

impl Playlist {
    /// Get track count.
    pub fn track_count(&self) -> u32 {
        self.leaf_count.unwrap_or(0)
    }

    /// Check if this is an audio playlist.
    pub fn is_audio(&self) -> bool {
        self.playlist_type == "audio"
    }
}
