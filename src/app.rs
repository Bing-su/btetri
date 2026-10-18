use std::time::Duration;

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use miette::IntoDiagnostic;

use crate::config::{Config, Keys, validate_player};
use crate::game::{Action, Game, Mode, Rules, Status};
use crate::storage::{Record, Store};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Screen {
    Menu,
    Playing,
    Help(HelpReturn),
    Paused,
    GameOver,
    Records,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HelpReturn {
    Playing,
    Paused,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MenuItem {
    Player,
    Mode,
    StartLevel,
    Start,
    Records,
    Quit,
}

impl MenuItem {
    pub fn label(self) -> &'static str {
        match self {
            Self::Player => "PLAYER",
            Self::Mode => "MODE",
            Self::StartLevel => "START LEVEL",
            Self::Start => "START GAME",
            Self::Records => "RECORDS",
            Self::Quit => "QUIT",
        }
    }

    fn next(self) -> Self {
        match self {
            Self::Player => Self::Mode,
            Self::Mode => Self::StartLevel,
            Self::StartLevel => Self::Start,
            Self::Start => Self::Records,
            Self::Records => Self::Quit,
            Self::Quit => Self::Player,
        }
    }

    fn previous(self) -> Self {
        match self {
            Self::Player => Self::Quit,
            Self::Mode => Self::Player,
            Self::StartLevel => Self::Mode,
            Self::Start => Self::StartLevel,
            Self::Records => Self::Start,
            Self::Quit => Self::Records,
        }
    }
}

#[derive(Debug, Default)]
pub enum SaveState {
    #[default]
    None,
    Pending,
    Saved,
    Failed(String),
}

impl From<Result<(), String>> for SaveState {
    fn from(result: Result<(), String>) -> Self {
        match result {
            Ok(()) => Self::Saved,
            Err(error) => Self::Failed(error),
        }
    }
}

// Keep rows and errors inside their state so a new request cannot display stale data.
#[derive(Debug)]
pub enum RecordsState {
    Requested,
    Loaded(Vec<Record>),
    Failed(String),
}

impl RecordsState {
    pub fn rows(&self) -> &[Record] {
        match self {
            Self::Loaded(rows) => rows,
            _ => &[],
        }
    }
}

pub struct App {
    pub(crate) config: Config,
    pub(crate) screen: Screen,
    pub(crate) menu_selection: MenuItem,
    pub(crate) dialog_selection: usize,
    pub(crate) name_draft: Option<String>,
    pub(crate) game: Option<Game>,
    pub(crate) record: Option<Record>,
    pub(crate) save: SaveState,
    pub(crate) records: RecordsState,
    pub(crate) record_mode: Mode,
    pub(crate) record_selection: usize,
    records_back: Screen,
    pub(crate) notice: Option<String>,
    pub(crate) quit: bool,
    pub(crate) keyboard_releases: bool,
    soft_drop_keys: Vec<KeyCode>,
    pressed_actions: Vec<KeyCode>,
}

impl TryFrom<Config> for App {
    type Error = miette::Report;

    fn try_from(config: Config) -> Result<Self, Self::Error> {
        config
            .validate()
            .map_err(|error| miette::miette!("{error}"))?;
        Ok(Self {
            config,
            screen: Screen::Menu,
            menu_selection: MenuItem::Start,
            dialog_selection: 0,
            name_draft: None,
            game: None,
            record: None,
            save: SaveState::None,
            records: RecordsState::Loaded(Vec::new()),
            record_mode: Mode::Marathon,
            record_selection: 0,
            records_back: Screen::Menu,
            notice: None,
            quit: false,
            keyboard_releases: false,
            soft_drop_keys: Vec::new(),
            pressed_actions: Vec::new(),
        })
    }
}

impl App {
    pub fn start(&mut self) -> miette::Result<()> {
        self.clear_held_keys();
        self.pressed_actions.clear();
        let rules = Rules {
            start_level: self.config.start_level,
            max_level: self.config.max_level,
            end_on_max_level: self.config.end_on_max_level,
        };
        self.game = Some(Game::new(self.config.mode, rules, rand::random()).into_diagnostic()?);
        self.record = None;
        self.save = SaveState::None;
        self.screen = Screen::Playing;
        self.dialog_selection = 0;
        self.notice = None;
        Ok(())
    }

    pub fn open_records(&mut self, mode: Mode) {
        self.records_back = if self.screen == Screen::GameOver {
            Screen::GameOver
        } else {
            Screen::Menu
        };
        self.screen = Screen::Records;
        self.record_mode = mode;
        self.request_records();
    }

    fn request_records(&mut self) {
        self.record_selection = 0;
        self.records = RecordsState::Requested;
    }

    pub fn process_storage(&mut self, store: &Store) {
        if matches!(self.save, SaveState::Pending)
            && let Some(record) = &self.record
        {
            self.save = store
                .save(record)
                .map_err(|error| format!("{error:#}"))
                .into();
        }
        // Complete the synchronous query here, e.g. mode changes cannot leave stale rows visible.
        if matches!(self.records, RecordsState::Requested) {
            self.records = match store.records(self.record_mode) {
                Ok(records) => {
                    self.record_selection = self
                        .record
                        .as_ref()
                        .and_then(|current| records.iter().position(|row| row.id == current.id))
                        .unwrap_or_default();
                    RecordsState::Loaded(records)
                }
                Err(error) => RecordsState::Failed(format!("{error:#}")),
            };
        }
    }

    pub fn finish_session(
        &mut self,
        store: &Store,
        result: miette::Result<()>,
    ) -> miette::Result<()> {
        // Preserve a result created just before a terminal failure, e.g. by the final hard drop.
        self.process_storage(store);
        if let SaveState::Failed(error) = &self.save {
            // Keep both failures so a disconnected terminal cannot hide the unsaved score.
            let unsaved = format!("{error}\nUnsaved result: {:?}", self.record);
            return match result {
                Ok(()) => Err(miette::miette!("{unsaved}")),
                Err(error) => Err(error.wrap_err(unsaved)),
            };
        }
        result
    }

    pub fn advance(&mut self, duration: Duration) -> miette::Result<()> {
        if self.screen == Screen::Playing {
            if let Some(game) = &mut self.game {
                game.advance(duration);
            }
            self.finish_if_needed()?;
        }
        Ok(())
    }

    fn finish_if_needed(&mut self) -> miette::Result<()> {
        if let Some(game) = &self.game
            && game.status() != Status::Playing
            && self.record.is_none()
        {
            self.record = Some(Record::from_game(game, &self.config.player)?);
            self.save = SaveState::Pending;
            self.screen = Screen::GameOver;
            self.dialog_selection = 0;
        }
        Ok(())
    }

    pub fn pause_for_resize(&mut self) {
        self.clear_held_keys();
        self.pressed_actions.clear();
        match self.screen {
            Screen::Playing => {
                self.screen = Screen::Paused;
                self.dialog_selection = 0;
            }
            // Closing help after a resize must still require an explicit resume.
            Screen::Help(_) => {
                self.screen = Screen::Help(HelpReturn::Paused);
                self.dialog_selection = 0;
            }
            _ => {}
        }
    }

    pub fn key(&mut self, key: KeyEvent) -> miette::Result<()> {
        if key.kind == KeyEventKind::Release {
            let released = match key.code {
                KeyCode::Char(c) => KeyCode::Char(c.to_ascii_lowercase()),
                code => code,
            };
            self.pressed_actions.retain(|&code| code != released);
            self.soft_drop_keys.retain(|&code| match (code, key.code) {
                (KeyCode::Char(held), KeyCode::Char(released)) => {
                    !held.eq_ignore_ascii_case(&released)
                }
                _ => code != key.code,
            });
            if let Some(game) = &mut self.game {
                game.set_soft_drop(!self.soft_drop_keys.is_empty());
            }
            return Ok(());
        }
        match self.screen {
            Screen::Menu => self.menu_key(key)?,
            Screen::Playing => self.playing_key(key)?,
            Screen::Help(return_to) => {
                if key.code == KeyCode::Esc || Keys::matches(&self.config.keys.help, key) {
                    self.screen = match return_to {
                        HelpReturn::Playing => Screen::Playing,
                        HelpReturn::Paused => Screen::Paused,
                    };
                }
            }
            Screen::Paused => self.pause_key(key),
            Screen::GameOver => self.result_key(key)?,
            Screen::Records => self.records_key(key),
        }
        Ok(())
    }

    fn menu_key(&mut self, key: KeyEvent) -> miette::Result<()> {
        if self.name_draft.is_some() {
            self.edit_name(key);
            return Ok(());
        }
        match key.code {
            KeyCode::Up => self.menu_selection = self.menu_selection.previous(),
            KeyCode::Down | KeyCode::Tab => self.menu_selection = self.menu_selection.next(),
            KeyCode::Left => self.change_setting(-1),
            KeyCode::Right => self.change_setting(1),
            KeyCode::Esc => self.quit = true,
            KeyCode::Enter => match self.menu_selection {
                MenuItem::Player => self.name_draft = Some(self.config.player.clone()),
                MenuItem::Mode | MenuItem::StartLevel => self.change_setting(1),
                MenuItem::Start => self.start()?,
                MenuItem::Records => self.open_records(self.config.mode),
                MenuItem::Quit => self.quit = true,
            },
            _ => {}
        }
        Ok(())
    }

    fn edit_name(&mut self, key: KeyEvent) {
        if key.code == KeyCode::Esc {
            self.name_draft = None;
            self.notice = None;
            return;
        }
        if key.code == KeyCode::Enter {
            let Some(draft) = &self.name_draft else {
                return;
            };
            if let Err(error) = validate_player(draft) {
                self.notice = Some(error);
                return;
            }
            self.config.player = draft.trim().to_owned();
            self.name_draft = None;
            self.notice = None;
            self.menu_selection = MenuItem::Mode;
        } else if let Some(draft) = &mut self.name_draft {
            match key.code {
                KeyCode::Backspace => {
                    draft.pop();
                }
                KeyCode::Delete => draft.clear(),
                KeyCode::Char(c)
                    if !c.is_control()
                        && !key
                            .modifiers
                            .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
                        && draft.chars().count() < 16 =>
                {
                    draft.push(c)
                }
                _ => {}
            }
        }
    }

    fn change_setting(&mut self, delta: i32) {
        match self.menu_selection {
            MenuItem::Mode => self.config.mode = self.config.mode.cycle(delta),
            MenuItem::StartLevel => {
                self.config.start_level = self
                    .config
                    .start_level
                    .saturating_add_signed(delta)
                    .clamp(1, self.config.max_level)
            }
            _ => {}
        }
    }

    fn playing_key(&mut self, key: KeyEvent) -> miette::Result<()> {
        if Keys::matches(&self.config.keys.pause, key) {
            self.clear_held_keys();
            self.screen = Screen::Paused;
            self.dialog_selection = 0;
            return Ok(());
        }
        if Keys::matches(&self.config.keys.help, key) {
            self.clear_held_keys();
            self.screen = Screen::Help(HelpReturn::Playing);
            return Ok(());
        }
        let Some(action) = self.config.keys.action(key) else {
            return Ok(());
        };
        if key.kind == KeyEventKind::Repeat
            && !matches!(action, Action::Left | Action::Right | Action::SoftDrop)
        {
            return Ok(());
        }
        if self.keyboard_releases
            && !matches!(action, Action::Left | Action::Right | Action::SoftDrop)
        {
            let code = match key.code {
                KeyCode::Char(c) => KeyCode::Char(c.to_ascii_lowercase()),
                code => code,
            };
            // Windows reports auto-repeat as Press too; e.g. held Space must lock only once.
            if self.pressed_actions.contains(&code) {
                return Ok(());
            }
            self.pressed_actions.push(code);
        }
        let Some(game) = &mut self.game else {
            return Ok(());
        };
        if action == Action::SoftDrop && self.keyboard_releases {
            if key.kind != KeyEventKind::Press || self.soft_drop_keys.contains(&key.code) {
                return Ok(());
            }
            self.soft_drop_keys.push(key.code);
            if !game.soft_drop() {
                game.apply(action);
                game.set_soft_drop(true);
            }
        } else {
            game.apply(action);
        }
        self.finish_if_needed()
    }

    fn clear_held_keys(&mut self) {
        // Focus loss or a dialog can swallow releases, e.g. Alt-Tab while holding Down.
        self.soft_drop_keys.clear();
        if let Some(game) = &mut self.game {
            game.set_soft_drop(false);
        }
    }

    fn pause_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Up | KeyCode::Down => self.dialog_selection = 1 - self.dialog_selection,
            KeyCode::Esc => self.screen = Screen::Playing,
            KeyCode::Enter if self.dialog_selection == 0 => self.screen = Screen::Playing,
            KeyCode::Enter => self.return_to_menu(),
            _ if Keys::matches(&self.config.keys.pause, key) => self.screen = Screen::Playing,
            _ => {}
        }
    }

    fn result_key(&mut self, key: KeyEvent) -> miette::Result<()> {
        if key.code == KeyCode::Char('r') && matches!(self.save, SaveState::Failed(_)) {
            self.save = SaveState::Pending;
            self.notice = None;
            return Ok(());
        }
        match key.code {
            KeyCode::Up => self.dialog_selection = (self.dialog_selection + 2) % 3,
            KeyCode::Down => self.dialog_selection = (self.dialog_selection + 1) % 3,
            KeyCode::Enter | KeyCode::Esc if !matches!(self.save, SaveState::Saved) => {
                self.notice =
                    Some("Keep this result open until saved. R retries a failed save.".into());
            }
            KeyCode::Esc => self.return_to_menu(),
            KeyCode::Enter => match self.dialog_selection {
                0 => self.start()?,
                1 => self.open_records(self.config.mode),
                _ => self.return_to_menu(),
            },
            _ => {}
        }
        Ok(())
    }

    fn return_to_menu(&mut self) {
        self.screen = Screen::Menu;
        self.menu_selection = MenuItem::Start;
        self.game = None;
        self.record = None;
        self.save = SaveState::None;
        self.notice = None;
    }

    fn records_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => self.screen = self.records_back,
            KeyCode::Left => {
                self.record_mode = self.record_mode.cycle(-1);
                self.request_records();
            }
            KeyCode::Right => {
                self.record_mode = self.record_mode.cycle(1);
                self.request_records();
            }
            KeyCode::Up => self.record_selection = self.record_selection.saturating_sub(1),
            KeyCode::Down => {
                self.record_selection =
                    (self.record_selection + 1).min(self.records.rows().len().saturating_sub(1))
            }
            KeyCode::PageUp => self.record_selection = self.record_selection.saturating_sub(10),
            KeyCode::PageDown => {
                self.record_selection =
                    (self.record_selection + 10).min(self.records.rows().len().saturating_sub(1))
            }
            KeyCode::Char('r') if matches!(self.records, RecordsState::Failed(_)) => {
                self.request_records()
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    #[rstest::rstest]
    #[case(false)]
    #[case(true)]
    fn ctrl_c_holds_without_quitting(#[case] keyboard_releases: bool) {
        let mut app = App::try_from(Config::default()).unwrap();
        app.keyboard_releases = keyboard_releases;
        app.start().unwrap();
        let initial_piece = app.game.as_ref().unwrap().active().kind;

        // Ctrl rotation must allow a following C hold without exiting the game.
        app.key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL))
            .unwrap();

        assert!(!app.quit);
        assert_eq!(app.screen, Screen::Playing);
        assert_eq!(app.game.as_ref().unwrap().held(), Some(initial_piece));
    }

    #[test]
    fn held_soft_drop_stops_only_after_all_bindings_release() {
        let config = Config::parse("[keys]\nsoft_drop = ['down', 's']", "test").unwrap();
        let mut app = App::try_from(config).unwrap();
        app.keyboard_releases = true;
        app.start().unwrap();
        let initial_y = app.game.as_ref().unwrap().active().y;
        app.key(key(KeyCode::Down)).unwrap();
        app.key(KeyEvent::new_with_kind(
            KeyCode::Down,
            KeyModifiers::NONE,
            KeyEventKind::Repeat,
        ))
        .unwrap();
        assert_eq!(app.game.as_ref().unwrap().active().y, initial_y + 1);
        app.key(key(KeyCode::Char('s'))).unwrap();
        app.key(KeyEvent::new_with_kind(
            KeyCode::Down,
            KeyModifiers::CONTROL,
            KeyEventKind::Release,
        ))
        .unwrap();
        assert!(app.game.as_ref().unwrap().soft_drop());
        app.advance(Duration::from_millis(50)).unwrap();
        assert_eq!(app.game.as_ref().unwrap().active().y, initial_y + 2);
        app.key(KeyEvent::new_with_kind(
            KeyCode::Char('S'),
            KeyModifiers::SHIFT,
            KeyEventKind::Release,
        ))
        .unwrap();
        assert!(!app.game.as_ref().unwrap().soft_drop());
    }

    #[test]
    fn legacy_soft_drop_steps_on_press_and_repeat_without_latching() {
        let mut app = App::try_from(Config::default()).unwrap();
        app.start().unwrap();
        let initial_y = app.game.as_ref().unwrap().active().y;
        for kind in [KeyEventKind::Press, KeyEventKind::Repeat] {
            app.key(KeyEvent::new_with_kind(
                KeyCode::Down,
                KeyModifiers::NONE,
                kind,
            ))
            .unwrap();
        }
        let game = app.game.as_ref().unwrap();
        assert_eq!(game.active().y, initial_y + 2);
        assert!(!game.soft_drop());
    }

    #[test]
    fn repeated_press_does_not_hard_drop_again_until_release() {
        let mut app = App::try_from(Config::default()).unwrap();
        app.keyboard_releases = true;
        app.start().unwrap();
        app.key(key(KeyCode::Char(' '))).unwrap();
        let board = *app.game.as_ref().unwrap().board();
        let active = app.game.as_ref().unwrap().active();
        app.key(key(KeyCode::Char(' '))).unwrap();
        assert_eq!(app.game.as_ref().unwrap().board(), &board);
        assert_eq!(app.game.as_ref().unwrap().active(), active);
        app.key(KeyEvent::new_with_kind(
            KeyCode::Char(' '),
            KeyModifiers::NONE,
            KeyEventKind::Release,
        ))
        .unwrap();
        app.key(key(KeyCode::Char(' '))).unwrap();
        assert_ne!(app.game.as_ref().unwrap().board(), &board);
    }

    #[rstest::rstest]
    #[case(Some(KeyCode::Esc))]
    #[case(Some(KeyCode::Char('?')))]
    #[case(None)]
    fn leaving_play_clears_held_soft_drop(#[case] dialog: Option<KeyCode>) {
        let mut app = App::try_from(Config::default()).unwrap();
        app.keyboard_releases = true;
        app.start().unwrap();
        app.key(key(KeyCode::Down)).unwrap();
        assert!(app.game.as_ref().unwrap().soft_drop());
        if let Some(code) = dialog {
            app.key(key(code)).unwrap();
        } else {
            app.pause_for_resize();
        }
        assert!(!app.game.as_ref().unwrap().soft_drop());
        assert!(app.soft_drop_keys.is_empty());
        app.key(key(KeyCode::Esc)).unwrap();
        app.key(key(KeyCode::Down)).unwrap();
        assert!(app.game.as_ref().unwrap().soft_drop());
    }

    #[rstest::rstest]
    #[case(KeyCode::Up)]
    #[case(KeyCode::Char('z'))]
    #[case(KeyCode::Char('a'))]
    #[case(KeyCode::Char(' '))]
    #[case(KeyCode::Char('c'))]
    fn one_shot_actions_ignore_reported_repeats(#[case] code: KeyCode) {
        let mut app = App::try_from(Config::default()).unwrap();
        app.start().unwrap();
        let before = app.game.as_ref().unwrap().active();
        app.key(KeyEvent::new_with_kind(
            code,
            KeyModifiers::NONE,
            KeyEventKind::Repeat,
        ))
        .unwrap();
        assert_eq!(app.game.as_ref().unwrap().active(), before);
        assert_eq!(app.game.as_ref().unwrap().held(), None);
        assert!(
            app.game
                .as_ref()
                .unwrap()
                .board()
                .iter()
                .flatten()
                .all(Option::is_none)
        );
    }

    #[rstest::rstest]
    #[case(KeyCode::Down)]
    #[case(KeyCode::Tab)]
    fn menu_navigation_wraps_and_up_reverses_it(#[case] forward: KeyCode) {
        let mut app = App::try_from(Config::default()).unwrap();
        for expected in [
            MenuItem::Records,
            MenuItem::Quit,
            MenuItem::Player,
            MenuItem::Mode,
            MenuItem::StartLevel,
            MenuItem::Start,
        ] {
            let previous = app.menu_selection;
            app.key(key(forward)).unwrap();
            assert_eq!(app.menu_selection, expected);
            app.key(key(KeyCode::Up)).unwrap();
            assert_eq!(app.menu_selection, previous);
            app.key(key(forward)).unwrap();
        }
    }

    #[test]
    fn menu_settings_and_commands_follow_the_selected_item() {
        let mut app = App::try_from(Config::default()).unwrap();
        app.menu_selection = MenuItem::Player;
        app.key(key(KeyCode::Enter)).unwrap();
        app.name_draft = Some(" Alice ".into());
        app.key(key(KeyCode::Enter)).unwrap();
        assert_eq!(app.config.player, "Alice");
        assert_eq!(app.menu_selection, MenuItem::Mode);

        app.key(key(KeyCode::Right)).unwrap();
        assert_eq!(app.config.mode, Mode::Sprint);
        app.key(key(KeyCode::Left)).unwrap();
        assert_eq!(app.config.mode, Mode::Marathon);
        app.key(key(KeyCode::Down)).unwrap();
        app.key(key(KeyCode::Left)).unwrap();
        assert_eq!(app.config.start_level, 1);
        app.key(key(KeyCode::Enter)).unwrap();
        assert_eq!(app.config.start_level, 2);
        app.key(key(KeyCode::Down)).unwrap();
        app.key(key(KeyCode::Enter)).unwrap();
        assert_eq!(app.screen, Screen::Playing);
        assert_eq!(app.game.as_ref().unwrap().start_level(), 2);

        app.key(key(KeyCode::Esc)).unwrap();
        app.key(key(KeyCode::Down)).unwrap();
        app.key(key(KeyCode::Enter)).unwrap();
        assert_eq!(app.screen, Screen::Menu);
        assert_eq!(app.menu_selection, MenuItem::Start);
        app.key(key(KeyCode::Down)).unwrap();
        app.key(key(KeyCode::Enter)).unwrap();
        assert_eq!(app.screen, Screen::Records);
        assert_eq!(app.record_mode, Mode::Marathon);
        assert!(matches!(app.records, RecordsState::Requested));
        app.key(key(KeyCode::Esc)).unwrap();
        app.key(key(KeyCode::Down)).unwrap();
        app.key(key(KeyCode::Enter)).unwrap();
        assert!(app.quit);
    }

    #[test]
    fn pause_freezes_time_and_resume_keeps_board() {
        let mut app = App::try_from(Config::default()).unwrap();
        app.start().unwrap();
        app.advance(Duration::from_millis(200)).unwrap();
        let before = app.game.as_ref().unwrap().active();
        app.key(key(KeyCode::Esc)).unwrap();
        app.advance(Duration::from_secs(20)).unwrap();
        assert_eq!(
            app.game.as_ref().unwrap().elapsed(),
            Duration::from_millis(200)
        );
        app.key(key(KeyCode::Enter)).unwrap();
        assert_eq!(app.game.as_ref().unwrap().active(), before);
        assert_eq!(app.screen, Screen::Playing);
    }

    #[test]
    fn finished_game_cannot_be_replaced_before_save() {
        let mut app = App::try_from(Config::default()).unwrap();
        app.config.mode = Mode::Ultra;
        app.start().unwrap();
        app.advance(Duration::from_secs(120)).unwrap();
        assert_eq!(app.screen, Screen::GameOver);
        let id = app.record.as_ref().unwrap().id.clone();
        app.key(key(KeyCode::Enter)).unwrap();
        assert_eq!(app.record.as_ref().unwrap().id, id);
        app.save = Err("disk full".into()).into();
        app.key(key(KeyCode::Char('r'))).unwrap();
        assert!(matches!(app.save, SaveState::Pending));
        assert_eq!(app.record.as_ref().unwrap().id, id);
        app.save = Ok(()).into();
        app.key(key(KeyCode::Enter)).unwrap();
        assert_eq!(app.screen, Screen::Playing);
    }

    #[test]
    fn unicode_name_edit_can_be_cancelled() {
        let mut app = App::try_from(Config::default()).unwrap();
        app.menu_selection = MenuItem::Player;
        app.key(key(KeyCode::Enter)).unwrap();
        app.key(key(KeyCode::Delete)).unwrap();
        app.key(key(KeyCode::Char('김'))).unwrap();
        app.key(key(KeyCode::Backspace)).unwrap();
        app.key(key(KeyCode::Esc)).unwrap();
        assert_eq!(app.config.player, "PLAYER");
    }

    #[test]
    fn help_freezes_game_and_resize_requires_explicit_resume() {
        let mut app = App::try_from(Config::default()).unwrap();
        app.start().unwrap();
        app.advance(Duration::from_millis(200)).unwrap();
        app.key(key(KeyCode::Char('?'))).unwrap();
        app.advance(Duration::from_secs(10)).unwrap();
        assert_eq!(
            app.game.as_ref().unwrap().elapsed(),
            Duration::from_millis(200)
        );
        app.key(key(KeyCode::Esc)).unwrap();
        assert_eq!(app.screen, Screen::Playing);
        app.key(key(KeyCode::Char('?'))).unwrap();
        app.pause_for_resize();
        app.key(key(KeyCode::Esc)).unwrap();
        assert_eq!(app.screen, Screen::Paused);
        app.key(key(KeyCode::Enter)).unwrap();
        assert_eq!(app.screen, Screen::Playing);
    }

    #[test]
    fn records_load_once_and_mode_changes_replace_old_rows() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("records.db");
        let store = Store::open(&path).unwrap();
        let db = rusqlite::Connection::open(path).unwrap();
        let mut app = App::try_from(Config::default()).unwrap();
        app.start().unwrap();
        let row = Record::from_game(app.game.as_ref().unwrap(), "Alice").unwrap();
        store.save(&row).unwrap();
        app.open_records(Mode::Marathon);
        app.process_storage(&store);
        assert_eq!(app.records.rows(), std::slice::from_ref(&row));
        // A later frame must not query again, e.g. after another connection changes the table.
        db.execute("DROP TABLE game_records", []).unwrap();
        app.process_storage(&store);
        assert_eq!(app.records.rows(), [row]);
        app.key(key(KeyCode::Right)).unwrap();
        assert!(app.records.rows().is_empty());
        app.process_storage(&store);
        assert!(matches!(app.records, RecordsState::Failed(_)));
    }

    #[test]
    fn records_retry_clears_failure_and_selects_the_current_result() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("records.db");
        let store = Store::open(&path).unwrap();
        let db = rusqlite::Connection::open(path).unwrap();
        let mut app = App::try_from(Config::default()).unwrap();
        app.config.mode = Mode::Ultra;
        app.start().unwrap();
        app.advance(Duration::from_secs(120)).unwrap();
        let current = app.record.clone().unwrap();
        app.process_storage(&store);
        let higher = Record {
            id: "higher".into(),
            score: current.score + 1,
            ..current.clone()
        };
        store.save(&higher).unwrap();
        // Hide the table temporarily to exercise the real query failure and retry path.
        db.execute("ALTER TABLE game_records RENAME TO unavailable", [])
            .unwrap();
        app.open_records(Mode::Ultra);
        app.process_storage(&store);
        assert!(matches!(app.records, RecordsState::Failed(_)));
        db.execute("ALTER TABLE unavailable RENAME TO game_records", [])
            .unwrap();
        app.key(key(KeyCode::Char('r'))).unwrap();
        assert!(matches!(app.records, RecordsState::Requested));
        assert!(app.records.rows().is_empty());
        app.process_storage(&store);
        assert_eq!(app.record_selection, 1);
        assert_eq!(app.records.rows().get(app.record_selection), Some(&current));
        app.key(key(KeyCode::Esc)).unwrap();
        assert_eq!(app.screen, Screen::GameOver);
    }
}
