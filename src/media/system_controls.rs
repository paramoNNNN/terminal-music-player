//! Native media commands pass through the ordinary playback reducers. AppState
//! remains authoritative, including idempotent Play/Pause and stale commands.
use crate::app::action::PlaybackAction;
use crate::app::state::PlayStatus;
use crate::app::AppState;

#[derive(Debug, Clone, Copy)]
pub enum Command {
    Play,
    Pause,
    Toggle,
    Next,
    Previous,
    Stop,
    Seek(u64),
    Volume(f32),
}

pub fn playback_action(command: Command, state: &AppState) -> Option<PlaybackAction> {
    Some(match command {
        Command::Play
            if matches!(
                state.playback.status,
                PlayStatus::Playing | PlayStatus::Buffering
            ) =>
        {
            return None
        }
        Command::Pause
            if !matches!(
                state.playback.status,
                PlayStatus::Playing | PlayStatus::Buffering
            ) =>
        {
            return None
        }
        Command::Play | Command::Pause | Command::Toggle => PlaybackAction::TogglePlayPause,
        Command::Next => PlaybackAction::Next,
        Command::Previous => PlaybackAction::Previous,
        Command::Stop => PlaybackAction::Stop,
        Command::Seek(position) => PlaybackAction::Seek(position.min(state.playback.duration_ms)),
        Command::Volume(volume) => PlaybackAction::SetVolume(volume),
    })
}

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
pub use macos::{run, sync, verify_native};

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
pub use linux::{run, sync};

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
pub fn sync(_state: &AppState) -> Vec<Command> {
    Vec::new()
}
#[cfg(not(any(target_os = "macos", target_os = "linux")))]
pub fn run(work: impl FnOnce() -> anyhow::Result<()> + Send + 'static) -> anyhow::Result<()> {
    work()
}
