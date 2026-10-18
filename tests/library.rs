use std::time::Duration;

use btetri::game::{Action, Game, InvalidMode, Mode, Status};

#[test]
fn a_frontend_can_drive_a_deterministic_game() {
    let mut first = Game::new("40l".parse().unwrap(), 42);
    let mut second = Game::new(Mode::FortyLines, 42);
    for game in [&mut first, &mut second] {
        game.apply(Action::Hold);
        game.apply(Action::Left);
        game.apply(Action::Clockwise);
        game.advance(Duration::from_millis(250));
        game.apply(Action::HardDrop);
    }
    assert_eq!(first.board(), second.board());
    assert_eq!(first.active().cells(), second.active().cells());
    assert_eq!(first.held(), second.held());
    assert_eq!(
        first.next().collect::<Vec<_>>(),
        second.next().collect::<Vec<_>>()
    );
    assert_eq!(first.score(), second.score());
    assert!(first.score() > 0);
    assert_eq!(first.elapsed(), Duration::from_millis(250));
    assert_eq!(first.status(), Status::Playing);
}

#[test]
fn callers_can_match_typed_errors() {
    let error: InvalidMode = "unknown".parse::<Mode>().unwrap_err();
    assert_eq!(error, InvalidMode);
    assert_eq!(error.to_string(), "expected 40l, blitz, or practice");
    for mode in Mode::ALL {
        assert_eq!(mode.to_string().parse::<Mode>().unwrap(), mode);
    }
    for retired in ["marathon", "sprint", "ultra"] {
        assert_eq!(retired.parse::<Mode>(), Err(InvalidMode));
    }
}
