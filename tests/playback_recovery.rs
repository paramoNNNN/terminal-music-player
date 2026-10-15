use textamp::app::{
    action::PlaybackAction, dispatch::handle_core_event, event::PlaybackEvent,
    handlers::dispatch_playback, state::PlayStatus, AppState,
};
use textamp::audio::AudioPlayer;

#[tokio::test]
async fn failed_stream_keeps_position_and_pause_until_scoped_retry() {
    for status in [PlayStatus::Playing, PlayStatus::Paused] {
        let mut state = AppState::new();
        state.playback.request_id = 12;
        state.playback.status = status;
        state.playback.position_ms = 123_000;
        state.playback.scrobble_reported = true;
        let (tx, mut rx) = tokio::sync::mpsc::channel(8);
        handle_core_event(
            PlaybackEvent::PlaybackError {
                playback_id: Some(12),
                message: "audio transfer was interrupted".into(),
            }
            .into(),
            &mut state,
            &tx,
        );
        assert_eq!(state.playback.position_ms, 123_000);
        handle_core_event(
            PlaybackEvent::BufferingEnd { playback_id: 12 }.into(),
            &mut state,
            &tx,
        );
        assert_eq!(state.playback.retry_pending, Some(12));
        assert_eq!(
            state.playback.status,
            if status == PlayStatus::Paused {
                PlayStatus::Paused
            } else {
                PlayStatus::Buffering
            }
        );
        if status == PlayStatus::Paused {
            let mut audio = AudioPlayer::new_without_audio();
            dispatch_playback::dispatch(
                &tx,
                PlaybackAction::TogglePlayPause,
                &mut state,
                &mut audio,
            )
            .await
            .unwrap();
            assert_eq!(state.playback.status, PlayStatus::Buffering);
        }
        let retry = tokio::time::timeout(std::time::Duration::from_secs(2), rx.recv())
            .await
            .unwrap()
            .unwrap();
        let actions = handle_core_event(retry, &mut state, &tx);
        assert_eq!(actions.len(), 1);
        assert!(matches!(
            &actions[0],
            textamp::app::Action::Playback(PlaybackAction::RetryCurrentTrack)
        ));
        assert!(handle_core_event(
            PlaybackEvent::RetryAfterDelay { playback_id: 12 }.into(),
            &mut state,
            &tx
        )
        .is_empty());
        assert!(state.playback.scrobble_reported);

        state.playback.retry_pending = Some(12);
        let mut audio = AudioPlayer::new_without_audio();
        dispatch_playback::dispatch(&tx, PlaybackAction::Stop, &mut state, &mut audio)
            .await
            .unwrap();
        assert!(handle_core_event(
            PlaybackEvent::RetryAfterDelay { playback_id: 12 }.into(),
            &mut state,
            &tx
        )
        .is_empty());
    }
}

#[test]
fn paused_recovery_readiness_preserves_pause_and_position() {
    let mut state = AppState::new();
    state.playback.status = PlayStatus::Paused;
    state.playback.request_id = 4;
    state.playback.position_ms = 123_000;
    let (tx, _rx) = tokio::sync::mpsc::channel(8);
    handle_core_event(
        PlaybackEvent::BufferingEnd { playback_id: 3 }.into(),
        &mut state,
        &tx,
    );
    assert!(state.playback.playback_started_at.is_none());
    handle_core_event(
        PlaybackEvent::BufferingEnd { playback_id: 4 }.into(),
        &mut state,
        &tx,
    );
    assert_eq!(state.playback.status, PlayStatus::Paused);
    assert_eq!(state.playback.position_ms, 123_000);
    assert!(state.playback.playback_started_at.is_some());
}
