use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use textamp::app::{
    action::QueueAction,
    handlers::{dispatch_queue, key_input},
    state::View,
    Action, AppState,
};
use textamp::audio::AudioPlayer;
use textamp::library::models::Track;

fn song(key: &str) -> Track {
    Track {
        rating_key: key.into(),
        title: key.into(),
        ..Default::default()
    }
}

#[tokio::test]
async fn catalog_play_replaces_in_display_order_and_queue_edits_require_explicit_confirmation() {
    let mut state = AppState::new();
    let mut audio = AudioPlayer::new_without_audio();
    let (tx, _rx) = tokio::sync::mpsc::channel(8);
    let tracks = vec![
        song("third-by-name"),
        song("first-by-name"),
        song("second-by-name"),
    ];
    dispatch_queue::dispatch(
        &tx,
        QueueAction::PlayCatalog {
            tracks: tracks.clone(),
            index: 1,
        },
        &mut state,
        &mut audio,
    )
    .await
    .unwrap();
    assert_eq!(
        state
            .queue
            .tracks
            .iter()
            .map(|track| track.rating_key.as_str())
            .collect::<Vec<_>>(),
        ["third-by-name", "first-by-name", "second-by-name"]
    );
    assert_eq!(state.queue.index, Some(1));
    assert!(!state.queue.is_modified());
    state.queue.index = Some(2); // advancing or metadata changes aren't queue edits
    state.queue.tracks[0].favorited = Some(true);
    assert!(!state.queue.is_modified());
    dispatch_queue::dispatch(
        &tx,
        QueueAction::EnqueueTrack(Box::new(song("manual addition"))),
        &mut state,
        &mut audio,
    )
    .await
    .unwrap();
    assert!(state.queue.is_modified());
    state.view = View::Related;
    dispatch_queue::dispatch(
        &tx,
        QueueAction::PlayCatalog {
            tracks: vec![song("replacement")],
            index: 0,
        },
        &mut state,
        &mut audio,
    )
    .await
    .unwrap();
    assert!(!state.popups.confirm_dialog.as_ref().unwrap().selected_yes);
    assert_eq!(state.queue.tracks.len(), 4);
    assert_eq!(state.view, View::Related);
    let cancel = key_input::handle_key(
        KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
        &mut state,
        &Default::default(),
    );
    assert!(cancel.is_empty());
    assert!(state.popups.confirm_dialog.is_none());
    assert_eq!(state.queue.tracks.len(), 4);
    dispatch_queue::dispatch(
        &tx,
        QueueAction::PlayCatalog {
            tracks: vec![song("replacement")],
            index: 0,
        },
        &mut state,
        &mut audio,
    )
    .await
    .unwrap();
    let actions = key_input::handle_key(
        KeyEvent::new(KeyCode::Char('y'), KeyModifiers::NONE),
        &mut state,
        &Default::default(),
    );
    let [Action::Queue(action)] = actions.as_slice() else {
        panic!("expected confirmed catalog replacement");
    };
    dispatch_queue::dispatch(&tx, action.clone(), &mut state, &mut audio)
        .await
        .unwrap();
    assert_eq!(state.queue.tracks[0].rating_key, "replacement");
    assert_eq!(state.queue.tracks.len(), 1);
    assert_eq!(state.view, View::NowPlaying);
    assert!(!state.queue.is_modified());
}

#[tokio::test]
async fn catalog_confirmation_cannot_replace_a_newer_queue_or_library() {
    for change_library in [false, true] {
        let mut state = AppState::new();
        state.queue.tracks = vec![song("protected")]; // restored/unknown baseline is protected
        let mut audio = AudioPlayer::new_without_audio();
        let (tx, _rx) = tokio::sync::mpsc::channel(8);
        dispatch_queue::dispatch(
            &tx,
            QueueAction::PlayCatalog {
                tracks: vec![song("replacement")],
                index: 0,
            },
            &mut state,
            &mut audio,
        )
        .await
        .unwrap();
        let actions = key_input::handle_key(
            KeyEvent::new(KeyCode::Char('y'), KeyModifiers::NONE),
            &mut state,
            &Default::default(),
        );
        let [Action::Queue(action)] = actions.as_slice() else {
            panic!("expected confirmation");
        };
        if change_library {
            state.library_generation += 1;
        } else {
            state.queue.tracks.push(song("newer edit"));
        }
        dispatch_queue::dispatch(&tx, action.clone(), &mut state, &mut audio)
            .await
            .unwrap();
        assert_eq!(state.queue.tracks[0].rating_key, "protected");
        assert!(state.notifications.last_error.is_some());
    }
}
