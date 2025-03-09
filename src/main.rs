use ansi_term::{Colour::Fixed, Style};
use fuzzy_matcher::FuzzyMatcher;
use owo_colors::OwoColorize;
use zellij_tile::prelude::*;

use fuzzy_matcher::skim::SkimMatcherV2;
use std::collections::BTreeMap;
use std::fs::{read_to_string, File};
use std::io::{self, BufRead};
use std::path::{Path, PathBuf};
// use sprintf::sprintf;use std::fs::read_to_string;

#[derive(Default)]
struct State {
    error: Option<String>,
    current_session: Option<String>,
    /// Used to preserve the current directory when switching to a new session.
    current_dir: Option<PathBuf>,
    replace_current_session: bool,
    input: String,
    input_cusror_index: usize,
    userspace_configuration: BTreeMap<String, String>,
    layout: Vec<String>,
    layout_match: Option<String>,
    fz_matcher: SkimMatcherV2,
}

// impl Default for State {
//     fn default() -> Self {
//         Self {
//             error: None,
//             userspace_configuration: BTreeMap::default(),
//             input: String::default(),
//             input_cusror_index: 0,
//             layout: Vec::default(),
//             layout_match: None,
//             fz_matcher: SkimMatcherV2::default(),
//         }
//     }
// }

impl State {
    fn handle_key_event(&mut self, key: KeyWithModifier) -> bool {
        let mut should_render = true;
        match key.bare_key {
            BareKey::Enter => {
                if self.open_tab_layout() {
                    should_render = false;
                    hide_self();
                } else {
                    should_render = true;
                }
            }
            BareKey::Backspace => {
                if self.remove_input_at_index() {
                    // update fuzzy find result
                    self.fuzzy_find_layout();
                }
                should_render = true;
            }
            BareKey::Left => {
                if self.input_cusror_index > 0 {
                    self.input_cusror_index -= 1;
                }
                should_render = true;
            }
            BareKey::Right => {
                if self.input_cusror_index < self.input.len() {
                    self.input_cusror_index += 1;
                }
                should_render = true;
            }
            BareKey::Esc => {
                self.close();
                should_render = true;
            }
            BareKey::Char('c') if key.has_modifiers(&[KeyModifier::Ctrl]) => {
                self.close();
                should_render = true;
            }
            BareKey::Char(c) => {
                if self.insert_input_at_index(c) {
                    self.fuzzy_find_layout();
                }
                should_render = true;
            }
            _ => (),
        }
        should_render
    }
    /// close current plugins and its hepler pane
    fn close(&self) {
        close_plugin_pane(get_plugin_ids().plugin_id);
    }

    fn fuzzy_find_layout(&mut self) {
        let mut best_score = 0;

        // reset match
        self.layout_match = None;
        for l in self.layout.iter() {
            if let Some(score) = self.fz_matcher.fuzzy_match(l, &self.input) {
                if score > best_score {
                    best_score = score;
                    self.layout_match = Some(l.to_string());
                }
            }
        }
    }

    fn open_tab_layout(&mut self) -> bool {
        if let Some(selected_path) = &self.layout_match {
            // NOTE: /host/ relative path for loading layout
            let mut layout_path = "/host/".to_owned();
            layout_path.push_str(selected_path);

            // Create a path to the desired file
            let path = Path::new(&layout_path);

            // Open the path in read-only mode, returns `io::Result<File>`
            let layout_opt = match read_to_string(path) {
                Err(err) => {
                    self.error = Some(err.to_string());
                    None
                }
                Ok(content) => Some(content),
            };

            if let Some(layout) = layout_opt {
                // default
                // if not in_palce just apply new tabs
                // in current session
                if !self.replace_current_session {
                    new_tabs_with_layout(&layout);
                    return true;
                }

                // Use the saved current directory, because right now there is no built-in way to
                // auto-preserve it when switching a session.
                let current_dir = self.current_dir.clone();

                // when in_place configure apply layout to current session
                if let Some(current) = self.current_session.clone() {
                    // rename current
                    rename_session("zellij_wp_delete_me");

                    // race condition bug ??
                    // even after renaming current session
                    // plugin crash if switch_session_with_layout()
                    // uses the same current name
                    let mut new_name = "zwp:".to_owned();
                    new_name.push_str(&current);
                    // re-create a new session with the same name
                    // but apply the layout
                    switch_session_with_layout(
                        Some(&new_name),
                        LayoutInfo::Stringified(layout),
                        current_dir,
                    );

                    // clean up old session
                    kill_sessions(&["zellij_wp_delete_me"]);
                    delete_dead_session("zellij_wp_delete_me");
                } else {
                    switch_session_with_layout(None, LayoutInfo::Stringified(layout), current_dir);
                }

                return true;
            }
        }

        false
        // new_tabs_with_layout(layout);
    }

    /// remove_input_at_index  removes char at the
    /// cursor index and update input.
    /// Returns true if the input has change
    fn remove_input_at_index(&mut self) -> bool {
        if self.input.is_empty() {
            self.input.pop();
        } else if self.input_cusror_index > 0 && self.input_cusror_index <= self.input.len() {
            self.input.remove(self.input_cusror_index - 1);
            // update cursor index
            self.input_cusror_index -= 1;

            return true;
        } else if self.input_cusror_index == 0 {
            self.input.remove(0);
        }
        false
    }

    /// remove_input_at_index  removes char at the
    /// cursor index and update input.
    /// Returns true if the input has change
    fn insert_input_at_index(&mut self, c: char) -> bool {
        if self.input.is_empty() {
            self.input.push(c);

            // update cursor index
            self.input_cusror_index += 1;
        } else if self.input_cusror_index > 0 && self.input_cusror_index <= self.input.len() {
            self.input.insert(self.input_cusror_index, c);
            // update cursor index
            self.input_cusror_index += 1;

            return true;
        } else if self.input_cusror_index == 0 {
            self.input.insert(0, c);
            self.input_cusror_index += 1;
        }
        false
    }

    /// print the input prompt
    /// prints 1 line
    fn print_prompt(&self, _rows: usize, _cols: usize) {
        let prompt = " > ".cyan().bold().to_string();
        if self.input.is_empty() {
            println!(
                "{} {}{}",
                prompt,
                "┃".bold().white(),
                "Fuzzy find command".dimmed().italic(),
            );
        } else {
            self.print_non_empty_input_prompt(prompt);
        }
    }

    fn print_non_empty_input_prompt(&self, prompt: String) {
        match self.input_cusror_index.cmp(&self.input.len()) {
            std::cmp::Ordering::Equal => {
                println!("{} {}{}", prompt, self.input.dimmed(), "┃".bold().white(),);
            }
            std::cmp::Ordering::Less => {
                let copy = self.input.clone();
                let (before_curs, after_curs) = copy.split_at(self.input_cusror_index);

                println!(
                    "{} {}{}{}",
                    prompt,
                    before_curs.dimmed(),
                    "┃".bold().white(),
                    after_curs.dimmed()
                );
            }

            std::cmp::Ordering::Greater => (),
        }
    }
}

register_plugin!(State);

impl ZellijPlugin for State {
    fn load(&mut self, configuration: BTreeMap<String, String>) {
        self.userspace_configuration = configuration;

        // Permission
        // - ReadApplicationState => for Tab and Pane update
        // - RunCommands => to run floating command terminal
        // - ChangeApplicationState => rename plugin pane, close managed paned
        request_permission(&[
            PermissionType::ReadApplicationState,
            // PermissionType::RunCommands,
            PermissionType::ChangeApplicationState,
        ]);
        subscribe(&[
            EventType::ModeUpdate,
            EventType::TabUpdate,
            EventType::PaneUpdate,
            EventType::Key,
            EventType::SessionUpdate,
        ]);

        // File .zellij-workspace must exist in the current path (zellij cwd dir is mounted as /host)
        // NOTE: /host is the cwd of where the zellij session started
        //       and not the current cwd of the pane itself
        let filename = "/host/.zellij-workspace".to_owned();
        if let Ok(lines) = read_lines(filename) {
            // Consumes the iterator, returns an (Optional) String
            for layout in lines.map_while(Result::ok) {
                // ignore commented lines starting with '#'
                // or empty line
                if !layout.trim_start().starts_with('#') && !layout.trim_start().is_empty() {
                    self.layout.push(layout);
                }
            }
        }

        self.replace_current_session = self
            .userspace_configuration
            .get("replace_current_session")
            .map(|v| v == "true")
            .unwrap_or(false);

        self.current_dir = Some(get_plugin_ids().initial_cwd);

        rename_plugin_pane(get_plugin_ids().plugin_id, "WorkspaceManager");
    }

    fn update(&mut self, event: Event) -> bool {
        let mut should_render = false;

        // NOTE: uncomment if multiple event match
        match event {
            Event::Key(key) => {
                should_render = self.handle_key_event(key);
            }
            Event::SessionUpdate(session_infos, _) => {
                if let Some(current) = session_infos
                    .into_iter()
                    .find(|session_info| session_info.is_current_session)
                {
                    self.current_session = Some(current.name);
                }
            }
            _ => (),
        };

        // // suppress warning single match
        // if let Event::Key(key) = event {
        //     should_render = self.handle_key_event(key);
        // }

        should_render
    }

    fn render(&mut self, rows: usize, cols: usize) {
        let debug = self.userspace_configuration.get("debug");

        let mut count = 3;
        if let Some(err) = &self.error {
            println!("Error: {}", color_bold(RED, err))
        } else {
            println!();
        }

        count += 1;

        // prompt view

        self.print_prompt(rows, cols);
        count += 1;

        // layout fuzzy finder
        if let Some(m) = &self.layout_match {
            println!();
            println!(" $ {}", m);
            println!();

            count += 3;
        } else {
            println!();
            println!("-> {}", "Selected layout".dimmed());
            println!();
            count += 3;
        }
        println!(" Available layouts: ");

        count += 1;
        for l in self.layout.iter() {
            if self.fz_matcher.fuzzy_match(l, &self.input).is_some() {
                // limits display of layout
                // based on available rows in pane
                // with arbitrary buffer for safety
                if count >= rows - 4 {
                    println!(" - {}", "...".dimmed());
                    break;
                }
                println!(" - {}", l.dimmed());
                count += 1;
            }
        }

        // Key binding view
        println!();
        println!(
            "  <{}> <{}> Close Plugin",
            color_bold(WHITE, "Esc"),
            color_bold(WHITE, "Ctrl+c"),
        );

        if debug.is_some_and(|x| x == "true") {
            println!("input: {}", self.input);

            println!("Cursor: {}", self.input_cusror_index);
            println!("len: {}", self.input.len());
            println!(
                "session: {}",
                self.current_session.clone().unwrap_or("None".to_owned())
            );

            println!(
                "{} {:#?}",
                color_bold(GREEN, "Runtime configuration:"),
                self.userspace_configuration
            );
        }
    }
}

pub const CYAN: u8 = 51;
pub const GRAY_LIGHT: u8 = 238;
pub const GRAY_DARK: u8 = 245;
pub const WHITE: u8 = 15;
pub const BLACK: u8 = 16;
pub const RED: u8 = 124;
pub const GREEN: u8 = 154;
pub const ORANGE: u8 = 166;

fn color_bold(color: u8, text: &str) -> String {
    format!("{}", Style::new().fg(Fixed(color)).bold().paint(text))
}

// src: https://doc.rust-lang.org/rust-by-example/std_misc/file/read_lines.html
// The output is wrapped in a Result to allow matching on errors
// Returns an Iterator to the Reader of the lines of the file.
fn read_lines<P>(filename: P) -> io::Result<io::Lines<io::BufReader<File>>>
where
    P: AsRef<Path>,
{
    let file = File::open(filename)?;
    Ok(io::BufReader::new(file).lines())
}
