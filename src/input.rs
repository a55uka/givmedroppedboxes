//! Key handling for every screen, plus the lazy directory loading that
//! follows the tree cursor.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::app::App;
use crate::model::path_key;
use crate::run::Screen;

impl App {
    pub(crate) fn on_key(&mut self, key: KeyEvent) {
        match self.screen {
            Screen::Link => self.on_key_link(key),
            Screen::Browser => self.on_key_browser(key),
            Screen::Progress => self.on_key_progress(key),
            Screen::Summary => self.on_key_summary(key),
        }
    }

    // ----- link screen -----

    fn on_key_link(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => self.request_quit(),
            KeyCode::Enter => self.submit_link(),
            KeyCode::Backspace => {
                self.link.text.pop();
            }
            KeyCode::Char(c)
                if key.modifiers.is_empty() || key.modifiers == KeyModifiers::SHIFT =>
            {
                self.link.text.push(c);
            }
            _ => {}
        }
    }

    pub(crate) fn on_paste(&mut self, text: &str) {
        if self.screen == Screen::Link {
            let cleaned: String = text.chars().filter(|c| !c.is_control()).collect();
            self.link.text.push_str(&cleaned);
        }
    }

    // ----- browser screen -----

    fn on_key_browser(&mut self, key: KeyEvent) {
        self.browser.status = None;
        match key.code {
            KeyCode::Char('q' | 'Q') => self.request_quit(),
            KeyCode::Char('d' | 'D') => self.start_download(),
            KeyCode::Char('r' | 'R') => self.reload_selected(),
            KeyCode::Char(' ') => {
                let path = self.browser.tree.selected().to_vec();
                self.model.lock().unwrap().toggle_sel(&path);
                self.maybe_load_selected();
            }
            KeyCode::Char('a' | 'A') => self.model.lock().unwrap().select_all(true),
            KeyCode::Char('n' | 'N') => self.model.lock().unwrap().select_all(false),
            KeyCode::Char('j') | KeyCode::Down => {
                self.browser.tree.key_down();
                self.maybe_load_selected();
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.browser.tree.key_up();
                self.maybe_load_selected();
            }
            KeyCode::Char('h') | KeyCode::Left => {
                self.browser.tree.key_left();
                self.maybe_load_selected();
            }
            KeyCode::Char('l') | KeyCode::Right | KeyCode::Enter => {
                self.browser.tree.key_right();
                self.maybe_load_selected();
            }
            KeyCode::Char('g') => {
                self.browser.tree.select_first();
                self.maybe_load_selected();
            }
            KeyCode::Char('G') => {
                self.browser.tree.select_last();
                self.maybe_load_selected();
            }
            _ => {}
        }
    }

    /// If the cursor sits on an opened directory whose children were never
    /// listed, kick off a listing.
    fn maybe_load_selected(&mut self) {
        let path = self.browser.tree.selected().to_vec();
        let should = {
            let model = self.model.lock().unwrap();
            let key = path_key(&path);
            self.browser.tree.opened().contains(&path)
                && !model.loading.contains(&key)
                && !model.errors.contains_key(&key)
                && model
                    .node_at(&path)
                    .is_some_and(|n| n.is_dir() && n.children.is_none())
        };
        if should {
            self.start_load(path);
        }
    }

    // ----- progress / summary screens -----

    fn on_key_progress(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char('c' | 'C') | KeyCode::Esc => {
                self.cancel.cancel();
                self.screen = Screen::Browser;
                self.browser.status =
                    Some("download cancelled — partial files were removed".to_string());
            }
            KeyCode::Char('q' | 'Q') => self.request_quit(),
            _ => {}
        }
    }

    fn on_key_summary(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char('r' | 'R') => self.retry_failed(),
            KeyCode::Char('b' | 'B') | KeyCode::Enter | KeyCode::Esc => {
                self.screen = Screen::Browser;
            }
            KeyCode::Char('q' | 'Q') => self.request_quit(),
            _ => {}
        }
    }
}
