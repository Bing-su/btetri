use proptest::prelude::*;
use rstest::rstest;

use super::*;

fn game() -> Game {
    Game::new(Mode::Marathon, Rules::default(), 42).unwrap()
}

#[rstest]
#[case(1, 100)]
#[case(2, 300)]
#[case(3, 500)]
#[case(4, 800)]
fn clears_rows_and_scores_before_level_change(#[case] rows: usize, #[case] score: u64) {
    let mut game = game();
    // Leave a hidden block so this fixture measures ordinary line points, not PC.
    game.board[0][0] = Some(Piece::J);
    for row in game.board.iter_mut().rev().take(rows) {
        *row = [Some(Piece::J); WIDTH];
        row[4] = None;
    }
    game.active = ActivePiece {
        kind: Piece::I,
        x: 2,
        y: 18,
        rotation: 1,
    };
    assert!(game.fits(game.active));
    game.apply(Action::HardDrop);
    assert_eq!(game.lines(), u32::try_from(rows).unwrap());
    assert_eq!(game.score(), score);
    assert!(game.board.iter().all(|row| row.iter().any(Option::is_none)));
}

#[test]
fn hold_is_once_per_locked_piece_and_resets_rotation() {
    let mut game = game();
    let first = game.active.kind;
    game.apply(Action::Clockwise);
    game.apply(Action::Hold);
    assert_eq!(game.held(), Some(first));
    let second = game.active;
    game.apply(Action::Hold);
    assert_eq!(game.active, second);
    game.apply(Action::HardDrop);
    game.apply(Action::Hold);
    assert_eq!(game.active.kind, first);
    assert_eq!(game.active.rotation, 0);
}

#[test]
fn hard_drop_locks_immediately_at_ghost_position() {
    let mut game = game();
    let ghost = game.ghost();
    let expected_points = u64::try_from(ghost.y - game.active.y).unwrap() * 2;
    game.apply(Action::HardDrop);
    for (x, y) in ghost.cells() {
        assert_eq!(
            game.board[usize::try_from(y).unwrap()][usize::try_from(x).unwrap()],
            Some(ghost.kind)
        );
    }
    assert_eq!(game.score(), expected_points);
}

#[test]
fn grounded_piece_waits_half_second() {
    let mut game = game();
    game.active = ActivePiece {
        kind: Piece::O,
        rotation: 0,
        x: 3,
        y: 20,
    };
    game.advance(Duration::from_millis(499));
    assert!(game.board.iter().flatten().all(Option::is_none));
    game.advance(Duration::from_millis(1));
    assert_eq!(
        game.board
            .iter()
            .flatten()
            .filter(|cell| cell.is_some())
            .count(),
        4
    );
}

#[test]
fn extended_lock_delay_stops_resetting_after_fifteen_moves() {
    let mut game = game();
    game.active = ActivePiece {
        kind: Piece::O,
        rotation: 0,
        x: 3,
        y: 20,
    };
    for index in 0..15 {
        game.advance(Duration::from_millis(400));
        game.apply(if index % 2 == 0 {
            Action::Left
        } else {
            Action::Right
        });
    }
    assert!(game.board.iter().flatten().all(Option::is_none));
    game.advance(Duration::from_millis(400));
    game.apply(Action::Right);
    game.advance(Duration::from_millis(100));
    assert_eq!(
        game.board
            .iter()
            .flatten()
            .filter(|cell| cell.is_some())
            .count(),
        4
    );
}

#[test]
fn srs_rotates_i_off_left_wall() {
    let mut game = game();
    game.active = ActivePiece {
        kind: Piece::I,
        rotation: 1,
        x: -2,
        y: 5,
    };
    assert!(game.fits(game.active));
    game.apply(Action::CounterClockwise);
    assert_eq!(game.active.rotation, 0);
    assert_eq!(game.active.x, 0);
    assert!(game.fits(game.active));
}

#[rstest]
fn half_turn_rotates_directly_and_returns_after_two_turns(
    #[values(Piece::I, Piece::T, Piece::S, Piece::Z, Piece::J, Piece::L)] kind: Piece,
    #[values(0, 1, 2, 3)] rotation: u8,
) {
    let mut game = game();
    game.active = ActivePiece {
        kind,
        rotation,
        x: 3,
        y: 5,
    };
    let before = game.active;
    game.apply(Action::Rotate180);
    assert_eq!(game.active.rotation, (rotation + 2) % 4);
    assert_eq!((game.active.x, game.active.y), (before.x, before.y));
    game.apply(Action::Rotate180);
    assert_eq!(game.active, before);
}

#[test]
fn half_turn_kicks_off_floor_and_keeps_o_stable() {
    let mut game = game();
    game.active = ActivePiece {
        kind: Piece::T,
        rotation: 0,
        x: 3,
        y: 20,
    };
    game.apply(Action::Rotate180);
    assert_eq!(game.active.rotation, 2);
    assert_eq!(game.active.y, 19);
    assert!(game.fits(game.active));
    game.active = ActivePiece::spawn(Piece::O);
    let before = game.active;
    game.apply(Action::Rotate180);
    assert_eq!(game.active, before);
}

#[test]
fn soft_drop_steps_scores_and_held_state_is_idempotent() {
    let mut game = game();
    let before = game.active;
    game.apply(Action::SoftDrop);
    assert_eq!(game.active.y, before.y + 1);
    assert_eq!(game.score(), 1);
    assert!(!game.soft_drop());
    game.set_soft_drop(true);
    game.advance(Duration::from_millis(25));
    game.set_soft_drop(true);
    game.advance(Duration::from_millis(25));
    assert_eq!(game.active.y, before.y + 2);
    assert_eq!(game.score(), 2);
    game.set_soft_drop(false);
    game.advance(Duration::from_millis(100));
    assert_eq!(game.active.y, before.y + 2);
    game.active = game.ghost();
    let score = game.score();
    game.apply(Action::SoftDrop);
    assert_eq!(game.score(), score);
}

#[test]
fn spawn_collision_ends_game() {
    let mut game = game();
    game.board[2] = [Some(Piece::O); WIDTH];
    game.spawn(Piece::T);
    assert_eq!(game.status(), Status::TopOut);
}

#[test]
fn ultra_finishes_at_exactly_two_minutes() {
    let mut game = Game::new(Mode::Ultra, Rules::default(), 42).unwrap();
    game.elapsed = Duration::from_millis(119_999);
    game.advance(Duration::from_secs(1));
    assert_eq!(game.status(), Status::Complete);
    assert_eq!(game.elapsed(), Duration::from_secs(120));
}

#[test]
fn sprint_finishes_after_fortieth_line() {
    let mut game = Game::new(Mode::Sprint, Rules::default(), 42).unwrap();
    game.lines = 39;
    game.board[21] = [Some(Piece::J); WIDTH];
    game.board[21][4] = None;
    game.active = ActivePiece {
        kind: Piece::I,
        rotation: 1,
        x: 2,
        y: 18,
    };
    game.apply(Action::HardDrop);
    assert_eq!(game.lines(), 40);
    assert_eq!(game.status(), Status::Complete);
}

// Start one turn before a grounded T-spin so tests exercise input and lock scoring together.
fn spin_slot(spin: Spin, lines: usize) -> Game {
    let mut game = game();
    game.active = ActivePiece {
        kind: Piece::T,
        rotation: 1,
        x: 3,
        y: 19,
    };
    for (x, y) in [(3, 19), (3, 21)] {
        game.board[y][x] = Some(Piece::J);
    }
    let third_corner = if spin == Spin::Full { (5, 21) } else { (5, 19) };
    game.board[third_corner.1][third_corner.0] = Some(Piece::J);
    if lines >= 1 {
        game.board[20] = [Some(Piece::J); WIDTH];
        for x in 3..=5 {
            game.board[20][x] = None;
        }
    }
    if lines >= 2 {
        game.board[21] = [Some(Piece::J); WIDTH];
        game.board[21][4] = None;
    }
    assert!(game.fits(game.active));
    game.apply(Action::Clockwise);
    assert_eq!(game.active.rotation, 2);
    game
}

#[rstest]
#[case(Spin::Full, 0, 400)]
#[case(Spin::Full, 1, 800)]
#[case(Spin::Full, 2, 1200)]
#[case(Spin::Mini, 0, 100)]
#[case(Spin::Mini, 1, 200)]
fn rotated_t_scores_before_rows_are_removed(
    #[case] spin: Spin,
    #[case] lines: usize,
    #[case] points: u64,
) {
    let mut game = spin_slot(spin, lines);
    game.apply(Action::HardDrop);
    let event = game.last_score().unwrap();
    assert_eq!(event.spin, spin);
    assert_eq!(event.lines, u32::try_from(lines).unwrap());
    assert_eq!(game.score(), points);
}

#[test]
fn blocked_moves_preserve_a_spin_but_successful_movement_cancels_it() {
    let mut blocked = spin_slot(Spin::Full, 0);
    blocked.apply(Action::Left);
    blocked.apply(Action::Right);
    blocked.apply(Action::HardDrop);
    assert_eq!(blocked.score(), 400);
    let mut shifted = spin_slot(Spin::Mini, 0);
    shifted.apply(Action::Right);
    shifted.apply(Action::Left);
    shifted.apply(Action::HardDrop);
    assert_eq!(shifted.score(), 0);
    assert_eq!(shifted.last_score().unwrap().spin, Spin::None);
}

#[rstest]
#[case(false, 3, 1600)]
#[case(true, 2, 1200)]
fn fifth_srs_kick_scores_triple_and_promotes_mini(
    #[case] open_front: bool,
    #[case] lines: u32,
    #[case] points: u64,
) {
    let mut game = game();
    for y in 18..=20 {
        game.board[y] = [Some(Piece::J); WIDTH];
        game.board[y][4] = None;
    }
    game.board[19][5] = None;
    game.board[21][4] = Some(Piece::J);
    game.board[16][4] = Some(Piece::J);
    if open_front {
        game.board[20][5] = None;
    }
    game.active = ActivePiece {
        kind: Piece::T,
        rotation: 0,
        x: 4,
        y: 16,
    };
    assert!(game.fits(game.active));
    game.apply(Action::Clockwise);
    assert_eq!(
        game.active,
        ActivePiece {
            kind: Piece::T,
            rotation: 1,
            x: 3,
            y: 18
        }
    );
    game.apply(Action::HardDrop);
    assert_eq!(game.lines(), lines);
    assert_eq!(game.last_score().unwrap().spin, Spin::Full);
    assert_eq!(game.score(), points);
}

#[test]
fn spin_uses_the_level_before_crossing_ten_lines() {
    let mut game = spin_slot(Spin::Full, 2);
    game.lines = 9;
    game.apply(Action::HardDrop);
    assert_eq!(game.level(), 2);
    assert_eq!(game.score(), 1200);
}

#[test]
fn fourth_srs_kick_keeps_a_mini_double_as_mini() {
    let mut game = game();
    for y in 18..=20 {
        game.board[y] = [Some(Piece::J); WIDTH];
        game.board[y][4] = None;
    }
    game.board[19][5] = None;
    game.board[20][5] = None;
    game.board[21][4] = Some(Piece::J);
    game.board[16][3] = Some(Piece::J);
    game.board[16][4] = Some(Piece::J);
    game.active = ActivePiece {
        kind: Piece::T,
        rotation: 2,
        x: 3,
        y: 16,
    };
    assert!(game.fits(game.active));
    game.apply(Action::CounterClockwise);
    assert_eq!(
        game.active,
        ActivePiece {
            kind: Piece::T,
            rotation: 1,
            x: 3,
            y: 18
        }
    );
    game.apply(Action::HardDrop);
    assert_eq!(game.last_score().unwrap().spin, Spin::Mini);
    assert_eq!(game.lines(), 2);
    assert_eq!(game.score(), 400);
}

#[test]
fn wall_and_floor_count_as_spin_corners() {
    let mut wall = game();
    wall.board[20][1] = Some(Piece::J);
    wall.active = ActivePiece {
        kind: Piece::T,
        rotation: 0,
        x: 0,
        y: 18,
    };
    wall.apply(Action::Clockwise);
    assert_eq!(wall.active.x, -1);
    wall.apply(Action::HardDrop);
    assert_eq!(wall.last_score().unwrap().spin, Spin::Mini);
    assert_eq!(wall.score(), 100);

    let mut floor = game();
    floor.board[20][3] = Some(Piece::J);
    floor.board[20][6] = Some(Piece::J);
    floor.active = ActivePiece {
        kind: Piece::T,
        rotation: 1,
        x: 3,
        y: 19,
    };
    floor.apply(Action::CounterClockwise);
    assert_eq!(floor.active.y, 20);
    floor.apply(Action::HardDrop);
    assert_eq!(floor.last_score().unwrap().spin, Spin::Mini);
    assert_eq!(floor.score(), 100);
}

#[test]
fn fewer_than_three_corners_does_not_score_a_spin() {
    let mut game = spin_slot(Spin::Full, 0);
    game.board[19][3] = None;
    game.apply(Action::HardDrop);
    assert_eq!(game.last_score().unwrap().spin, Spin::None);
    assert_eq!(game.score(), 0);
}

#[test]
fn locks_keep_scoring_chains_and_exclude_drop_points_from_b2b() {
    let mut game = Game::new(
        Mode::Marathon,
        Rules {
            max_level: 1,
            ..Rules::default()
        },
        42,
    )
    .unwrap();
    let mut total_drop_points = 0;
    for (lines, expected, bonus) in [
        (4, 800, false),
        (4, 2050, true),
        (0, 2050, false),
        (4, 3250, true),
        (1, 3400, false),
        (4, 4300, false),
    ] {
        game.board = [[None; WIDTH]; HEIGHT];
        game.board[0][0] = Some(Piece::J);
        for row in game.board.iter_mut().rev().take(lines) {
            *row = [Some(Piece::J); WIDTH];
            row[4] = None;
        }
        game.active = ActivePiece {
            kind: Piece::I,
            rotation: 1,
            x: 2,
            y: 17,
        };
        let drop_points = 2;
        total_drop_points += drop_points;
        let before = game.score();
        game.apply(Action::HardDrop);
        assert_eq!(
            game.score() - before,
            game.last_score().unwrap().points + drop_points
        );
        assert_eq!(game.score(), expected + total_drop_points);
        assert_eq!(game.last_score().unwrap().back_to_back, bonus);
    }
}

fn perfect_clear_slot(lines: usize) -> Game {
    let mut game = game();
    game.active = match lines {
        1 => ActivePiece {
            kind: Piece::I,
            rotation: 0,
            x: 3,
            y: 20,
        },
        2 => ActivePiece {
            kind: Piece::O,
            rotation: 0,
            x: 3,
            y: 20,
        },
        3 => ActivePiece {
            kind: Piece::T,
            rotation: 1,
            x: 3,
            y: 19,
        },
        _ => ActivePiece {
            kind: Piece::I,
            rotation: 1,
            x: 2,
            y: 18,
        },
    };
    for row in game.board.iter_mut().rev().take(lines) {
        *row = [Some(Piece::J); WIDTH];
    }
    for (x, y) in game.active.cells() {
        game.board[usize::try_from(y).unwrap()][usize::try_from(x).unwrap()] = None;
    }
    assert!(game.fits(game.active));
    game
}

#[rstest]
#[case(1, 900)]
#[case(2, 1500)]
#[case(3, 2300)]
#[case(4, 2800)]
fn perfect_clear_awards_bonus_at_level_before_line_removal(
    #[case] lines: usize,
    #[case] total: u64,
) {
    let mut game = perfect_clear_slot(lines);
    game.rules.start_level = 3;
    game.lines = 9;
    game.apply(Action::HardDrop);
    assert!(game.board.iter().flatten().all(Option::is_none));
    assert!(game.last_score().unwrap().perfect_clear);
    assert_eq!(game.score(), total * 3);
    assert_eq!(game.level(), 4);
}

#[test]
fn hidden_or_visible_remaining_blocks_prevent_perfect_clear() {
    for row in [0, 10] {
        let mut game = perfect_clear_slot(1);
        game.board[row][0] = Some(Piece::J);
        game.apply(Action::HardDrop);
        assert!(!game.last_score().unwrap().perfect_clear);
        assert_eq!(game.score(), 100);
    }
    let mut game = game();
    game.apply(Action::HardDrop);
    assert_eq!(game.last_score().unwrap().lines, 0);
    assert!(!game.last_score().unwrap().perfect_clear);
}

#[test]
fn b2b_perfect_clear_adds_3200_and_combo_without_multiplying_the_bonus() {
    let mut game = perfect_clear_slot(4);
    game.apply(Action::HardDrop);
    assert_eq!(game.score(), 2800);
    let next = perfect_clear_slot(4);
    game.board = next.board;
    game.active = next.active;
    game.apply(Action::HardDrop);
    let event = game.last_score().unwrap();
    assert!(event.perfect_clear);
    assert!(event.back_to_back);
    assert_eq!(event.combo, Some(1));
    assert_eq!(event.points, 1200 + 3200 + 50);
    assert_eq!(game.score(), 7250);
}

#[test]
fn moving_down_after_rotation_cancels_spin_and_spawn_does_not_inherit_it() {
    for hard_drop in [false, true] {
        let mut falling = game();
        for (x, y) in [(3, 19), (3, 21), (5, 21)] {
            falling.board[y][x] = Some(Piece::J);
        }
        falling.active = ActivePiece {
            kind: Piece::T,
            rotation: 0,
            x: 3,
            y: 17,
        };
        falling.apply(Action::Clockwise);
        if !hard_drop {
            falling.advance(Duration::from_secs(2));
        }
        falling.apply(Action::HardDrop);
        assert_eq!(falling.last_score().unwrap().spin, Spin::None);
        assert_eq!(falling.score(), if hard_drop { 4 } else { 0 });
    }
    let mut spawned = spin_slot(Spin::Full, 0);
    spawned.spawn(Piece::T);
    spawned.active = ActivePiece {
        kind: Piece::T,
        rotation: 2,
        x: 3,
        y: 19,
    };
    spawned.apply(Action::HardDrop);
    assert_eq!(spawned.last_score().unwrap().spin, Spin::None);
}

proptest! {
    #[test]
    fn every_seven_bag_contains_each_piece(seed in any::<u64>()) {
        let mut game = Game::new(Mode::Marathon, Rules::default(), seed).unwrap();
        for _ in 0..10 {
            let mut bag = Vec::new();
            for _ in 0..7 { bag.push(game.active.kind); game.spawn_next(); }
            for piece in Piece::ALL { prop_assert_eq!(bag.iter().filter(|&&actual| actual == piece).count(), 1); }
        }
    }

    #[test]
    fn generated_moves_preserve_board_and_ghost_invariants(seed in any::<u64>(), actions in prop::collection::vec(0_u8..9, 0..200)) {
        let mut game = Game::new(Mode::Marathon, Rules::default(), seed).unwrap();
        for action in actions {
            if game.status() != Status::Playing { break; }
            match action {
                0 => game.apply(Action::Left), 1 => game.apply(Action::Right),
                2 => game.apply(Action::Clockwise), 3 => game.apply(Action::CounterClockwise),
                4 => game.apply(Action::HardDrop), 5 => game.apply(Action::Hold),
                6 => game.apply(Action::SoftDrop), 7 => game.apply(Action::Rotate180), _ => game.advance(Duration::from_millis(700)),
            }
            prop_assert!(game.board.iter().all(|row| row.iter().any(Option::is_none)));
            if game.status() == Status::Playing {
                prop_assert!(game.fits(game.active));
                let ghost = game.ghost();
                prop_assert!(game.fits(ghost));
                prop_assert!(!game.fits(ActivePiece { y: ghost.y + 1, ..ghost }), "ghost must rest on a surface");
            }
        }
    }
}
