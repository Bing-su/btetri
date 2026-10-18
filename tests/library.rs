use std::time::Duration;

use btetri::game::{Action, Game, InvalidLevel, InvalidMode, Mode, Rules, Status};

#[test]
fn a_frontend_can_drive_a_deterministic_game() {
    let mut first = Game::new("marathon".parse().unwrap(), Rules::default(), 42).unwrap();
    let mut second = Game::new(Mode::Marathon, Rules::default(), 42).unwrap();
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
    assert_eq!(error.to_string(), "expected marathon, sprint, or ultra");

    for (start_level, max_level) in [(0, 20), (2, 1), (1, 21)] {
        let result = Game::new(
            Mode::Marathon,
            Rules {
                start_level,
                max_level,
                ..Rules::default()
            },
            42,
        );
        assert!(matches!(result, Err(InvalidLevel)));
    }
    // Callers can propagate both library errors through standard error handling.
    let errors: [Box<dyn std::error::Error + Send + Sync>; 2] =
        [Box::new(InvalidMode), Box::new(InvalidLevel)];
    assert!(errors.iter().all(|error| error.source().is_none()));
}
