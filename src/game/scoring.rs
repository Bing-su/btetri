/// Spin classification used by the scoring rules.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Spin {
    /// No qualifying rotation-based spin.
    None,
    /// A mini T-spin or qualifying immobile non-T piece.
    Mini,
    /// A full T-spin.
    Full,
}

/// The latest lock's award, excluding soft/hard drop points.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ScoreEvent {
    /// Spin classification of the locked piece.
    pub spin: Spin,
    /// Number of rows removed by this lock.
    pub lines: u32,
    /// Total lock award after level scaling and bonuses, excluding drop points.
    pub points: u64,
    /// Zero on the first consecutive clear; None when no lines were cleared.
    pub combo: Option<u32>,
    /// Whether this difficult clear received the consecutive difficult-clear multiplier.
    pub back_to_back: bool,
    /// Whether this line clear left the entire board empty.
    pub perfect_clear: bool,
}

/// Scoring history needed to carry combos and difficult-clear chains between locks.
#[derive(Default)]
pub(super) struct Scoring {
    back_to_back: bool,
    /// Most recent lock, including empty locks that break combos.
    pub(super) last: Option<ScoreEvent>,
}

impl Scoring {
    /// Award one lock and update its chains; empty locks preserve back-to-back but reset combos.
    /// Perfect-clear points are added before level scaling and never receive the B2B multiplier.
    pub(super) fn award(
        &mut self,
        spin: Spin,
        lines: u32,
        level: u32,
        perfect_clear: bool,
    ) -> ScoreEvent {
        let base = match (spin, lines) {
            (Spin::None, 1) | (Spin::Mini, 0) => 100,
            (Spin::None, 2) => 300,
            (Spin::None, 3) => 500,
            (Spin::None, 4) | (Spin::Full, 1) | (Spin::Mini, 3) => 800,
            (Spin::Mini, 1) => 200,
            (Spin::Mini, 2) | (Spin::Full, 0) => 400,
            (Spin::Full, 2) => 1200,
            (Spin::Full, 3) | (Spin::Mini, 4) => 1600,
            _ => 0,
        };
        let difficult = lines > 0 && (lines == 4 || spin != Spin::None);
        let back_to_back = difficult && self.back_to_back;
        // Empty locks preserve B2B, but every ordinary line clear breaks it.
        if lines > 0 {
            self.back_to_back = difficult;
        }
        let combo = (lines > 0).then(|| {
            self.last
                .and_then(|event| event.combo)
                .map_or(0, |count| count + 1)
        });
        let clear_points = if back_to_back { base * 3 / 2 } else { base };
        let perfect_clear = perfect_clear && lines > 0;
        // Add PC separately: its bonus is level-scaled, but never multiplied by B2B.
        let perfect_points = if perfect_clear { 3500 } else { 0 };
        let event = ScoreEvent {
            spin,
            lines,
            points: (clear_points + perfect_points + 50 * u64::from(combo.unwrap_or_default()))
                * u64::from(level),
            combo,
            back_to_back,
            perfect_clear,
        };
        self.last = Some(event);
        event
    }
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;

    #[rstest]
    #[case(Spin::None, 0, 0)]
    #[case(Spin::None, 1, 100)]
    #[case(Spin::None, 2, 300)]
    #[case(Spin::None, 3, 500)]
    #[case(Spin::None, 4, 800)]
    #[case(Spin::Mini, 0, 100)]
    #[case(Spin::Mini, 1, 200)]
    #[case(Spin::Mini, 2, 400)]
    #[case(Spin::Mini, 3, 800)]
    #[case(Spin::Mini, 4, 1600)]
    #[case(Spin::Full, 0, 400)]
    #[case(Spin::Full, 1, 800)]
    #[case(Spin::Full, 2, 1200)]
    #[case(Spin::Full, 3, 1600)]
    fn awards_base_points_at_current_level(
        #[case] spin: Spin,
        #[case] lines: u32,
        #[case] base: u64,
    ) {
        assert_eq!(
            Scoring::default().award(spin, lines, 3, false).points,
            base * 3
        );
    }

    #[test]
    fn b2b_survives_empty_locks_and_mixes_tetrises_with_both_spins() {
        let mut scoring = Scoring::default();
        assert_eq!(scoring.award(Spin::None, 4, 1, false).points, 800);
        assert_eq!(scoring.award(Spin::Full, 0, 1, false).points, 400);
        assert_eq!(scoring.award(Spin::None, 0, 1, false).points, 0);
        assert_eq!(scoring.award(Spin::Full, 2, 1, false).points, 1800);
        let mini = scoring.award(Spin::Mini, 1, 1, false);
        assert!(mini.back_to_back);
        assert_eq!(mini.points, 350);
        // Ordinary clears break B2B; the next difficult clear starts a fresh chain.
        assert_eq!(scoring.award(Spin::None, 1, 1, false).points, 200);
        let tetris = scoring.award(Spin::None, 4, 1, false);
        assert!(!tetris.back_to_back);
        assert_eq!(tetris.points, 950);
    }

    #[test]
    fn combo_starts_at_zero_and_empty_spin_resets_it_without_multiplying_bonus() {
        let mut scoring = Scoring::default();
        assert_eq!(scoring.award(Spin::None, 4, 2, false).combo, Some(0));
        let second = scoring.award(Spin::None, 4, 2, false);
        assert_eq!(second.combo, Some(1));
        assert_eq!(second.points, (1200 + 50) * 2);
        assert_eq!(scoring.award(Spin::Mini, 0, 2, false).combo, None);
        let next = scoring.award(Spin::None, 4, 2, false);
        assert_eq!(next.combo, Some(0));
        assert_eq!(next.points, 2400);
    }
}
