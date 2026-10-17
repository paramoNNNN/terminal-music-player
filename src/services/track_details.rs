//! Display source metadata without fetching audio or probing a remote file.
use crate::library::models::Track;

pub fn describe(track: &Track) -> String {
    let mut rows = Vec::new();
    let mut row = |label: &str, value: String| {
        rows.push(format!(
            "{label}: {}",
            crate::util::sanitize_display_text(&value)
        ));
    };
    row("Title", track.title.clone());
    row("Artist", track.track_artist().into());
    row("Album", track.album_name().into());
    row("Album artist", track.artist_name().into());
    if !track.artist_credits.is_empty() {
        row(
            "Credits",
            track
                .artist_credits
                .iter()
                .map(|a| a.name.as_str())
                .collect::<Vec<_>>()
                .join(", "),
        );
    }
    row(
        "Track",
        track.index.map_or("Unavailable".into(), |n| n.to_string()),
    );
    row(
        "Disc",
        track
            .parent_index
            .map_or("Unavailable".into(), |n| n.to_string()),
    );
    row(
        "Year",
        track.year.map_or("Unavailable".into(), |n| n.to_string()),
    );
    row(
        "Duration",
        track.duration.map_or("Unavailable".into(), |ms| {
            format!("{}:{:02}.{:03}", ms / 60000, ms / 1000 % 60, ms % 1000)
        }),
    );
    row(
        "Genres",
        if track.genres.is_empty() {
            "Unavailable".into()
        } else {
            track.genres.join(", ")
        },
    );
    row(
        "Personal plays",
        track
            .view_count
            .map_or("Unavailable".into(), |n| n.to_string()),
    );
    rows.push("\nAUDIO / FILE · original source".into());
    if track.media.is_empty() {
        rows.push("Audio and file metadata unavailable from this source.".into());
    }
    for media in &track.media {
        rows.push(format!(
            "Codec: {}",
            media.audio_codec.as_deref().unwrap_or("Unavailable")
        ));
        rows.push(format!(
            "Container: {}",
            media
                .container
                .as_deref()
                .or_else(|| media.part.first().and_then(|p| p.container.as_deref()))
                .unwrap_or("Unavailable")
        ));
        for (label, value, unit) in [
            ("Bitrate", media.bitrate, "kbps"),
            ("Sample rate", media.sampling_rate, "Hz"),
            ("Bit depth", media.bit_depth, "bit"),
            ("Channels", media.audio_channels.map(u32::from), ""),
        ] {
            rows.push(format!(
                "{label}: {}",
                value.map_or("Unavailable".to_string(), |n| format!("{n} {unit}")
                    .trim()
                    .to_owned())
            ));
        }
        for part in &media.part {
            rows.push(format!(
                "Size: {}",
                part.size.map_or("Unavailable".into(), |size| format!(
                    "{:.2} MiB ({size} bytes)",
                    size as f64 / 1048576.0
                ))
            ));
            // Source paths are metadata, never authenticated stream URLs.
            if let Some(path) = part.file.as_deref().filter(|p| !p.contains("://")) {
                rows.push(format!(
                    "Path: {}",
                    crate::util::sanitize_display_text(path)
                ));
            }
        }
    }
    rows.push("\nTAGS · supplied by the source".into());
    if track.metadata_tags.is_empty() {
        rows.push("No additional tags supplied.".into());
    }
    for (key, value) in &track.metadata_tags {
        rows.push(format!(
            "{}: {}",
            crate::util::sanitize_display_text(key),
            crate::util::sanitize_display_text(value)
        ));
    }
    crate::util::sanitize_display_text(&rows.join("\n")).into_owned()
}
