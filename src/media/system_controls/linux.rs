//! MPRIS on the session bus. D-Bus callbacks only enqueue commands; the app
//! thread owns playback. No server URLs or credentials are exported.
use super::Command;
use crate::app::{state::PlayStatus, AppState};
use std::{
    collections::{HashMap, VecDeque},
    io::Write,
    path::Path,
    sync::{Arc, Mutex, OnceLock},
};
use tokio::sync::watch;
use zbus::zvariant::{ObjectPath, OwnedValue, Value};

const PATH: &str = "/org/mpris/MediaPlayer2";
const PLAYER: &str = "org.mpris.MediaPlayer2.Player";

#[derive(Clone, Debug, PartialEq)]
struct Snapshot {
    id: String,
    title: String,
    artist: String,
    album: String,
    artwork: Option<Arc<[u8]>>,
    status: &'static str,
    position: i64,
    duration: i64,
    volume: f64,
    has_track: bool,
    next: bool,
    previous: bool,
}
impl Default for Snapshot {
    fn default() -> Self {
        Self {
            id: "/org/mpris/MediaPlayer2/TrackList/NoTrack".into(),
            title: String::new(),
            artist: String::new(),
            album: String::new(),
            artwork: None,
            status: "Stopped",
            position: 0,
            duration: 0,
            volume: 0.8,
            has_track: false,
            next: false,
            previous: false,
        }
    }
}
impl Snapshot {
    fn metadata(&self, art_url: Option<&str>) -> HashMap<&'static str, OwnedValue> {
        if !self.has_track {
            return HashMap::new();
        }
        let mut metadata = HashMap::from([
            (
                "mpris:trackid",
                OwnedValue::from(
                    ObjectPath::try_from(self.id.clone()).expect("generated track path"),
                ),
            ),
            ("mpris:length", OwnedValue::from(self.duration)),
            (
                "xesam:title",
                OwnedValue::from(zbus::zvariant::Str::from(self.title.clone())),
            ),
            (
                "xesam:album",
                OwnedValue::from(zbus::zvariant::Str::from(self.album.clone())),
            ),
            (
                "xesam:artist",
                OwnedValue::try_from(Value::from(vec![self.artist.clone()])).expect("string array"),
            ),
        ]);
        if let Some(url) = art_url {
            metadata.insert(
                "mpris:artUrl",
                zbus::zvariant::Str::from(url.to_owned()).into(),
            );
        }
        metadata
    }
}

/// A desktop-readable image owned by this session. Writes happen only on the
/// MPRIS worker, once per changed cover; unique URLs avoid desktop image caches
/// showing the previous cover. Dropping the publication removes its file.
#[derive(Default)]
struct PublishedArtwork {
    bytes: Option<Arc<[u8]>>,
    _file: Option<tempfile::NamedTempFile>,
    url: Option<String>,
}
impl PublishedArtwork {
    fn update(&mut self, bytes: Option<&Arc<[u8]>>, directory: &Path) -> std::io::Result<()> {
        if self.bytes.as_ref() == bytes {
            return Ok(());
        }
        *self = Self::default();
        let Some(bytes) = bytes else { return Ok(()) };
        std::fs::create_dir_all(directory)?;
        let format = image::guess_format(bytes).ok();
        let extension = format
            .as_ref()
            .and_then(|format| format.extensions_str().first().copied())
            .unwrap_or("img");
        let mut file = tempfile::Builder::new()
            .prefix("cover-")
            .suffix(&format!(".{extension}"))
            .tempfile_in(directory)?;
        file.write_all(bytes)?;
        let url = reqwest::Url::from_file_path(std::path::absolute(file.path())?)
            .map_err(|()| std::io::Error::other("MPRIS artwork path is not absolute"))?;
        self.url = Some(url.into());
        self._file = Some(file);
        self.bytes = Some(bytes.clone());
        Ok(())
    }
    fn url_for(&self, bytes: Option<&Arc<[u8]>>) -> Option<&str> {
        (self.bytes.as_ref() == bytes)
            .then_some(self.url.as_deref())
            .flatten()
    }
}
#[derive(Default)]
struct Bridge {
    snapshot: Mutex<Snapshot>,
    artwork: Mutex<PublishedArtwork>,
    commands: Mutex<VecDeque<(String, Command)>>,
}
static BRIDGE: OnceLock<Arc<Bridge>> = OnceLock::new();
impl Bridge {
    fn metadata(&self, snapshot: &Snapshot) -> HashMap<&'static str, OwnedValue> {
        let artwork = self.artwork.lock().unwrap_or_else(|e| e.into_inner());
        snapshot.metadata(artwork.url_for(snapshot.artwork.as_ref()))
    }
    fn snapshot(&self) -> Snapshot {
        self.snapshot
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }
    fn command(&self, command: Command) {
        let id = self.snapshot().id;
        self.command_for(id, command);
    }
    fn command_for(&self, id: String, command: Command) {
        let mut commands = self.commands.lock().unwrap_or_else(|e| e.into_inner());
        if commands.len() < 64 {
            commands.push_back((id, command));
        }
    }
}

pub fn sync(state: &AppState) -> Vec<Command> {
    BRIDGE
        .get()
        .map(|bridge| sync_bridge(bridge, state))
        .unwrap_or_default()
}
fn sync_bridge(bridge: &Bridge, state: &AppState) -> Vec<Command> {
    let track = state.current_track();
    let previous = bridge.snapshot();
    let index = if state.playback_mode == crate::app::state::PlaybackMode::Radio {
        state.radio.track_index
    } else {
        state.queue.index
    };
    let snapshot = Snapshot {
        id: track.map_or_else(
            || Snapshot::default().id,
            |t| {
                format!(
                    "/org/mpris/MediaPlayer2/track/t{:x}_{}",
                    md5::compute(t.rating_key.as_bytes()),
                    state.playback.request_id
                )
            },
        ),
        title: track.map(|t| t.title.clone()).unwrap_or_default(),
        artist: track.map(|t| t.track_artist().into()).unwrap_or_default(),
        album: track.map(|t| t.album_name().into()).unwrap_or_default(),
        artwork: track.and_then(|track| {
            let current = state.artwork.current_thumb.as_deref()?;
            if current != track.rating_key && Some(current) != track.best_thumb() {
                return None;
            }
            let data = state.artwork.current_data.as_deref()?;
            Some(
                previous
                    .artwork
                    .as_ref()
                    .filter(|bytes| bytes.as_ref() == data)
                    .cloned()
                    .unwrap_or_else(|| Arc::from(data)),
            )
        }),
        status: match state.playback.status {
            PlayStatus::Playing | PlayStatus::Buffering => "Playing",
            PlayStatus::Paused => "Paused",
            PlayStatus::Stopped => "Stopped",
        },
        position: micros(state.playback.position_ms),
        duration: micros(
            state
                .playback
                .duration_ms
                .max(track.map_or(0, |t| t.duration_ms())),
        ),
        volume: if state.playback.muted {
            0.0
        } else {
            state.playback.volume as f64
        },
        has_track: track.is_some(),
        next: index.is_some_and(|i| i + 1 < state.playback_tracks().len())
            || (state.playback_mode == crate::app::state::PlaybackMode::Radio
                && (state.radio.active_station.is_some() || state.radio.seed.is_some())),
        previous: index.is_some_and(|i| i > 0),
    };
    let mut commands = bridge.commands.lock().unwrap_or_else(|e| e.into_inner());
    let result = commands
        .drain(..)
        .filter(|(id, _)| *id == snapshot.id)
        .map(|(_, command)| command)
        .collect();
    *bridge.snapshot.lock().unwrap_or_else(|e| e.into_inner()) = snapshot;
    result
}
fn micros(ms: u64) -> i64 {
    ms.saturating_mul(1000).min(i64::MAX as u64) as i64
}

struct Root;
#[zbus::interface(name = "org.mpris.MediaPlayer2")]
impl Root {
    fn raise(&self) {}
    fn quit(&self) {}
    #[zbus(property)]
    fn can_quit(&self) -> bool {
        false
    }
    #[zbus(property)]
    fn can_raise(&self) -> bool {
        false
    }
    #[zbus(property)]
    fn has_track_list(&self) -> bool {
        false
    }
    #[zbus(property)]
    fn identity(&self) -> &str {
        "Textamp"
    }
    #[zbus(property)]
    fn supported_uri_schemes(&self) -> Vec<&str> {
        vec![]
    }
    #[zbus(property)]
    fn supported_mime_types(&self) -> Vec<&str> {
        vec![]
    }
}
struct Player(Arc<Bridge>);
#[zbus::interface(name = "org.mpris.MediaPlayer2.Player")]
impl Player {
    #[zbus(signal)]
    async fn seeked(
        emitter: &zbus::object_server::SignalEmitter<'_>,
        position: i64,
    ) -> zbus::Result<()>;
    fn play(&self) {
        self.0.command(Command::Play);
    }
    fn pause(&self) {
        self.0.command(Command::Pause);
    }
    fn play_pause(&self) {
        self.0.command(Command::Toggle);
    }
    fn stop(&self) {
        self.0.command(Command::Stop);
    }
    fn next(&self) {
        self.0.command(Command::Next);
    }
    fn previous(&self) {
        self.0.command(Command::Previous);
    }
    fn seek(&self, offset: i64) {
        let snapshot = self.0.snapshot();
        if snapshot.has_track {
            self.0.command_for(
                snapshot.id,
                Command::Seek(
                    snapshot
                        .position
                        .saturating_add(offset)
                        .clamp(0, snapshot.duration) as u64
                        / 1000,
                ),
            );
        }
    }
    fn set_position(&self, track_id: ObjectPath<'_>, position: i64) {
        let snapshot = self.0.snapshot();
        if snapshot.has_track
            && track_id.as_str() == snapshot.id
            && (0..=snapshot.duration).contains(&position)
        {
            self.0
                .command_for(snapshot.id, Command::Seek(position as u64 / 1000));
        }
    }
    fn open_uri(&self, _uri: &str) -> zbus::fdo::Result<()> {
        Err(zbus::fdo::Error::NotSupported(
            "Choose a library in Textamp".into(),
        ))
    }
    #[zbus(property)]
    fn playback_status(&self) -> &str {
        self.0.snapshot().status
    }
    #[zbus(property)]
    fn rate(&self) -> f64 {
        1.0
    }
    #[zbus(property)]
    fn set_rate(&self, rate: f64) -> zbus::fdo::Result<()> {
        if rate == 1.0 {
            Ok(())
        } else {
            Err(zbus::fdo::Error::NotSupported(
                "Playback rate is fixed".into(),
            ))
        }
    }
    #[zbus(property)]
    fn minimum_rate(&self) -> f64 {
        1.0
    }
    #[zbus(property)]
    fn maximum_rate(&self) -> f64 {
        1.0
    }
    #[zbus(property)]
    fn metadata(&self) -> HashMap<&'static str, OwnedValue> {
        self.0.metadata(&self.0.snapshot())
    }
    #[zbus(property)]
    fn volume(&self) -> f64 {
        self.0.snapshot().volume
    }
    #[zbus(property)]
    fn set_volume(&self, volume: f64) {
        if volume.is_finite() {
            self.0
                .command(Command::Volume(volume.clamp(0.0, 1.0) as f32));
        }
    }
    #[zbus(property(emits_changed_signal = "false"))]
    fn position(&self) -> i64 {
        self.0.snapshot().position
    }
    #[zbus(property)]
    fn can_go_next(&self) -> bool {
        self.0.snapshot().next
    }
    #[zbus(property)]
    fn can_go_previous(&self) -> bool {
        self.0.snapshot().previous
    }
    #[zbus(property)]
    fn can_play(&self) -> bool {
        self.0.snapshot().has_track
    }
    #[zbus(property)]
    fn can_pause(&self) -> bool {
        self.0.snapshot().has_track
    }
    #[zbus(property)]
    fn can_seek(&self) -> bool {
        self.0.snapshot().duration > 0
    }
    #[zbus(property)]
    fn can_control(&self) -> bool {
        true
    }
}

async fn serve(
    bridge: Arc<Bridge>,
    mut shutdown: watch::Receiver<bool>,
    builder: zbus::connection::Builder<'_>,
) -> zbus::Result<()> {
    let connection = builder
        .name("org.mpris.MediaPlayer2.textamp")?
        .serve_at(PATH, Root)?
        .serve_at(PATH, Player(bridge.clone()))?
        .build()
        .await?;
    let mut previous = Snapshot::default();
    let artwork_directory = crate::config::XdgPaths::new("textamp")
        .cache_dir
        .join("mpris");
    let mut timer = tokio::time::interval(std::time::Duration::from_millis(100));
    loop {
        tokio::select! {
            _ = shutdown.changed() => break,
            _ = timer.tick() => {
                let current = bridge.snapshot();
                if current.artwork != previous.artwork {
                    let mut artwork = bridge.artwork.lock().unwrap_or_else(|e| e.into_inner());
                    if let Err(error) = artwork.update(current.artwork.as_ref(), &artwork_directory) {
                        tracing::warn!("Cannot publish MPRIS artwork: {error}");
                    }
                }
                let mut changed: HashMap<&str, OwnedValue> = HashMap::new();
                if current.status != previous.status { changed.insert("PlaybackStatus", Value::from(current.status).try_into()?); }
                if current.id != previous.id || current.title != previous.title || current.artist != previous.artist || current.album != previous.album || current.duration != previous.duration || current.artwork != previous.artwork {
                    changed.insert("Metadata", Value::from(bridge.metadata(&current)).try_into()?);
                }
                if current.volume != previous.volume { changed.insert("Volume", current.volume.into()); }
                for (name, new, old) in [
                    ("CanGoNext", current.next, previous.next), ("CanGoPrevious", current.previous, previous.previous),
                    ("CanPlay", current.has_track, previous.has_track), ("CanPause", current.has_track, previous.has_track),
                    ("CanSeek", current.duration > 0, previous.duration > 0),
                ] { if new != old { changed.insert(name, new.into()); } }
                if !changed.is_empty() {
                    connection.emit_signal(None::<&str>, PATH, "org.freedesktop.DBus.Properties", "PropertiesChanged", &(PLAYER, changed, Vec::<&str>::new())).await?;
                }
                if current.id == previous.id && (current.position - previous.position).abs() > 1_500_000 {
                    Player::seeked(&zbus::object_server::SignalEmitter::new(&connection, PATH)?, current.position).await?;
                }
                previous = current;
            }
        }
    }
    Ok(())
}

pub fn run(work: impl FnOnce() -> anyhow::Result<()> + Send + 'static) -> anyhow::Result<()> {
    let bridge = BRIDGE.get_or_init(|| Arc::new(Bridge::default())).clone();
    let (shutdown, rx) = watch::channel(false);
    let worker = std::thread::Builder::new().name("textamp-mpris".into()).spawn(move || {
        let result = tokio::runtime::Builder::new_current_thread().enable_all().build().map_err(anyhow::Error::from).and_then(|runtime| runtime.block_on(async {
                let server_rx = rx.clone();
                tokio::select! {
                    result = async { serve(bridge, server_rx, zbus::connection::Builder::session()?).await } => result.map_err(anyhow::Error::from),
                _ = async { let mut rx = rx; let _ = rx.changed().await; } => Ok(()),
            }
        }));
        if let Err(error) = result { tracing::warn!("Linux media controls unavailable: {error}"); }
    })?;
    let result = work();
    let _ = shutdown.send(true);
    let _ = worker.join();
    if let Some(bridge) = BRIDGE.get() {
        *bridge.artwork.lock().unwrap_or_else(|e| e.into_inner()) = PublishedArtwork::default();
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::StreamExt;
    use std::{
        io::{BufRead, BufReader},
        process::{Child, Stdio},
        time::Duration,
    };
    struct Bus(Child);
    impl Drop for Bus {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    #[test]
    fn published_cover_urls_escape_paths_reuse_files_and_clean_up() {
        let directory = tempfile::Builder::new()
            .prefix("textamp cover ü ")
            .tempdir()
            .unwrap();
        let mut png = std::io::Cursor::new(Vec::new());
        image::DynamicImage::new_rgb8(2, 2)
            .write_to(&mut png, image::ImageFormat::Png)
            .unwrap();
        let bytes: Arc<[u8]> = Arc::from(png.into_inner());
        let mut artwork = PublishedArtwork::default();
        artwork.update(Some(&bytes), directory.path()).unwrap();
        let url = artwork.url_for(Some(&bytes)).unwrap().to_owned();
        assert!(url.contains("%20"));
        let path = reqwest::Url::parse(&url).unwrap().to_file_path().unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), bytes.as_ref());
        artwork.update(Some(&bytes), directory.path()).unwrap();
        assert_eq!(artwork.url_for(Some(&bytes)), Some(url.as_str()));
        assert!(artwork.url_for(None).is_none());
        artwork.update(None, directory.path()).unwrap();
        assert!(!path.exists());
        artwork.update(Some(&bytes), directory.path()).unwrap();
        let path = reqwest::Url::parse(artwork.url_for(Some(&bytes)).unwrap())
            .unwrap()
            .to_file_path()
            .unwrap();
        drop(artwork);
        assert!(!path.exists());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn private_bus_reports_metadata_signals_and_routes_media_commands() {
        let child = std::process::Command::new("dbus-daemon")
            .args(["--session", "--nofork", "--print-address=1"])
            .stdout(Stdio::piped())
            .spawn()
            .expect("dbus-daemon is required for the Linux media fixture");
        let mut bus = Bus(child);
        let mut address = String::new();
        BufReader::new(bus.0.stdout.take().unwrap())
            .read_line(&mut address)
            .unwrap();
        let address = address.trim().to_owned();
        let bridge = Arc::new(Bridge::default());
        let (shutdown, rx) = watch::channel(false);
        let builder = zbus::connection::Builder::address(address.as_str()).unwrap();
        let server = tokio::spawn(serve(bridge.clone(), rx, builder));
        let connection = zbus::connection::Builder::address(address.as_str())
            .unwrap()
            .build()
            .await
            .unwrap();
        let player = zbus::proxy::Builder::<zbus::Proxy<'_>>::new(&connection)
            .destination("org.mpris.MediaPlayer2.textamp")
            .unwrap()
            .path(PATH)
            .unwrap()
            .interface(PLAYER)
            .unwrap()
            .cache_properties(zbus::proxy::CacheProperties::No)
            .build()
            .await
            .unwrap();
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                if player
                    .get_property::<String>("PlaybackStatus")
                    .await
                    .is_ok()
                {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        let properties = zbus::Proxy::new(
            &connection,
            "org.mpris.MediaPlayer2.textamp",
            PATH,
            "org.freedesktop.DBus.Properties",
        )
        .await
        .unwrap();
        let mut signals = properties
            .receive_signal("PropertiesChanged")
            .await
            .unwrap();
        let mut state = AppState::new();
        state.queue.tracks = vec![crate::library::models::Track {
            rating_key: "fixture".into(),
            title: "Fixture song".into(),
            original_title: Some("Fixture artist".into()),
            parent_title: Some("Fixture album".into()),
            duration: Some(60000),
            ..Default::default()
        }];
        state.queue.index = Some(0);
        state.playback.status = PlayStatus::Playing;
        state.playback.duration_ms = 60000;
        state.playback.position_ms = 5000;
        assert!(sync_bridge(&bridge, &state).is_empty());
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                let message = signals.next().await.unwrap();
                let (_, values, _): (String, HashMap<String, OwnedValue>, Vec<String>) =
                    message.body().deserialize().unwrap();
                if values.contains_key("Metadata") {
                    assert_eq!(
                        <&str>::try_from(values.get("PlaybackStatus").unwrap()).unwrap(),
                        "Playing"
                    );
                    break;
                }
            }
        })
        .await
        .unwrap();
        let metadata: HashMap<String, OwnedValue> = player.get_property("Metadata").await.unwrap();
        assert_eq!(
            <&str>::try_from(&metadata["xesam:title"]).unwrap(),
            "Fixture song"
        );
        assert_eq!(
            i64::try_from(&metadata["mpris:length"]).unwrap(),
            60_000_000
        );
        assert!(!metadata.contains_key("xesam:url"));
        assert!(!metadata.contains_key("mpris:artUrl"));
        // Artwork can arrive after playback starts. Verify both the D-Bus
        // notification and that its URL points to real, credential-free bytes.
        let mut png = std::io::Cursor::new(Vec::new());
        image::DynamicImage::new_rgb8(8, 8)
            .write_to(&mut png, image::ImageFormat::Png)
            .unwrap();
        state.artwork.current_thumb = Some("fixture".into());
        state.artwork.current_data = Some(png.into_inner());
        sync_bridge(&bridge, &state);
        let art_url = tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                let message = signals.next().await.unwrap();
                let (_, values, _): (String, HashMap<String, OwnedValue>, Vec<String>) =
                    message.body().deserialize().unwrap();
                if let Some(metadata) = values.get("Metadata") {
                    let metadata: HashMap<String, OwnedValue> =
                        metadata.try_clone().unwrap().try_into().unwrap();
                    if let Some(url) = metadata.get("mpris:artUrl") {
                        break <&str>::try_from(url).unwrap().to_owned();
                    }
                }
            }
        })
        .await
        .unwrap();
        let url = reqwest::Url::parse(&art_url).unwrap();
        assert_eq!(url.scheme(), "file");
        let cover_path = url.to_file_path().unwrap();
        assert_eq!(
            std::fs::read(&cover_path).unwrap(),
            state.artwork.current_data.as_ref().unwrap().as_slice()
        );
        let metadata: HashMap<String, OwnedValue> = player.get_property("Metadata").await.unwrap();
        assert_eq!(
            <&str>::try_from(&metadata["mpris:artUrl"]).unwrap(),
            art_url
        );
        sync_bridge(&bridge, &state);
        assert_eq!(
            bridge.metadata(&bridge.snapshot())["mpris:artUrl"],
            metadata["mpris:artUrl"]
        );
        // Browsing or stale art for another track must never publish that cover.
        state.artwork.current_thumb = Some("unrelated".into());
        sync_bridge(&bridge, &state);
        assert!(!player
            .get_property::<HashMap<String, OwnedValue>>("Metadata")
            .await
            .unwrap()
            .contains_key("mpris:artUrl"));
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                let message = signals.next().await.unwrap();
                let (_, values, _): (String, HashMap<String, OwnedValue>, Vec<String>) =
                    message.body().deserialize().unwrap();
                if let Some(metadata) = values.get("Metadata") {
                    let metadata: HashMap<String, OwnedValue> =
                        metadata.try_clone().unwrap().try_into().unwrap();
                    if !metadata.contains_key("mpris:artUrl") {
                        break;
                    }
                }
            }
        })
        .await
        .unwrap();
        assert!(
            !cover_path.exists(),
            "superseded cover file must be removed"
        );
        let track_id = ObjectPath::try_from(bridge.snapshot().id).unwrap();
        for method in ["Pause", "Play", "PlayPause", "Next", "Previous", "Stop"] {
            player.call::<_, _, ()>(method, &()).await.unwrap();
        }
        player
            .call::<_, _, ()>("Seek", &(2_000_000_i64,))
            .await
            .unwrap();
        player
            .call::<_, _, ()>("SetPosition", &(track_id.clone(), 12_000_000_i64))
            .await
            .unwrap();
        player
            .call::<_, _, ()>(
                "SetPosition",
                &(ObjectPath::try_from("/stale").unwrap(), 1_i64),
            )
            .await
            .unwrap();
        player.set_property("Volume", 0.4_f64).await.unwrap();
        let commands = sync_bridge(&bridge, &state);
        assert!(
            matches!(commands.as_slice(), [Command::Pause, Command::Play, Command::Toggle, Command::Next, Command::Previous, Command::Stop, Command::Seek(7000), Command::Seek(12000), Command::Volume(v)] if (*v - 0.4).abs() < 0.001)
        );
        bridge.command(Command::Next);
        let old_id = bridge.snapshot().id;
        state.playback.request_id += 1;
        assert!(
            sync_bridge(&bridge, &state).is_empty(),
            "discard commands for superseded playback"
        );
        bridge.command_for(old_id, Command::Seek(1000));
        assert!(
            sync_bridge(&bridge, &state).is_empty(),
            "discard a seek enqueued after its track was superseded"
        );
        state.playback.status = PlayStatus::Paused;
        sync_bridge(&bridge, &state);
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                let message = signals.next().await.unwrap();
                let (_, values, _): (String, HashMap<String, OwnedValue>, Vec<String>) =
                    message.body().deserialize().unwrap();
                if values
                    .get("PlaybackStatus")
                    .is_some_and(|v| <&str>::try_from(v).ok() == Some("Paused"))
                {
                    break;
                }
            }
        })
        .await
        .unwrap();
        // playerctl is optional outside the Nix check environment; exercise the
        // actual desktop client when installed, on this private bus only.
        if let Ok(output) = tokio::process::Command::new("playerctl")
            .args(["--player=textamp", "status"])
            .env("DBUS_SESSION_BUS_ADDRESS", &address)
            .output()
            .await
        {
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "Paused");
        }
        shutdown.send(true).unwrap();
        server.await.unwrap().unwrap();
    }
}
