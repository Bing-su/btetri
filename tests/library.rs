use std::time::Duration;

use btetri::game::{Action, Game, InvalidMode, Mode};
use proptest::prelude::*;

proptest! {
    #[test]
    fn a_frontend_can_drive_a_deterministic_game(
        seed in any::<u64>(),
        mode in prop::sample::select(&Mode::ALL),
        steps in prop::collection::vec((
            prop::option::of(prop::sample::select(&[
                Action::Left, Action::Right, Action::Clockwise, Action::CounterClockwise,
                Action::Rotate180, Action::HardDrop, Action::SoftDrop, Action::Hold,
            ])),
            0_u64..1000,
        ), 1..100),
    ) {
        let mut first = Game::new(mode, seed);
        let mut second = Game::new(mode, seed);
        for (action, milliseconds) in steps {
            for game in [&mut first, &mut second] {
                if let Some(action) = action {
                    game.apply(action);
                } else {
                    game.advance(Duration::from_millis(milliseconds));
                }
            }
            prop_assert_eq!(first.board(), second.board());
            prop_assert_eq!(first.active(), second.active());
            prop_assert_eq!(first.ghost(), second.ghost());
            prop_assert_eq!(first.held(), second.held());
            prop_assert_eq!(first.next().collect::<Vec<_>>(), second.next().collect::<Vec<_>>());
            prop_assert_eq!(first.score(), second.score());
            prop_assert_eq!(first.last_score(), second.last_score());
            prop_assert_eq!(first.lines(), second.lines());
            prop_assert_eq!(first.level(), second.level());
            prop_assert_eq!(first.practice_resets(), second.practice_resets());
            prop_assert_eq!(first.elapsed(), second.elapsed());
            prop_assert_eq!(first.remaining(), second.remaining());
            prop_assert_eq!(first.status(), second.status());
        }
    }
}

#[rstest::rstest]
fn callers_can_match_typed_errors(
    #[values("unknown", "marathon", "sprint", "ultra", "")] name: &str,
) {
    let error: InvalidMode = name.parse::<Mode>().unwrap_err();
    assert_eq!(error, InvalidMode);
    assert_eq!(error.to_string(), "expected 40l, blitz, or practice");
}

#[rstest::rstest]
fn mode_names_round_trip(#[values(Mode::FortyLines, Mode::Blitz, Mode::Practice)] mode: Mode) {
    assert_eq!(mode.to_string().parse::<Mode>().unwrap(), mode);
}
