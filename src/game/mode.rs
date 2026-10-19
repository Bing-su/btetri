use std::fmt;
use std::str::FromStr;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Mode {
    #[default]
    FortyLines,
    Blitz,
    Marathon,
    Practice,
}

impl Mode {
    pub const ALL: [Self; 4] = [
        Self::FortyLines,
        Self::Blitz,
        Self::Marathon,
        Self::Practice,
    ];

    pub fn cycle(self, delta: i32) -> Self {
        let index = match self {
            Self::FortyLines => 0,
            Self::Blitz => 1,
            Self::Marathon => 2,
            Self::Practice => 3,
        };
        match (index + delta.rem_euclid(4)).rem_euclid(4) {
            0 => Self::FortyLines,
            1 => Self::Blitz,
            2 => Self::Marathon,
            _ => Self::Practice,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::FortyLines => "40 LINES",
            Self::Blitz => "BLITZ",
            Self::Marathon => "MARATHON",
            Self::Practice => "PRACTICE",
        }
    }

    pub(super) fn level(self, mut lines: u32) -> u32 {
        if self == Self::Marathon {
            return (1 + lines / 10).min(20);
        }
        if self != Self::Blitz {
            return 1;
        }
        let mut level: u32 = 1;
        // Each Blitz goal is ceil(2.1 * level), e.g. 3, 5, 7 lines for the first levels.
        while lines >= (21 * level).div_ceil(10) {
            lines -= (21 * level).div_ceil(10);
            level += 1;
        }
        level
    }

    pub(super) fn gravity_seconds(self, level: u32) -> f64 {
        if self == Self::Marathon {
            let exponent = f64::from(level - 1);
            return (0.8 - exponent * 0.007).powf(exponent).max(0.001);
        }
        if self != Self::Blitz {
            return 1.0 / (60.0 * 0.02);
        }
        // Clamp at 20G before the formula's base can turn negative at extreme levels.
        if level >= 14 {
            return 1.0 / 1200.0;
        }
        let exponent = f64::from(level - 1);
        (0.65 - exponent * 0.007).powf(exponent).max(1.0 / 1200.0)
    }
}

impl fmt::Display for Mode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::FortyLines => "40l",
            Self::Blitz => "blitz",
            Self::Marathon => "marathon",
            Self::Practice => "practice",
        })
    }
}

impl FromStr for Mode {
    type Err = InvalidMode;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "40l" => Ok(Self::FortyLines),
            "blitz" => Ok(Self::Blitz),
            "marathon" => Ok(Self::Marathon),
            "practice" => Ok(Self::Practice),
            _ => Err(InvalidMode),
        }
    }
}

/// A mode name was rejected; for example, use `40l` rather than the retired `sprint`.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
#[error("expected 40l, blitz, marathon, or practice")]
pub struct InvalidMode;
