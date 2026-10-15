//! AppKit and MediaPlayer are confined to the process main thread. The terminal
//! runtime runs on its own thread and exchanges owned snapshots and commands.
use super::Command;
use crate::app::action::PlaybackAction;
use crate::app::{state::PlayStatus, AppState};
use block2::RcBlock;
use objc2::rc::{autoreleasepool, Allocated, Retained};
use objc2::runtime::{AnyClass, AnyObject};
use objc2::{define_class, msg_send, sel, ClassType, MainThreadMarker};
use objc2_core_foundation::CGSize;
use objc2_foundation::{NSDate, NSNumber, NSObject, NSObjectProtocol, NSString};
use std::collections::VecDeque;
use std::ptr::NonNull;
use std::sync::Arc;
use std::sync::{Mutex, OnceLock};

#[link(name = "AppKit", kind = "framework")]
unsafe extern "C" {}
#[link(name = "MediaPlayer", kind = "framework")]
unsafe extern "C" {
    static MPMediaItemPropertyTitle: *const NSString;
    static MPMediaItemPropertyArtist: *const NSString;
    static MPMediaItemPropertyAlbumTitle: *const NSString;
    static MPMediaItemPropertyArtwork: *const NSString;
    static MPMediaItemPropertyPlaybackDuration: *const NSString;
    static MPNowPlayingInfoPropertyElapsedPlaybackTime: *const NSString;
    static MPNowPlayingInfoPropertyPlaybackRate: *const NSString;
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Snapshot {
    key: String,
    title: String,
    artist: String,
    album: String,
    position: u64,
    duration: u64,
    status: PlayStatus,
    artwork: Option<Arc<[u8]>>,
}
#[derive(Default)]
struct Bridge {
    snapshot: Mutex<Option<Snapshot>>,
    commands: Mutex<VecDeque<(String, Command)>>,
}
static BRIDGE: OnceLock<Bridge> = OnceLock::new();

fn command(command: Command) -> isize {
    let Some(bridge) = BRIDGE.get() else {
        return 100;
    };
    let Ok(snapshot) = bridge.snapshot.lock() else {
        return 100;
    };
    let Some(snapshot) = snapshot.as_ref() else {
        return 100;
    };
    let Ok(mut commands) = bridge.commands.lock() else {
        return 100;
    };
    if commands.len() >= 64 {
        return 100;
    }
    commands.push_back((snapshot.key.clone(), command));
    0 // MPRemoteCommandHandlerStatusSuccess
}

define_class!(
    // NSObject has no additional subclass requirements. This object is retained
    // on the main thread until its remote-command targets have been removed.
    #[unsafe(super = NSObject)]
    struct MediaTarget;
    unsafe impl NSObjectProtocol for MediaTarget {}
    impl MediaTarget {
        #[unsafe(method(play:))]
        fn play(&self, _event: &AnyObject) -> isize { command(Command::Play) }
        #[unsafe(method(pause:))]
        fn pause(&self, _event: &AnyObject) -> isize { command(Command::Pause) }
        #[unsafe(method(toggle:))]
        fn toggle(&self, _event: &AnyObject) -> isize { command(Command::Toggle) }
        #[unsafe(method(next:))]
        fn next(&self, _event: &AnyObject) -> isize { command(Command::Next) }
        #[unsafe(method(previous:))]
        fn previous(&self, _event: &AnyObject) -> isize { command(Command::Previous) }
        #[unsafe(method(stop:))]
        fn stop(&self, _event: &AnyObject) -> isize { command(Command::Stop) }
        #[unsafe(method(seek:))]
        fn seek(&self, event: &AnyObject) -> isize {
            // Apple's public MPChangePlaybackPositionCommandEvent property.
            let seconds: f64 = unsafe { msg_send![event, positionTime] };
            if !seconds.is_finite() || seconds < 0.0 { return 200; }
            command(Command::Seek((seconds * 1000.0) as u64))
        }
    }
);

/// Called by the application event loop, never from a native callback. No
/// credentials, HTTP calls, audio devices or UI objects cross this bridge.
pub fn sync(state: &AppState) -> Vec<Command> {
    let Some(bridge) = BRIDGE.get() else {
        return Vec::new();
    };
    // Reuse immutable image bytes across position updates; artwork arrival is
    // itself a snapshot change, even when the terminal is not focused.
    let previous = bridge
        .snapshot
        .lock()
        .ok()
        .and_then(|snapshot| snapshot.clone());
    let snapshot = state
        .current_track()
        .filter(|_| state.playback.status != PlayStatus::Stopped)
        .map(|track| Snapshot {
            key: track.rating_key.clone(),
            title: track.title.clone(),
            artist: track.track_artist().into(),
            album: track.album_name().into(),
            position: state.playback.position_ms,
            duration: state.playback.duration_ms.max(track.duration_ms()),
            status: state.playback.status,
            artwork: state.artwork.current_data.as_deref().and_then(|data| {
                let current = state.artwork.current_thumb.as_deref()?;
                if current != track.rating_key && Some(current) != track.best_thumb() {
                    return None;
                }
                Some(
                    previous
                        .as_ref()
                        .and_then(|snapshot| snapshot.artwork.as_ref())
                        .filter(|bytes| bytes.as_ref() == data)
                        .cloned()
                        .unwrap_or_else(|| Arc::from(data)),
                )
            }),
        });
    let key = snapshot.as_ref().map(|snapshot| snapshot.key.as_str());
    let actions = bridge
        .commands
        .lock()
        .map(|mut commands| {
            commands
                .drain(..)
                .filter(|(target, _)| Some(target.as_str()) == key)
                .map(|(_, command)| command)
                .collect()
        })
        .unwrap_or_default();
    if let Ok(mut latest) = bridge.snapshot.lock() {
        *latest = snapshot;
    }
    actions
}

/// Native objects stay on the AppKit thread. The copied block owns its image
/// for as long as MediaPlayer can request artwork; no filesystem or network I/O
/// is performed from that callback.
unsafe fn native_artwork(bytes: &[u8]) -> Option<Retained<AnyObject>> {
    unsafe {
        let data: Retained<AnyObject> = msg_send![objc2::class!(NSData),
            dataWithBytes: bytes.as_ptr().cast::<std::ffi::c_void>(), length: bytes.len()];
        let image_class = AnyClass::get(c"NSImage")?;
        let allocated: Allocated<AnyObject> = msg_send![image_class, alloc];
        let image: Option<Retained<AnyObject>> = msg_send![allocated, initWithData: &*data];
        let image = image?;
        let size: CGSize = msg_send![&*image, size];
        if size.width <= 0.0 || size.height <= 0.0 {
            return None;
        }
        let handler = RcBlock::new(move |_requested: CGSize| NonNull::from(&*image));
        let artwork_class = AnyClass::get(c"MPMediaItemArtwork")?;
        let allocated: Allocated<AnyObject> = msg_send![artwork_class, alloc];
        msg_send![allocated, initWithBoundsSize: size, requestHandler: &*handler]
    }
}

type CachedArtwork = Option<(Arc<[u8]>, Option<Retained<AnyObject>>)>;

unsafe fn publish(center: &AnyObject, snapshot: Option<&Snapshot>, cached: &mut CachedArtwork) {
    // SAFETY: selectors, argument types and ownership follow the public
    // MPNowPlayingInfoCenter / Foundation APIs; caller holds main-thread access
    // and an autorelease pool. NSString keys are Apple's documented constants.
    unsafe {
        let Some(snapshot) = snapshot else {
            *cached = None;
            let _: () = msg_send![center, setPlaybackState: 3_isize];
            let _: () = msg_send![center, setNowPlayingInfo: Option::<&AnyObject>::None];
            return;
        };
        let dictionary: Retained<AnyObject> = msg_send![objc2::class!(NSMutableDictionary), new];
        for (key, value) in [
            (&*MPMediaItemPropertyTitle, &snapshot.title),
            (&*MPMediaItemPropertyArtist, &snapshot.artist),
            (&*MPMediaItemPropertyAlbumTitle, &snapshot.album),
        ] {
            let value = NSString::from_str(value);
            let _: () = msg_send![&dictionary, setObject: &*value, forKey: key];
        }
        if let Some(bytes) = snapshot.artwork.as_ref() {
            if cached
                .as_ref()
                .is_none_or(|(previous, _)| previous != bytes)
            {
                *cached = Some((bytes.clone(), native_artwork(bytes)));
            }
            if let Some((_, Some(artwork))) = cached.as_ref() {
                let _: () = msg_send![&dictionary, setObject: &**artwork, forKey: &*MPMediaItemPropertyArtwork];
            }
        } else {
            *cached = None;
        }
        for (key, value) in [
            (
                &*MPMediaItemPropertyPlaybackDuration,
                snapshot.duration as f64 / 1000.0,
            ),
            (
                &*MPNowPlayingInfoPropertyElapsedPlaybackTime,
                snapshot.position as f64 / 1000.0,
            ),
            (
                &*MPNowPlayingInfoPropertyPlaybackRate,
                if snapshot.status == PlayStatus::Playing {
                    1.0
                } else {
                    0.0
                },
            ),
        ] {
            let value = NSNumber::new_f64(value);
            let _: () = msg_send![&dictionary, setObject: &*value, forKey: key];
        }
        let _: () = msg_send![center, setNowPlayingInfo: &*dictionary];
        let status: isize = if snapshot.status == PlayStatus::Playing {
            1
        } else {
            2
        };
        let _: () = msg_send![center, setPlaybackState: status];
    }
}

pub fn run(work: impl FnOnce() -> anyhow::Result<()> + Send + 'static) -> anyhow::Result<()> {
    run_inner(work, |_, _, _| Ok(()))
}

fn run_inner(
    work: impl FnOnce() -> anyhow::Result<()> + Send + 'static,
    mut observe: impl FnMut(&AnyObject, Option<&Snapshot>, &MediaTarget) -> anyhow::Result<()>,
) -> anyhow::Result<()> {
    let _main = MainThreadMarker::new()
        .ok_or_else(|| anyhow::anyhow!("macOS media controls require the process main thread"))?;
    BRIDGE.get_or_init(Bridge::default);
    autoreleasepool(|_| {
        // SAFETY: framework classes and selectors exist on supported macOS.
        // All AppKit messages and native objects remain on the main thread.
        unsafe {
            let application_class = AnyClass::get(c"NSApplication")
                .ok_or_else(|| anyhow::anyhow!("AppKit unavailable"))?;
            let app: Retained<AnyObject> = msg_send![application_class, sharedApplication];
            let _: bool = msg_send![&app, setActivationPolicy: 1_isize]; // Accessory: no Dock/window.
            let target: Retained<MediaTarget> = msg_send![MediaTarget::class(), new];
            let _: () = msg_send![&app, setDelegate: &*target];
            let _: () = msg_send![&app, finishLaunching];
            let center_class = AnyClass::get(c"MPNowPlayingInfoCenter")
                .ok_or_else(|| anyhow::anyhow!("MediaPlayer unavailable"))?;
            let center: Retained<AnyObject> = msg_send![center_class, defaultCenter];
            let remote_class = AnyClass::get(c"MPRemoteCommandCenter")
                .ok_or_else(|| anyhow::anyhow!("Remote media controls unavailable"))?;
            let remote: Retained<AnyObject> = msg_send![remote_class, sharedCommandCenter];
            let mut commands: Vec<Retained<AnyObject>> = Vec::new();
            for (get, action) in [
                (sel!(playCommand), sel!(play:)),
                (sel!(pauseCommand), sel!(pause:)),
                (sel!(togglePlayPauseCommand), sel!(toggle:)),
                (sel!(nextTrackCommand), sel!(next:)),
                (sel!(previousTrackCommand), sel!(previous:)),
                (sel!(stopCommand), sel!(stop:)),
                (sel!(changePlaybackPositionCommand), sel!(seek:)),
            ] {
                let command: Retained<AnyObject> = objc2::msg_send![&remote, performSelector: get];
                let _: () = msg_send![&command, addTarget: &*target, action: action];
                let _: () = msg_send![&command, setEnabled: true];
                commands.push(command);
            }
            let worker = std::thread::Builder::new()
                .name("textamp-terminal".into())
                .spawn(work)?;
            let mode = NSString::from_str("kCFRunLoopDefaultMode");
            let mut previous: Option<Snapshot> = None;
            let mut cached_artwork: CachedArtwork = None;
            while !worker.is_finished() {
                autoreleasepool(|_| -> anyhow::Result<()> {
                    let latest = BRIDGE
                        .get()
                        .and_then(|bridge| {
                            bridge.snapshot.lock().ok().map(|snapshot| snapshot.clone())
                        })
                        .flatten();
                    if latest != previous {
                        publish(&center, latest.as_ref(), &mut cached_artwork);
                        // Observers are used only by the opt-in native fixture.
                        if let Err(error) = observe(&center, latest.as_ref(), &target) {
                            tracing::error!("Native media fixture: {error:#}");
                            return Err(error);
                        }
                        previous = latest;
                    }
                    let until = NSDate::dateWithTimeIntervalSinceNow(0.05);
                    let event: Option<Retained<AnyObject>> = msg_send![&app, nextEventMatchingMask: usize::MAX, untilDate: &*until, inMode: &*mode, dequeue: true];
                    if let Some(event) = event {
                        let _: () = msg_send![&app, sendEvent: &*event];
                    }
                    Ok(())
                })?;
            }
            publish(&center, None, &mut cached_artwork);
            observe(&center, None, &target)?;
            for command in commands {
                let _: () = msg_send![&command, setEnabled: false];
                let _: () = msg_send![&command, removeTarget: &*target];
            }
            let _: () = msg_send![&app, setDelegate: Option::<&AnyObject>::None];
            worker
                .join()
                .map_err(|_| anyhow::anyhow!("Terminal runtime panicked"))?
        }
    })
}

/// Explicit native fixture: publishes synthetic metadata, sends a native Pause
/// command through the bridge, verifies paused progress/rate, then clears it.
/// Opens no audio device and accesses no saved account or personal server.
pub fn verify_native() -> anyhow::Result<()> {
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };
    use std::time::{Duration, Instant};
    let stage = Arc::new(AtomicUsize::new(0));
    let worker_stage = stage.clone();
    run_inner(
        move || {
            let mut state = AppState::new();
            state.queue.tracks = vec![crate::library::models::Track {
                rating_key: "native-fixture".into(),
                title: "Textamp native fixture".into(),
                original_title: Some("Fixture Artist".into()),
                parent_title: Some("Fixture Album".into()),
                duration: Some(120_000),
                ..Default::default()
            }];
            state.queue.index = Some(0);
            state.set_playback_mode(crate::app::state::PlaybackMode::Queue);
            state.playback.status = PlayStatus::Playing;
            state.playback.position_ms = 12_000;
            state.playback.duration_ms = 120_000;
            let mut png = std::io::Cursor::new(Vec::new());
            image::DynamicImage::new_rgb8(32, 32).write_to(&mut png, image::ImageFormat::Png)?;
            state.artwork.current_thumb = Some("native-fixture".into());
            state.artwork.current_data = Some(png.into_inner());
            let deadline = Instant::now() + Duration::from_secs(5);
            loop {
                let commands = sync(&state);
                if !commands.is_empty() {
                    anyhow::ensure!(
                        commands.len() == 1 && matches!(commands[0], Command::Pause),
                        "Native Pause callback mismatch"
                    );
                    anyhow::ensure!(
                        matches!(
                            super::playback_action(commands[0], &state),
                            Some(PlaybackAction::TogglePlayPause)
                        ),
                        "Native Pause did not route to playback"
                    );
                    state.playback.status = PlayStatus::Paused;
                    sync(&state);
                    break;
                }
                anyhow::ensure!(Instant::now() < deadline, "Native Pause callback timed out");
                std::thread::sleep(Duration::from_millis(10));
            }
            while worker_stage.load(Ordering::Acquire) < 2 {
                anyhow::ensure!(
                    Instant::now() < deadline,
                    "Native paused readback timed out"
                );
                std::thread::sleep(Duration::from_millis(10));
            }
            state.playback.status = PlayStatus::Stopped;
            sync(&state);
            Ok(())
        },
        move |center, snapshot, target| {
            // SAFETY: run_inner invokes the observer on the AppKit main thread.
            unsafe {
                let info: Option<Retained<AnyObject>> = msg_send![center, nowPlayingInfo];
                let Some(snapshot) = snapshot else {
                    anyhow::ensure!(info.is_none(), "Stopped metadata was not cleared");
                    return Ok(());
                };
                let info =
                    info.ok_or_else(|| anyhow::anyhow!("Native Now Playing metadata is missing"))?;
                for (key, expected) in [
                    (&*MPMediaItemPropertyTitle, "Textamp native fixture"),
                    (&*MPMediaItemPropertyArtist, "Fixture Artist"),
                    (&*MPMediaItemPropertyAlbumTitle, "Fixture Album"),
                ] {
                    let actual: Option<Retained<NSString>> = msg_send![&info, objectForKey: key];
                    anyhow::ensure!(
                        actual.is_some_and(|actual| actual.to_string() == expected),
                        "Native metadata mismatch"
                    );
                }
                for (key, expected) in [
                    (&*MPMediaItemPropertyPlaybackDuration, 120.0),
                    (&*MPNowPlayingInfoPropertyElapsedPlaybackTime, 12.0),
                    (
                        &*MPNowPlayingInfoPropertyPlaybackRate,
                        if snapshot.status == PlayStatus::Playing {
                            1.0
                        } else {
                            0.0
                        },
                    ),
                ] {
                    let actual: Option<Retained<NSNumber>> = msg_send![&info, objectForKey: key];
                    anyhow::ensure!(
                        actual.is_some_and(|actual| actual.as_f64() == expected),
                        "Native playback value mismatch"
                    );
                }
                let artwork: Option<Retained<AnyObject>> =
                    msg_send![&info, objectForKey: &*MPMediaItemPropertyArtwork];
                let artwork = artwork
                    .ok_or_else(|| anyhow::anyhow!("Native Now Playing artwork is missing"))?;
                let image: Option<Retained<AnyObject>> = msg_send![&*artwork,
                    imageWithSize: CGSize { width: 32.0, height: 32.0 }];
                anyhow::ensure!(image.is_some(), "Native artwork callback returned no image");
                let actual: isize = msg_send![center, playbackState];
                anyhow::ensure!(
                    actual
                        == if snapshot.status == PlayStatus::Playing {
                            1
                        } else {
                            2
                        },
                    "Native playback state mismatch"
                );
                if snapshot.status == PlayStatus::Playing && stage.load(Ordering::Acquire) == 0 {
                    stage.store(1, Ordering::Release);
                    let event = NSObject::new();
                    let result: isize = msg_send![target, pause: &*event];
                    anyhow::ensure!(result == 0, "Native command rejected");
                }
                if snapshot.status == PlayStatus::Paused {
                    stage.store(2, Ordering::Release);
                }
            }
            Ok(())
        },
    )
}
