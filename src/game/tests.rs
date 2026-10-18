use proptest::prelude::*;
use rstest::rstest;

use super::*;

#[rstest]
#[case(0, 1)]
#[case(2, 1)]
#[case(3, 2)]
#[case(7, 2)]
#[case(8, 3)]
#[case(14, 3)]
#[case(15, 4)]
#[case(24, 5)]
#[case(120, 11)]
#[case(198, 14)]
fn blitz_progresses_at_cumulative_goals(#[case] lines: u32, #[case] level: u32) {
    let mut game = Game::new(Mode::Blitz, 42);
    game.lines = lines;
    assert_eq!(game.level(), level);
}

#[rstest]
#[case(0, Duration::from_secs(1))]
#[case(3, Duration::from_millis(643))]
#[case(198, Duration::from_nanos(833_333))]
#[case(260, Duration::from_nanos(833_333))]
fn blitz_gravity_matches_levels_and_stays_capped(#[case] lines: u32, #[case] interval: Duration) {
    let mut game = Game::new(Mode::Blitz, 42);
    game.lines = lines;
    assert_eq!(game.gravity_interval(), interval);
}

#[rstest]
#[case(Mode::FortyLines)]
#[case(Mode::Practice)]
fn untimed_modes_keep_fixed_gravity(#[case] mode: Mode) {
    let mut game = Game::new(mode, 42);
    game.lines = 200;
    assert_eq!(game.level(), 1);
    assert_eq!(game.remaining(), None);
    let y = game.active().y;
    game.advance(Duration::from_millis(833));
    assert_eq!(game.active().y, y);
    game.advance(Duration::from_millis(1));
    assert_eq!(game.active().y, y + 1);
}

#[rstest]
#[case(Mode::FortyLines, Spin::Mini)]
#[case(Mode::Practice, Spin::Mini)]
#[case(Mode::Blitz, Spin::None)]
fn immobile_o_rotations_follow_the_modes_spin_rule(#[case] mode: Mode, #[case] spin: Spin) {
    let mut game = Game::new(mode, 42);
    game.active = ActivePiece {
        kind: Piece::O,
        rotation: 0,
        x: 3,
        y: 37,
    };
    // Box the O in without filling rows, e.g. a spin must stay immobile in all four directions.
    for row in 36..40 {
        for column in 3..7 {
            game.board[row][column] = Some(Piece::J);
        }
    }
    for (x, y) in game.active.cells() {
        game.board[usize::try_from(y).unwrap()][usize::try_from(x).unwrap()] = None;
    }
    game.apply(Action::Clockwise);
    game.apply(Action::HardDrop);
    assert_eq!(game.last_score().unwrap().spin, spin);
    assert_eq!(game.score(), if spin == Spin::Mini { 100 } else { 0 });
}

#[rstest]
#[case(Action::Hold, false)]
#[case(Action::HardDrop, false)]
#[case(Action::HardDrop, true)]
fn practice_recovers_from_topout_without_losing_session_totals(
    #[case] action: Action,
    #[case] timed: bool,
) {
    let mut game = Game::new(Mode::Practice, 42);
    game.score = 123;
    game.lines = 42;
    game.elapsed = Duration::from_secs(150);
    game.board = [[Some(Piece::J); WIDTH]; HEIGHT];
    for (x, y) in game.active.cells() {
        game.board[usize::try_from(y).unwrap()][usize::try_from(x).unwrap()] = None;
    }
    // Block the hold spawn, or lock entirely above the skyline without clearing a line.
    game.board[HEIGHT - 1][0] = None;
    if action == Action::HardDrop {
        game.board = [[None; WIDTH]; HEIGHT];
        for (x, y) in game.active.cells() {
            game.board[usize::try_from(y + 2).unwrap()][usize::try_from(x).unwrap()] =
                Some(Piece::J);
        }
    }
    let elapsed = if timed {
        game.advance(Duration::from_millis(500));
        Duration::from_millis(150_500)
    } else {
        game.apply(action);
        Duration::from_secs(150)
    };
    assert_eq!(game.status(), Status::Playing);
    assert_eq!(game.practice_resets(), 1);
    assert!(game.board().iter().flatten().all(Option::is_none));
    assert_eq!(game.lines(), 42);
    assert!(game.score() >= 123);
    assert_eq!(game.elapsed(), elapsed);
    assert_eq!(game.held(), None);
    assert_eq!(game.last_score(), None);
    assert!(game.fits(game.active()));
    game.advance(Duration::from_millis(16));
    assert_eq!(game.elapsed(), elapsed + Duration::from_millis(16));
}

fn game() -> Game {
    Game::new(Mode::FortyLines, 42)
}

#[rstest]
#[case(Mode::FortyLines, Spin::Mini)]
#[case(Mode::Practice, Spin::Mini)]
#[case(Mode::Blitz, Spin::None)]
fn immobile_t_without_three_corners_is_a_mini_only_with_all_mini_plus(
    #[case] mode: Mode,
    #[case] spin: Spin,
) {
    let mut game = Game::new(mode, 42);
    game.active = ActivePiece {
        kind: Piece::T,
        rotation: 1,
        x: 3,
        y: 37,
    };
    // The side blockers stop translations while only one of the four T corners is occupied.
    for (x, y) in [(2, 38), (6, 38), (4, 36), (3, 39)] {
        game.board[y][x] = Some(Piece::J);
    }
    assert!(game.fits(game.active));
    game.apply(Action::CounterClockwise);
    assert_eq!(game.active.rotation, 0);
    game.apply(Action::HardDrop);
    assert_eq!(game.last_score().unwrap().spin, spin);
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
        y: 36,
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
        y: 38,
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
fn fifteenth_grounded_move_locks_immediately() {
    let mut game = game();
    game.active = ActivePiece {
        kind: Piece::O,
        rotation: 0,
        x: 3,
        y: 38,
    };
    game.lowest_y = game.active.y;
    for index in 0..14 {
        game.advance(Duration::from_millis(400));
        game.apply(if index % 2 == 0 {
            Action::Left
        } else {
            Action::Right
        });
    }
    assert!(game.board.iter().flatten().all(Option::is_none));
    assert_eq!(game.lock_resets, 14);
    game.apply(Action::Right);
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
#[case(false)]
#[case(true)]
fn falling_to_a_new_low_restores_the_lock_budget(#[case] soft_drop: bool) {
    let mut game = game();
    game.active = ActivePiece {
        kind: Piece::O,
        rotation: 0,
        x: 3,
        y: 30,
    };
    game.lowest_y = game.active.y;
    game.board[32][4] = Some(Piece::J);
    game.lock_resets = 14;
    game.lock_started = true;
    game.grounded_elapsed = Duration::from_millis(400);

    // Leaving the ledge spends the final move, but cannot lock a piece in midair.
    game.apply(Action::Right);
    assert_eq!(game.lock_resets, 15);
    assert!(!game.grounded());
    if soft_drop {
        game.apply(Action::SoftDrop);
    } else {
        game.gravity_elapsed = game
            .gravity_interval()
            .checked_sub(Duration::from_millis(10))
            .unwrap();
        game.advance(Duration::from_millis(10));
    }
    assert_eq!(game.active.y, 31);
    assert_eq!(game.lowest_y, 31);
    assert_eq!(game.lock_resets, 0);
    assert_eq!(game.grounded_elapsed, Duration::ZERO);

    // Descending restores the budget, but does not erase the earlier surface contact.
    game.apply(Action::Right);
    assert_eq!(game.lock_resets, 1);
}

#[test]
fn returning_to_the_same_height_with_no_moves_left_locks_immediately() {
    let mut game = game();
    game.active = ActivePiece {
        kind: Piece::O,
        rotation: 0,
        x: 3,
        y: 30,
    };
    game.lowest_y = game.active.y;
    game.board[32][4] = Some(Piece::J);
    game.lock_started = true;
    game.lock_resets = 14;
    game.apply(Action::Right);
    assert_eq!(game.lock_resets, 15);
    assert_eq!(game.board[30][4], None);
    game.apply(Action::Left);
    assert_eq!(game.board[30][4], Some(Piece::O));
    assert_eq!(game.board[31][5], Some(Piece::O));
}

#[test]
fn falling_back_to_an_old_low_does_not_restore_moves_or_time() {
    let mut game = game();
    game.active = ActivePiece {
        kind: Piece::O,
        rotation: 0,
        x: 3,
        y: 31,
    };
    game.lowest_y = 32;
    game.board[34][4] = Some(Piece::J);
    game.lock_started = true;
    game.lock_resets = 14;
    game.grounded_elapsed = Duration::from_millis(400);
    game.apply(Action::SoftDrop);
    assert_eq!(game.active.y, 32);
    assert_eq!(game.lock_resets, 14);
    assert_eq!(game.grounded_elapsed, Duration::from_millis(400));
    game.advance(Duration::from_millis(100));
    assert_eq!(game.board[32][4], Some(Piece::O));
}

#[test]
fn floor_kicks_spend_the_lock_budget_but_blocked_moves_do_not() {
    let mut game = game();
    game.active = ActivePiece {
        kind: Piece::T,
        rotation: 0,
        x: 0,
        y: 38,
    };
    game.lowest_y = game.active.y;
    game.lock_resets = 14;
    game.apply(Action::Left);
    assert_eq!(game.lock_resets, 14);
    assert!(game.board.iter().flatten().all(Option::is_none));
    game.apply(Action::Clockwise);
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
fn leaving_a_surface_starts_counting_airborne_moves_without_a_clock_tick() {
    let mut game = game();
    game.active = ActivePiece {
        kind: Piece::O,
        rotation: 0,
        x: 3,
        y: 30,
    };
    game.lowest_y = game.active.y;
    game.board[32][4] = Some(Piece::J);
    game.apply(Action::Right);
    assert!(!game.grounded());
    assert_eq!(game.lock_resets, 1);
    game.apply(Action::Right);
    assert_eq!(game.lock_resets, 2);
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

#[rstest]
#[case(0, Action::Clockwise, &[(5, 25)], (1, 0))]
#[case(0, Action::CounterClockwise, &[(3, 27), (4, 25), (6, 25)], (2, 1))]
#[case(1, Action::CounterClockwise, &[(2, 26), (6, 26)], (-1, 2))]
#[case(3, Action::CounterClockwise, &[(1, 27), (5, 27)], (1, -2))]
fn srs_plus_i_spins_choose_the_published_kick_order(
    #[case] rotation: u8,
    #[case] action: Action,
    #[case] blocks: &[(usize, usize)],
    #[case] offset: (i32, i32),
) {
    let mut game = game();
    game.active = ActivePiece {
        kind: Piece::I,
        rotation,
        x: 3,
        y: 25,
    };
    for &(x, y) in blocks {
        game.board[y][x] = Some(Piece::J);
    }
    assert!(game.fits(game.active));
    game.apply(action);
    assert_eq!(
        (game.active.x, game.active.y),
        (3 + offset.0, 25 + offset.1)
    );
    assert_eq!(
        game.active.rotation,
        (rotation + if action == Action::Clockwise { 1 } else { 3 }) % 4
    );
    assert!(game.fits(game.active));
}

#[test]
fn half_turn_kicks_off_floor_and_keeps_o_stable() {
    let mut game = game();
    game.active = ActivePiece {
        kind: Piece::T,
        rotation: 0,
        x: 3,
        y: 38,
    };
    game.apply(Action::Rotate180);
    assert_eq!(game.active.rotation, 2);
    assert_eq!(game.active.y, 37);
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
    game.board[HIDDEN - 1] = [Some(Piece::O); WIDTH];
    game.spawn(Piece::T, false);
    assert_eq!(game.status(), Status::TopOut);
}

#[rstest]
fn spawn_descends_only_when_the_next_row_is_free(
    #[values(Piece::I, Piece::O, Piece::T, Piece::S, Piece::Z, Piece::J, Piece::L)] kind: Piece,
    #[values(false, true)] blocked: bool,
    #[values(false, true)] from_hold: bool,
) {
    let mut game = game();
    if blocked {
        game.board[HIDDEN][4] = Some(Piece::J);
    }
    if from_hold {
        game.held = Some(kind);
        game.apply(Action::Hold);
    } else {
        game.spawn(kind, false);
    }
    let initial = ActivePiece::spawn(kind);
    assert_eq!(game.status(), Status::Playing);
    assert_eq!(game.active.kind, kind);
    assert_eq!(game.active.y, initial.y + i32::from(!blocked));
    assert!(
        initial
            .cells()
            .iter()
            .all(|&(_, y)| y < i32::try_from(HIDDEN).unwrap())
    );
    assert_eq!(game.lowest_y, game.active.y);
    assert_eq!(game.lock_resets, 0);
}

#[rstest]
#[case(false)]
#[case(true)]
fn locking_above_the_skyline_requires_a_visible_cell(#[case] fully_hidden: bool) {
    let mut game = game();
    let hidden = i32::try_from(HIDDEN).unwrap();
    game.active = ActivePiece {
        kind: Piece::I,
        rotation: 1,
        x: -2,
        y: hidden - if fully_hidden { 4 } else { 3 },
    };
    let cells = game.active.cells();
    game.board[if fully_hidden { HIDDEN } else { HIDDEN + 1 }][0] = Some(Piece::J);
    game.apply(Action::HardDrop);
    for (x, y) in cells {
        assert_eq!(
            game.board[usize::try_from(y).unwrap()][usize::try_from(x).unwrap()],
            Some(Piece::I)
        );
    }
    assert_eq!(
        game.status(),
        if fully_hidden {
            Status::TopOut
        } else {
            Status::Playing
        }
    );
}

#[test]
fn the_buffer_has_a_ceiling() {
    let game = game();
    assert!(!game.fits(ActivePiece {
        kind: Piece::O,
        rotation: 0,
        x: 3,
        y: -1
    }));
    assert_eq!(game.board().len(), 40);
    assert_eq!(VISIBLE_HEIGHT, 20);
}

#[rstest]
#[case(false)]
#[case(true)]
fn only_a_line_clear_rescues_the_next_blocked_spawn(#[case] clear: bool) {
    let mut game = game();
    game.board[39] = [Some(Piece::J); WIDTH];
    game.board[39][0] = None;
    game.board[39][1] = None;
    if !clear {
        game.board[39][2] = None;
    }
    game.board[if clear { 18 } else { 19 }][4] = Some(Piece::J);
    game.active = ActivePiece {
        kind: Piece::O,
        rotation: 0,
        x: -1,
        y: 38,
    };
    game.queue.clear();
    game.queue.push_back(Piece::T);
    game.apply(Action::HardDrop);
    assert_eq!(game.lines(), u32::from(clear));
    assert_eq!(
        game.status(),
        if clear {
            Status::Playing
        } else {
            Status::TopOut
        }
    );
    assert_eq!(game.active.y, if clear { 17 } else { 18 });
    if clear {
        assert!(game.fits(game.active));
        assert_eq!(game.lowest_y, 17);
        assert_eq!(game.lock_resets, 0);
        // The previous clear must not rescue a later hold into the same blocked spawn.
        game.held = Some(Piece::T);
        game.apply(Action::Hold);
        assert_eq!(game.status(), Status::TopOut);
    }
}

#[test]
fn a_line_clear_forgives_locking_entirely_above_the_skyline() {
    let mut game = game();
    game.board[18] = [Some(Piece::J); WIDTH];
    game.board[18][0..4].fill(None);
    game.board[19][0] = Some(Piece::J);
    game.active = ActivePiece {
        kind: Piece::I,
        rotation: 0,
        x: 0,
        y: 17,
    };
    game.queue.clear();
    game.queue.push_back(Piece::T);
    game.apply(Action::HardDrop);
    assert_eq!(game.lines(), 1);
    assert_eq!(game.status(), Status::Playing);
    assert!(game.fits(game.active));
}

#[rstest]
#[case(Piece::I, Status::Playing, -1)]
#[case(Piece::T, Status::TopOut, 0)]
fn clutch_clear_cannot_push_cells_above_the_buffer(
    #[case] next: Piece,
    #[case] status: Status,
    #[case] spawn_y: i32,
) {
    let mut game = game();
    for row in game.board.iter_mut().take(HIDDEN) {
        row[3..7].fill(Some(Piece::J));
    }
    game.board[39] = [Some(Piece::J); WIDTH];
    game.board[39][0..2].fill(None);
    game.active = ActivePiece {
        kind: Piece::O,
        rotation: 0,
        x: -1,
        y: 38,
    };
    game.queue.clear();
    game.queue.push_back(next);
    game.apply(Action::HardDrop);
    assert_eq!(game.lines(), 1);
    assert_eq!(game.status(), status);
    assert_eq!(game.active.y, spawn_y);
    assert!(game.active.cells().iter().all(|&(_, y)| y >= 0));
    if status == Status::Playing {
        assert!(game.fits(game.active));
    }
}

#[test]
fn blitz_finishes_at_exactly_two_minutes() {
    let mut game = Game::new(Mode::Blitz, 42);
    game.elapsed = Duration::from_millis(119_999);
    game.advance(Duration::from_secs(1));
    assert_eq!(game.status(), Status::Complete);
    assert_eq!(game.elapsed(), Duration::from_secs(120));
}

#[test]
fn forty_lines_finishes_after_fortieth_line() {
    let mut game = Game::new(Mode::FortyLines, 42);
    game.lines = 39;
    game.board[39] = [Some(Piece::J); WIDTH];
    game.board[39][4] = None;
    game.active = ActivePiece {
        kind: Piece::I,
        rotation: 1,
        x: 2,
        y: 36,
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
        y: 37,
    };
    for (x, y) in [(3, 37), (3, 39)] {
        game.board[y][x] = Some(Piece::J);
    }
    let third_corner = if spin == Spin::Full { (5, 39) } else { (5, 37) };
    game.board[third_corner.1][third_corner.0] = Some(Piece::J);
    if lines >= 1 {
        game.board[38] = [Some(Piece::J); WIDTH];
        for x in 3..=5 {
            game.board[38][x] = None;
        }
    }
    if lines >= 2 {
        game.board[39] = [Some(Piece::J); WIDTH];
        game.board[39][4] = None;
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
    for y in 36..=38 {
        game.board[y] = [Some(Piece::J); WIDTH];
        game.board[y][4] = None;
    }
    game.board[37][5] = None;
    game.board[39][4] = Some(Piece::J);
    game.board[34][4] = Some(Piece::J);
    if open_front {
        game.board[38][5] = None;
    }
    game.active = ActivePiece {
        kind: Piece::T,
        rotation: 0,
        x: 4,
        y: 34,
    };
    assert!(game.fits(game.active));
    game.apply(Action::Clockwise);
    assert_eq!(
        game.active,
        ActivePiece {
            kind: Piece::T,
            rotation: 1,
            x: 3,
            y: 36
        }
    );
    game.apply(Action::HardDrop);
    assert_eq!(game.lines(), lines);
    assert_eq!(game.last_score().unwrap().spin, Spin::Full);
    assert_eq!(game.score(), points);
}

#[test]
fn spin_uses_the_level_before_crossing_a_blitz_goal() {
    let mut game = spin_slot(Spin::Full, 2);
    game.mode = Mode::Blitz;
    game.lines = 2;
    game.apply(Action::HardDrop);
    assert_eq!(game.level(), 2);
    assert_eq!(game.score(), 1200);
}

#[test]
fn fourth_srs_kick_keeps_a_mini_double_as_mini() {
    let mut game = game();
    for y in 36..=38 {
        game.board[y] = [Some(Piece::J); WIDTH];
        game.board[y][4] = None;
    }
    game.board[37][5] = None;
    game.board[38][5] = None;
    game.board[39][4] = Some(Piece::J);
    game.board[34][3] = Some(Piece::J);
    game.board[34][4] = Some(Piece::J);
    game.active = ActivePiece {
        kind: Piece::T,
        rotation: 2,
        x: 3,
        y: 34,
    };
    assert!(game.fits(game.active));
    game.apply(Action::CounterClockwise);
    assert_eq!(
        game.active,
        ActivePiece {
            kind: Piece::T,
            rotation: 1,
            x: 3,
            y: 36
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
    wall.board[38][1] = Some(Piece::J);
    wall.active = ActivePiece {
        kind: Piece::T,
        rotation: 0,
        x: 0,
        y: 36,
    };
    wall.apply(Action::Clockwise);
    assert_eq!(wall.active.x, -1);
    wall.apply(Action::HardDrop);
    assert_eq!(wall.last_score().unwrap().spin, Spin::Mini);
    assert_eq!(wall.score(), 100);

    let mut floor = game();
    floor.board[38][3] = Some(Piece::J);
    floor.board[38][6] = Some(Piece::J);
    floor.active = ActivePiece {
        kind: Piece::T,
        rotation: 1,
        x: 3,
        y: 37,
    };
    floor.apply(Action::CounterClockwise);
    assert_eq!(floor.active.y, 38);
    floor.apply(Action::HardDrop);
    assert_eq!(floor.last_score().unwrap().spin, Spin::Mini);
    assert_eq!(floor.score(), 100);
}

#[test]
fn fewer_than_three_corners_does_not_score_a_spin() {
    let mut game = spin_slot(Spin::Full, 0);
    game.board[37][3] = None;
    game.apply(Action::HardDrop);
    assert_eq!(game.last_score().unwrap().spin, Spin::None);
    assert_eq!(game.score(), 0);
}

#[test]
fn locks_keep_scoring_chains_and_exclude_drop_points_from_b2b() {
    let mut game = Game::new(Mode::FortyLines, 42);
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
            y: 35,
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
            y: 38,
        },
        2 => ActivePiece {
            kind: Piece::O,
            rotation: 0,
            x: 3,
            y: 38,
        },
        3 => ActivePiece {
            kind: Piece::T,
            rotation: 1,
            x: 3,
            y: 37,
        },
        _ => ActivePiece {
            kind: Piece::I,
            rotation: 1,
            x: 2,
            y: 36,
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
#[case(1, 3600)]
#[case(2, 3800)]
#[case(3, 4000)]
#[case(4, 4300)]
fn perfect_clear_awards_bonus_at_level_before_line_removal(
    #[case] lines: usize,
    #[case] total: u64,
) {
    let mut game = perfect_clear_slot(lines);
    game.mode = Mode::Blitz;
    game.lines = 14;
    game.apply(Action::HardDrop);
    assert!(game.board.iter().flatten().all(Option::is_none));
    assert!(game.last_score().unwrap().perfect_clear);
    assert_eq!(game.score(), total * 3);
    assert_eq!(game.level(), 4);
}

#[test]
fn hidden_or_visible_remaining_blocks_prevent_perfect_clear() {
    for row in [0, HIDDEN + 8] {
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
fn b2b_perfect_clear_adds_3500_and_combo_without_multiplying_the_bonus() {
    let mut game = perfect_clear_slot(4);
    game.apply(Action::HardDrop);
    assert_eq!(game.score(), 4300);
    let next = perfect_clear_slot(4);
    game.board = next.board;
    game.active = next.active;
    game.apply(Action::HardDrop);
    let event = game.last_score().unwrap();
    assert!(event.perfect_clear);
    assert!(event.back_to_back);
    assert_eq!(event.combo, Some(1));
    assert_eq!(event.points, 1200 + 3500 + 50);
    assert_eq!(game.score(), 9050);
}

#[test]
fn moving_down_after_rotation_cancels_spin_and_spawn_does_not_inherit_it() {
    for hard_drop in [false, true] {
        let mut falling = game();
        for (x, y) in [(3, 37), (3, 39), (5, 39)] {
            falling.board[y][x] = Some(Piece::J);
        }
        falling.active = ActivePiece {
            kind: Piece::T,
            rotation: 0,
            x: 3,
            y: 35,
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
    spawned.spawn(Piece::T, false);
    spawned.active = ActivePiece {
        kind: Piece::T,
        rotation: 2,
        x: 3,
        y: 37,
    };
    spawned.apply(Action::HardDrop);
    assert_eq!(spawned.last_score().unwrap().spin, Spin::None);
}

proptest! {
    #[test]
    fn every_seven_bag_contains_each_piece(seed in any::<u64>()) {
        let mut game = Game::new(Mode::FortyLines, seed);
        for _ in 0..10 {
            let mut bag = Vec::new();
            for _ in 0..7 { bag.push(game.active.kind); game.spawn_next(false); }
            for piece in Piece::ALL { prop_assert_eq!(bag.iter().filter(|&&actual| actual == piece).count(), 1); }
        }
    }

    #[test]
    fn generated_moves_preserve_board_and_ghost_invariants(seed in any::<u64>(), actions in prop::collection::vec(0_u8..9, 0..200)) {
        let mut game = Game::new(Mode::FortyLines, seed);
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
