use std::path::PathBuf;

use usage_rs::{Args, Cli, Subcommands};

use crate::game::Mode;

/// Play a local, keyboard-controlled terminal game.
#[derive(Debug, Cli)]
#[usage(bin = "btetri", version = env!("CARGO_PKG_VERSION"))]
pub struct Options {
    /// Store local records and settings in this database file.
    #[usage(long, global)]
    pub db: Option<PathBuf>,
    #[usage(subcommand)]
    pub command: Option<Command>,
}

#[derive(Debug, Subcommands)]
pub enum Command {
    /// Start a mode immediately, skipping the menu.
    Play(Play),
    /// Browse the local, per-mode records.
    Records(Records),
}

#[derive(Debug, Args)]
pub struct Play {
    /// 40l (40 lines), blitz (2 minutes), marathon (survival), or practice (unlimited).
    pub mode: Mode,
    /// Display name used in the local records.
    #[usage(short = 'n', long)]
    pub name: Option<String>,
}

#[derive(Debug, Args)]
pub struct Records {
    /// 40l, blitz, or marathon; defaults to 40l.
    pub mode: Option<Mode>,
}

#[cfg(test)]
mod tests {
    use std::ffi::OsStr;

    use super::*;

    #[test]
    fn direct_play_and_global_paths_are_parsed() {
        let args = ["play", "40l", "--name", "Alice", "--db", "my games.db"];
        let args: Vec<_> = args.iter().map(OsStr::new).collect();
        let parsed = Options::parse_from(&args).unwrap();
        let Some(Command::Play(play)) = parsed.command else {
            panic!("expected play")
        };
        assert_eq!(play.mode, Mode::FortyLines);
        assert_eq!(play.name.as_deref(), Some("Alice"));
        assert_eq!(parsed.db, Some(PathBuf::from("my games.db")));
    }

    #[test]
    fn marathon_play_and_records_are_parsed() {
        let parsed = Options::parse_from(&[OsStr::new("play"), OsStr::new("marathon")]).unwrap();
        assert!(matches!(
            parsed.command,
            Some(Command::Play(Play {
                mode: Mode::Marathon,
                ..
            }))
        ));
        let parsed = Options::parse_from(&[OsStr::new("records"), OsStr::new("marathon")]).unwrap();
        assert!(matches!(
            parsed.command,
            Some(Command::Records(Records {
                mode: Some(Mode::Marathon)
            }))
        ));
    }

    #[test]
    fn rejects_unknown_modes_and_bad_levels() {
        Options::parse_from(&[OsStr::new("--config"), OsStr::new("old.toml")]).unwrap_err();
        Options::parse_from(&[OsStr::new("play"), OsStr::new("unknown")]).unwrap_err();
        Options::parse_from(&[
            OsStr::new("play"),
            OsStr::new("blitz"),
            OsStr::new("--level"),
            OsStr::new("no"),
        ])
        .unwrap_err();
    }
}
