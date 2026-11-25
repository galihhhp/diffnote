use crate::comments::{CommentStore, Side};
use crate::config::{Config, SendMode};
use crate::diff::{DiffDoc, LineKind};
use crate::git::{self, FileEntry, Section, Snapshot};
use crate::transport::{self, Candidate, Target};
use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum View {
    Review,
    Git,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Mode {
    Normal,
    PendingG,
    CommentInput,
    CommitInput,
    TargetPicker,
    Help,
    FilePicker,
    Confirm,
    History,
    DiffSearch,
    CommentPanel,
    CommandOutput,
    BranchInput,
    BasePicker,
    RenameInput,
}

type PendingComment = (String, Section, Side, usize, String, String);

fn subsequence(query: &str, text: &str) -> bool {
    let mut chars = text.chars();
    query
        .chars()
        .all(|needle| chars.any(|candidate| candidate == needle))
}

pub struct App {
    pub repo: PathBuf,
    pub config: Config,
    pub snapshot: Snapshot,
    pub docs: HashMap<String, DiffDoc>,
    pub hashes: HashMap<String, String>,
    pub view: View,
    pub mode: Mode,
    pub selected: usize,
    pub cursor: usize,
    pub selected_target: usize,
    pub comments: CommentStore,
    pub candidates: Vec<Candidate>,
    pub target: Option<Target>,
    pub input: String,
    pub status: String,
    pub tree_token: String,
    pub last_tree_check: std::time::Instant,
    pub split: bool,
    pub show_comments: bool,
    pub should_quit: bool,
    pub confirm_action: Option<String>,
    pub pending_comment: Option<PendingComment>,
    pub pending_send: bool,
    pub file_query: String,
    pub file_matches: Vec<usize>,
    pub file_pick: usize,
    pub edit_marker: Option<std::path::PathBuf>,
    pub editing_id: Option<u64>,
    pub history: Vec<git::CommitEntry>,
    pub history_pick: usize,
    pub history_diff: Option<crate::diff::DiffDoc>,
    pub history_scroll: usize,
    pub branch_state: git::BranchState,
    pub command_title: String,
    pub command_output: String,
    pub branch_input: String,
    pub h_scroll: u16,
    pub base_branches: Vec<String>,
    pub base_pick: usize,
    pub rename_input: String,
    pub search_query: String,
    pub search_hits: Vec<usize>,
    pub search_pick: usize,
    pub comment_pick: usize,
}

pub fn doc_key(entry: &FileEntry) -> String {
    format!("{}:{}", entry.path, entry.section.tag())
}

fn key_to_string(key: KeyEvent) -> String {
    match key.code {
        KeyCode::Char(character) => {
            if key.modifiers.contains(KeyModifiers::CONTROL) {
                format!("C-{}", character)
            } else {
                character.to_string()
            }
        }
        KeyCode::Enter => {
            if key.modifiers.contains(KeyModifiers::SHIFT) {
                "S-Enter".to_string()
            } else {
                "Enter".to_string()
            }
        }
        KeyCode::Esc => "Esc".to_string(),
        KeyCode::Tab => "Tab".to_string(),
        KeyCode::Backspace => "Backspace".to_string(),
        KeyCode::Up => "Up".to_string(),
        KeyCode::Down => "Down".to_string(),
        KeyCode::Left => "Left".to_string(),
        KeyCode::Right => "Right".to_string(),
        _ => String::new(),
    }
}

impl App {
    pub fn new(repo: &Path, session: Option<String>, block: Option<String>) -> Result<App> {
        let root = git::root_in(repo)?;
        let comments = CommentStore::load(&root);
        let (config, warning) = Config::load();
        let split = config.split;
        let show_comments = config.show_comments;
        let mut app = App {
            repo: root,
            config,
            snapshot: Snapshot {
                files: Vec::new(),
                head: String::new(),
            },
            docs: HashMap::new(),
            hashes: HashMap::new(),
            view: View::Git,
            mode: Mode::Normal,
            selected: 0,
            cursor: 0,
            selected_target: 0,
            comments,
            candidates: Vec::new(),
            target: None,
            input: String::new(),
            status: String::new(),
            tree_token: String::new(),
            last_tree_check: std::time::Instant::now(),
            split,
            show_comments,
            should_quit: false,
            confirm_action: None,
            pending_comment: None,
            pending_send: false,
            file_query: String::new(),
            file_matches: Vec::new(),
            file_pick: 0,
            edit_marker: None,
            editing_id: None,
            history: Vec::new(),
            history_pick: 0,
            history_diff: None,
            history_scroll: 0,
            branch_state: git::BranchState::default(),
            command_title: String::new(),
            command_output: String::new(),
            branch_input: String::new(),
            h_scroll: 0,
            base_branches: Vec::new(),
            base_pick: 0,
            rename_input: String::new(),
            search_query: String::new(),
            search_hits: Vec::new(),
            search_pick: 0,
            comment_pick: 0,
        };
        app.refresh()?;
        app.candidates = transport::candidates(
            &app.repo,
            &app.config.adapters,
            &app.config.agents,
            &app.config.new_tab_agent,
        );
        if let (Some(session), Some(block)) = (session, block) {
            app.status = format!("paired with Rex block {}", block);
            app.target = Some(Target::Rex { session, block });
        } else {
            app.status = "press s to choose where to send".to_string();
        }
        if !warning.is_empty() {
            app.status = warning;
        }
        Ok(app)
    }

    pub fn current_file(&self) -> Option<&FileEntry> {
        self.snapshot.files.get(self.selected)
    }

    pub fn current_doc(&self) -> Option<&DiffDoc> {
        self.current_file()
            .and_then(|entry| self.docs.get(&doc_key(entry)))
    }

    pub fn refresh(&mut self) -> Result<()> {
        self.snapshot = git::snapshot(&self.repo)?;
        self.docs.clear();
        self.hashes.clear();
        for entry in &self.snapshot.files {
            let key = doc_key(entry);
            let raw = git::diff_text(&self.repo, &entry.path, entry.section).unwrap_or_default();
            self.docs
                .insert(key.clone(), crate::diff::parse(&raw, &entry.path));
            self.hashes.insert(
                key,
                git::content_hash(&self.repo, &entry.path, entry.section),
            );
        }
        self.comments.reanchor(&self.docs, &self.hashes);
        let _ = self.comments.save(&self.repo);
        self.branch_state = git::branch_state(&self.repo, &self.config.base_branch);
        self.tree_token = git::tree_token(&self.repo);
        if self.selected >= self.snapshot.files.len() {
            self.selected = 0;
        }
        let length = self
            .current_doc()
            .map(|doc| doc.lines.len())
            .unwrap_or(1);
        self.cursor = self.cursor.min(length.saturating_sub(1));
        Ok(())
    }

    fn guard(&self) -> bool {
        git::tree_token(&self.repo) == self.tree_token
    }

    fn report<T>(&mut self, result: Result<T>, success: &str) {
        match result {
            Ok(_) => {
                if let Err(error) = self.refresh() {
                    self.status = error.to_string();
                } else {
                    self.status = success.to_string();
                }
            }
            Err(error) => self.status = error.to_string(),
        }
    }

    pub fn handle_key(&mut self, key: KeyEvent) {
        let pressed = key_to_string(key);
        if pressed.is_empty() {
            return;
        }
        match self.mode {
            Mode::Normal => self.handle_normal(&pressed),
            Mode::PendingG => {
                if pressed == "c" {
                    self.input.clear();
                    self.mode = Mode::CommitInput;
                } else {
                    self.mode = Mode::Normal;
                }
            }
            Mode::CommentInput | Mode::CommitInput => self.handle_input(&pressed, key),
            Mode::TargetPicker => self.handle_target_picker(&pressed),
            Mode::Help => self.mode = Mode::Normal,
            Mode::Confirm => self.handle_confirm(&pressed),
            Mode::FilePicker => self.handle_file_picker(&pressed, key),
            Mode::History => self.handle_history(&pressed),
            Mode::DiffSearch => self.handle_search(&pressed, key),
            Mode::CommentPanel => self.handle_comment_panel(&pressed),
            Mode::CommandOutput => {
                if pressed == "Esc" || pressed == "q" || pressed == self.config.key("quit") {
                    self.mode = Mode::Normal;
                }
            }
            Mode::BranchInput => self.handle_branch_input(&pressed, key),
            Mode::BasePicker => self.handle_base_picker(&pressed),
            Mode::RenameInput => self.handle_rename_input(&pressed, key),
        }
    }

    fn handle_normal(&mut self, pressed: &str) {
        if pressed == "Up" || pressed == self.config.key("up") {
            self.move_cursor(-1);
            return;
        }
        if pressed == "Down" || pressed == self.config.key("down") {
            self.move_cursor(1);
            return;
        }
        if pressed == "g" && self.config.key("commit").starts_with('g') {
            self.mode = Mode::PendingG;
            return;
        }
        let Some(action) = self.config.action_for(pressed) else {
            return;
        };
        match action.as_str() {
            "quit" => self.should_quit = true,
            "toggle_view" => {
                self.view = match self.view {
                    View::Review => View::Git,
                    View::Git => View::Review,
                }
            }
            "next_hunk" => self.jump_hunk(1),
            "prev_hunk" => self.jump_hunk(-1),
            "next_file" => self.move_file(1),
            "prev_file" => self.move_file(-1),
            "comment" => self.open_comment(),
            "delete" => self.delete_comment(),
            "send" => self.open_send_picker(),
            "new_tab" => self.send_new_tab(),
            "pick_target" => {
                self.candidates = transport::candidates(
                &self.repo,
                &self.config.adapters,
                &self.config.agents,
                &self.config.new_tab_agent,
            );
                self.pending_send = false;
                self.selected_target = 0;
                self.mode = Mode::TargetPicker;
            }
            "copy" => self.copy_payload(),
            "refresh" => self.report(Ok::<(), anyhow::Error>(()), "refreshed"),
            "stage" => self.stage(false),
            "stage_all" => self.stage_all(),
            "unstage" => self.unstage(),
            "unstage_all" => self.unstage_all(),
            "stage_hunk" => self.stage(true),
            "help" => self.mode = Mode::Help,
            "toggle_split" => self.split = !self.split,
            "scroll_left" => self.h_scroll = self.h_scroll.saturating_sub(4),
            "scroll_right" => self.h_scroll = self.h_scroll.saturating_add(4).min(400),
            "scroll_reset" => self.h_scroll = 0,
            "pick_base" => self.open_base_picker(),
            "rename_branch" => self.open_rename_input(),
            "push" => self.confirm_push(),
            "force_push" => self.confirm_force_push(),
            "pull" => self.confirm_pull(),
            "toggle_comments" => self.show_comments = !self.show_comments,
            "goto_file" => self.open_file_picker(),
            "focus_comments" => self.focus_comments(),
            "edit_comment" => self.start_edit_comment(),
            "resend_comments" => self.resend_comments(),
            "history" => self.open_history(),
            "update_branch" => self.run_update(),
            "branchup" => self.open_branch_input(),
            "search" => self.open_search(),
            "next_match" => self.cycle_match(1),
            "prev_match" => self.cycle_match(-1),
            "open_editor_tab" => self.open_editor(transport::EditorPlacement::Tab),
            "open_editor_float" => self.open_editor(transport::EditorPlacement::Float),
            _ => {}
        }
    }

    fn open_editor(&mut self, placement: transport::EditorPlacement) {
        let Some(entry) = self.current_file().cloned() else {
            self.status = "no file selected".to_string();
            return;
        };
        let index_line = self
            .current_doc()
            .and_then(|doc| doc.lines.get(self.cursor))
            .and_then(|diff_line| diff_line.new_no.or(diff_line.old_no))
            .unwrap_or(1);
        let line = if entry.section == Section::Staged {
            git::worktree_line(&self.repo, &entry.path, index_line)
        } else {
            index_line
        };
        match transport::open_editor(
            &self.repo,
            &entry.path,
            line,
            placement,
            &self.config.editor_command,
        ) {
            Ok(marker) => {
                self.edit_marker = Some(marker);
                let label = match placement {
                    transport::EditorPlacement::Tab => "new tab",
                    transport::EditorPlacement::Float => "float",
                };
                self.status = format!(
                    "opened {} at line {} in {} (AstroVim)",
                    entry.path, line, label
                );
            }
            Err(error) => self.status = format!("editor: {}", error),
        }
    }

    fn start_edit_comment(&mut self) {
        let Some(entry) = self.current_file().cloned() else {
            self.status = "no file selected".to_string();
            return;
        };
        let line_no = self
            .current_doc()
            .and_then(|doc| doc.lines.get(self.cursor))
            .and_then(|line| line.new_no.or(line.old_no))
            .unwrap_or(0);
        let Some(comment) = self
            .comments
            .for_line(&entry.path, line_no)
            .first()
            .copied()
        else {
            self.focus_comments();
            return;
        };
        self.input = comment.text.clone();
        self.editing_id = Some(comment.id);
        self.pending_comment = None;
        self.mode = Mode::CommentInput;
    }

    fn resend_comments(&mut self) {
        let count = self.comments.resend_all();
        let _ = self.comments.save(&self.repo);
        self.status = if count == 0 {
            "no sent comments to resend".to_string()
        } else {
            format!("{} comment(s) queued again", count)
        };
    }

    fn open_history(&mut self) {
        self.history = git::log(&self.repo, 40).unwrap_or_default();
        self.history_pick = 0;
        self.update_history_diff();
        self.mode = Mode::History;
        if self.history.is_empty() {
            self.status = "no commits yet".to_string();
        }
    }

    fn update_history_diff(&mut self) {
        self.history_diff = self
            .history
            .get(self.history_pick)
            .and_then(|entry| git::commit_diff(&self.repo, &entry.sha).ok())
            .map(|raw| crate::diff::parse(&raw, "commit"));
    }

    fn handle_history(&mut self, pressed: &str) {
        if pressed == "Esc" || pressed == "q" || pressed == self.config.key("quit") {
            self.mode = Mode::Normal;
            return;
        }
        if pressed == "Up" || pressed == self.config.key("up") {
            if self.history_pick > 0 {
                self.history_pick -= 1;
                self.history_scroll = 0;
                self.update_history_diff();
            }
            return;
        }
        if pressed == "Down" || pressed == self.config.key("down") {
            if self.history_pick + 1 < self.history.len() {
                self.history_pick += 1;
                self.history_scroll = 0;
                self.update_history_diff();
            }
            return;
        }
        if pressed == "J" {
            self.history_scroll = self.history_scroll.saturating_add(6);
            return;
        }
        if pressed == "K" {
            self.history_scroll = self.history_scroll.saturating_sub(6);
            return;
        }
        if pressed == "Enter" || pressed == self.config.key("confirm") {
            self.mode = Mode::Normal;
            if let Some(entry) = self.history.get(self.history_pick) {
                self.status = format!("selected {}", entry.sha);
            }
        }
    }

    fn open_search(&mut self) {
        self.search_query.clear();
        self.search_hits.clear();
        self.search_pick = 0;
        self.mode = Mode::DiffSearch;
    }

    fn update_search(&mut self) {
        let query = self.search_query.to_lowercase();
        self.search_hits = match self.current_doc() {
            Some(doc) if !query.is_empty() => doc
                .lines
                .iter()
                .enumerate()
                .filter(|(_, line)| line.text.to_lowercase().contains(&query))
                .map(|(index, _)| index)
                .collect(),
            _ => Vec::new(),
        };
        self.search_pick = 0;
        self.jump_to_match();
    }

    fn jump_to_match(&mut self) {
        if let Some(&index) = self.search_hits.get(self.search_pick) {
            self.cursor = index;
            self.status = format!(
                "match {} of {}",
                self.search_pick + 1,
                self.search_hits.len()
            );
        }
    }

    fn cycle_match(&mut self, delta: i32) {
        if self.search_hits.is_empty() {
            self.status = "no active search".to_string();
            return;
        }
        let length = self.search_hits.len() as i32;
        let current = self.search_pick as i32;
        self.search_pick = (current + delta).rem_euclid(length) as usize;
        self.jump_to_match();
    }

    fn handle_search(&mut self, pressed: &str, key: KeyEvent) {
        if pressed == "Esc" || pressed == self.config.key("cancel") {
            self.mode = Mode::Normal;
            return;
        }
        if pressed == "Enter" || pressed == self.config.key("confirm") {
            self.mode = Mode::Normal;
            return;
        }
        if pressed == "Backspace" {
            self.search_query.pop();
            self.update_search();
            return;
        }
        if pressed == "C-u" {
            self.search_query.clear();
            self.update_search();
            return;
        }
        if let KeyCode::Char(character) = key.code {
            if !key.modifiers.contains(KeyModifiers::CONTROL)
                && !key.modifiers.contains(KeyModifiers::ALT)
            {
                self.search_query.push(character);
                self.update_search();
            }
        }
    }

    fn focus_comments(&mut self) {
        if self.comments.items.is_empty() {
            self.status = "no comments yet".to_string();
            return;
        }
        if self.comment_pick >= self.comments.items.len() {
            self.comment_pick = 0;
        }
        self.mode = Mode::CommentPanel;
        self.status = "comments: j/k select, m edit, d delete, Enter jump, Esc back"
            .to_string();
    }

    fn handle_comment_panel(&mut self, pressed: &str) {
        if pressed == "Esc"
            || pressed == "q"
            || pressed == self.config.key("cancel")
            || pressed == self.config.key("focus_comments")
        {
            self.mode = Mode::Normal;
            return;
        }
        let length = self.comments.items.len();
        if pressed == "Up" || pressed == self.config.key("up") {
            self.comment_pick = self.comment_pick.saturating_sub(1);
            return;
        }
        if pressed == "Down" || pressed == self.config.key("down") {
            self.comment_pick = (self.comment_pick + 1).min(length.saturating_sub(1));
            return;
        }
        let Some(comment) = self.comments.items.get(self.comment_pick).cloned() else {
            self.mode = Mode::Normal;
            return;
        };
        if pressed == "Enter" || pressed == self.config.key("confirm") {
            if let Some(index) = self
                .snapshot
                .files
                .iter()
                .position(|entry| entry.path == comment.path)
            {
                self.selected = index;
                self.cursor = 0;
            }
            self.mode = Mode::Normal;
            self.status = format!("jumped to {}", comment.path);
            return;
        }
        if pressed == self.config.key("edit_comment") {
            self.input = comment.text.clone();
            self.editing_id = Some(comment.id);
            self.pending_comment = None;
            self.mode = Mode::CommentInput;
            return;
        }
        if pressed == self.config.key("delete") {
            self.comments.items.retain(|item| item.id != comment.id);
            let _ = self.comments.save(&self.repo);
            self.comment_pick = self.comment_pick.min(self.comments.items.len().saturating_sub(1));
            self.status = "comment deleted".to_string();
            if self.comments.items.is_empty() {
                self.mode = Mode::Normal;
            }
        }
    }

    fn open_base_picker(&mut self) {
        self.base_branches = git::list_branches(&self.repo).unwrap_or_default();
        self.base_pick = self
            .base_branches
            .iter()
            .position(|name| name == &self.config.base_branch)
            .unwrap_or(0);
        self.mode = Mode::BasePicker;
        if self.base_branches.is_empty() {
            self.status = "no local branches".to_string();
        }
    }

    fn handle_base_picker(&mut self, pressed: &str) {
        if pressed == "Esc" || pressed == self.config.key("cancel") || pressed == "q" {
            self.mode = Mode::Normal;
            return;
        }
        if pressed == "Up" || pressed == self.config.key("up") {
            self.base_pick = self.base_pick.saturating_sub(1);
            return;
        }
        if pressed == "Down" || pressed == self.config.key("down") {
            self.base_pick = (self.base_pick + 1).min(self.base_branches.len().saturating_sub(1));
            return;
        }
        if pressed == "Enter" || pressed == self.config.key("confirm") {
            if let Some(name) = self.base_branches.get(self.base_pick).cloned() {
                self.config.base_branch = name.clone();
                self.branch_state = git::branch_state(&self.repo, &name);
                self.status = format!("base branch set to {}", name);
            }
            self.mode = Mode::Normal;
        }
    }

    fn open_rename_input(&mut self) {
        self.rename_input.clear();
        self.mode = Mode::RenameInput;
    }

    fn handle_rename_input(&mut self, pressed: &str, key: KeyEvent) {
        if pressed == "Esc" || pressed == self.config.key("cancel") {
            self.mode = Mode::Normal;
            return;
        }
        if pressed == "Backspace" {
            self.rename_input.pop();
            return;
        }
        if pressed == "C-u" {
            self.rename_input.clear();
            return;
        }
        if pressed == "Enter" || pressed == self.config.key("confirm") {
            let name = self.rename_input.trim().to_string();
            self.mode = Mode::Normal;
            if name.is_empty() {
                self.status = "branch name is empty".to_string();
                return;
            }
            self.confirm_action = Some(format!("rename:{}", name));
            self.mode = Mode::Confirm;
            self.status = format!("Rename {} to {}?", self.branch_state.current, name);
            return;
        }
        if let KeyCode::Char(character) = key.code {
            if !key.modifiers.contains(KeyModifiers::CONTROL)
                && !key.modifiers.contains(KeyModifiers::ALT)
            {
                self.rename_input.push(character);
            }
        }
    }

    fn confirm_push(&mut self) {
        self.confirm_action = Some("push".to_string());
        self.mode = Mode::Confirm;
        self.status = format!("Push {}?", self.branch_state.current);
    }

    fn confirm_pull(&mut self) {
        self.confirm_action = Some("pull".to_string());
        self.mode = Mode::Confirm;
        self.status = format!("Pull into {}?", self.branch_state.current);
    }

    fn confirm_force_push(&mut self) {
        self.confirm_action = Some("force_push".to_string());
        self.mode = Mode::Confirm;
        self.status = format!(
            "Force-with-lease push {}? Verify remote state first",
            self.branch_state.current
        );
    }

    fn open_branch_input(&mut self) {
        self.branch_input.clear();
        self.mode = Mode::BranchInput;
    }

    fn handle_branch_input(&mut self, pressed: &str, key: KeyEvent) {
        if pressed == "Esc" || pressed == self.config.key("cancel") {
            self.mode = Mode::Normal;
            return;
        }
        if pressed == "Backspace" {
            self.branch_input.pop();
            return;
        }
        if pressed == "C-u" {
            self.branch_input.clear();
            return;
        }
        if pressed == "Enter" || pressed == self.config.key("confirm") {
            let name = self.branch_input.trim().to_string();
            self.mode = Mode::Normal;
            if name.is_empty() {
                self.status = "branch name is empty".to_string();
                return;
            }
            let command = self
                .config
                .branchup_command
                .replace("{branch}", &name)
                .replace("{base}", &self.config.base_branch);
            self.run_command("branchup", &command);
            return;
        }
        if let KeyCode::Char(character) = key.code {
            if !key.modifiers.contains(KeyModifiers::CONTROL)
                && !key.modifiers.contains(KeyModifiers::ALT)
            {
                self.branch_input.push(character);
            }
        }
    }

    fn run_update(&mut self) {
        let command = self
            .config
            .update_command
            .replace("{base}", &self.config.base_branch);
        self.run_command("git_update", &command);
    }

    fn run_command(&mut self, title: &str, template: &str) {
        let mut command = template.to_string();
        for token in ["{update_script}", "{branchup_script}"] {
            if let Some(path) = crate::scripts::command_path(token) {
                command = command.replace(token, &crate::transport::shell_quote(&path.display().to_string()));
            }
        }
        self.command_title = format!("{}: {}", title, command);
        match git::run_shell(&self.repo, &command) {
            Ok(output) => {
                self.command_output = output;
                self.status = format!("{} finished", title);
            }
            Err(error) => {
                self.command_output = error.to_string();
                self.status = format!("{} failed", title);
            }
        }
        let _ = self.refresh();
        self.mode = Mode::CommandOutput;
    }

    pub fn poll_tree(&mut self) {
        if self.mode != Mode::Normal {
            return;
        }
        if self.last_tree_check.elapsed() < std::time::Duration::from_millis(900) {
            return;
        }
        self.last_tree_check = std::time::Instant::now();
        let token = git::tree_token(&self.repo);
        if token == self.tree_token {
            return;
        }
        match self.refresh() {
            Ok(()) => self.status = "auto-refreshed: files changed".to_string(),
            Err(error) => self.status = error.to_string(),
        }
    }

    pub fn poll_editor(&mut self) {
        let Some(marker) = self.edit_marker.clone() else {
            return;
        };
        if !marker.exists() {
            return;
        }
        let _ = std::fs::remove_file(&marker);
        self.edit_marker = None;
        self.report(Ok::<(), anyhow::Error>(()), "refreshed after editor");
    }

    fn move_cursor(&mut self, delta: i32) {
        let length = self.current_doc().map(|doc| doc.lines.len()).unwrap_or(1);
        if delta > 0 {
            self.cursor = (self.cursor + 1).min(length.saturating_sub(1));
        } else {
            self.cursor = self.cursor.saturating_sub(1);
        }
    }

    fn move_file(&mut self, delta: i32) {
        let length = self.snapshot.files.len();
        if length == 0 {
            return;
        }
        if delta > 0 {
            self.selected = (self.selected + 1) % length;
        } else {
            self.selected = (self.selected + length - 1) % length;
        }
        self.cursor = 0;
        self.h_scroll = 0;
    }

    fn jump_hunk(&mut self, delta: i32) {
        let positions: Vec<usize> = self
            .current_doc()
            .map(|doc| {
                doc.lines
                    .iter()
                    .enumerate()
                    .filter(|(_, line)| line.kind == LineKind::Hunk)
                    .map(|(index, _)| index)
                    .collect()
            })
            .unwrap_or_default();
        if positions.is_empty() {
            return;
        }
        if delta > 0 {
            if let Some(position) = positions.iter().find(|&&item| item > self.cursor) {
                self.cursor = *position;
            }
        } else if let Some(position) = positions.iter().rev().find(|&&item| item < self.cursor) {
            self.cursor = *position;
        }
    }

    fn open_comment(&mut self) {
        let Some(entry) = self.current_file().cloned() else {
            return;
        };
        let key = doc_key(&entry);
        let Some(doc) = self.docs.get(&key) else {
            return;
        };
        let Some(line) = doc.lines.get(self.cursor) else {
            return;
        };
        if line.kind == LineKind::Header || line.kind == LineKind::Hunk {
            self.status = "move to a content line first".to_string();
            return;
        }
        let side = if line.kind == LineKind::Remove {
            Side::Left
        } else {
            Side::Right
        };
        let line_no = line.new_no.or(line.old_no).unwrap_or(0);
        self.editing_id = None;
        self.pending_comment = Some((
            entry.path.clone(),
            entry.section,
            side,
            line_no,
            line.text.clone(),
            self.hashes.get(&key).cloned().unwrap_or_default(),
        ));
        self.input.clear();
        self.mode = Mode::CommentInput;
    }

    fn delete_comment(&mut self) {
        let Some(entry) = self.current_file().cloned() else {
            return;
        };
        let key = doc_key(&entry);
        let line_no = self
            .docs
            .get(&key)
            .and_then(|doc| doc.lines.get(self.cursor))
            .and_then(|line| line.new_no.or(line.old_no))
            .unwrap_or(0);
        self.comments
            .items
            .retain(|comment| !(comment.path == entry.path && comment.line_no == line_no));
        let _ = self.comments.save(&self.repo);
        self.status = "comment removed".to_string();
    }

    fn open_send_picker(&mut self) {
        if self.comments.unsent().is_empty() {
            self.status = "no unsent comments".to_string();
            return;
        }
        self.candidates = transport::candidates(
                &self.repo,
                &self.config.adapters,
                &self.config.agents,
                &self.config.new_tab_agent,
            );
        if self.candidates.is_empty() {
            self.status = "no agent detected; copy with y".to_string();
            return;
        }
        self.selected_target = 0;
        self.pending_send = true;
        self.mode = Mode::TargetPicker;
    }

    fn deliver(&mut self, target: &Target) {
        let payload = self.comments.payload(&self.repo);
        match transport::send(
            target,
            &payload,
            self.config.send_mode.clone(),
            &self.repo,
            &self.config,
        ) {
            Ok(message) => {
                self.comments.mark_sent();
                let _ = self.comments.save(&self.repo);
                self.status = message;
            }
            Err(error) => self.status = format!("send failed: {}", error),
        }
    }

    fn send_new_tab(&mut self) {
        if self.comments.unsent().is_empty() {
            self.status = "no unsent comments".to_string();
            return;
        }
        let payload = self.comments.payload(&self.repo);
        match transport::send(
            &Target::NewTab,
            &payload,
            SendMode::Submit,
            &self.repo,
            &self.config,
        ) {
            Ok(message) => {
                self.comments.mark_sent();
                let _ = self.comments.save(&self.repo);
                self.status = message;
            }
            Err(error) => self.status = format!("new tab failed: {}", error),
        }
    }

    fn copy_payload(&mut self) {
        let payload = self.comments.payload(&self.repo);
        match transport::send(
            &Target::Clipboard,
            &payload,
            SendMode::Queue,
            &self.repo,
            &self.config,
        ) {
            Ok(message) => self.status = message,
            Err(error) => self.status = error.to_string(),
        }
    }

    fn stage_all(&mut self) {
        self.confirm_action = Some("stage_all".to_string());
        self.mode = Mode::Confirm;
        self.status = format!(
            "Stage all {} files? Enter confirms, Esc cancels",
            self.snapshot.files.len()
        );
    }

    fn unstage_all(&mut self) {
        self.confirm_action = Some("unstage_all".to_string());
        self.mode = Mode::Confirm;
        self.status = format!(
            "Unstage all {} files? Enter confirms, Esc cancels",
            self.snapshot.files.len()
        );
    }

    fn stage(&mut self, hunk: bool) {
        if !self.guard() {
            self.status = "tree changed since render; press r to refresh".to_string();
            return;
        }
        let Some(entry) = self.current_file().cloned() else {
            return;
        };
        let key = doc_key(&entry);
        if hunk {
            if entry.section == Section::Untracked {
                self.status = "use a to stage new files".to_string();
                return;
            }
            let patch = self.docs.get(&key).and_then(|doc| {
                let index = doc.lines.get(self.cursor).map(|line| line.hunk).unwrap_or(0);
                crate::diff::patch_for_hunk(doc, index)
            });
            let Some(patch) = patch else {
                self.status = "no hunk under cursor".to_string();
                return;
            };
            self.report(git::stage_hunk(&self.repo, &patch), "hunk staged");
            return;
        }
        if entry.section == Section::Staged {
            self.status = "already staged".to_string();
            return;
        }
        self.report(git::stage_file(&self.repo, &entry.path), "file staged");
    }

    fn unstage(&mut self) {
        if !self.guard() {
            self.status = "tree changed since render; press r to refresh".to_string();
            return;
        }
        let Some(entry) = self.current_file().cloned() else {
            return;
        };
        if entry.section != Section::Staged {
            self.status = "file is not staged".to_string();
            return;
        }
        self.report(git::unstage_file(&self.repo, &entry.path), "file unstaged");
    }

    fn do_commit(&mut self, message: String) {
        if message.trim().is_empty() {
            self.status = "commit message is empty".to_string();
            self.mode = Mode::Normal;
            return;
        }
        self.report(git::commit(&self.repo, message.trim()), "committed");
        self.mode = Mode::Normal;
        self.input.clear();
        self.confirm_action = None;
    }

    pub fn handle_paste(&mut self, text: String) {
        if self.mode == Mode::FilePicker {
            let clean = text.replace("\r\n", " ").replace(['\r', '\n'], " ");
            self.file_query.push_str(&clean);
            self.file_pick = 0;
            self.update_file_matches();
            return;
        }
        if self.mode == Mode::CommentInput || self.mode == Mode::CommitInput {
            let clean = text.replace("\r\n", "\n").replace('\r', "\n");
            self.input.push_str(&clean);
        }
    }

    fn open_file_picker(&mut self) {
        self.file_query.clear();
        self.file_pick = 0;
        self.mode = Mode::FilePicker;
        self.update_file_matches();
    }

    fn update_file_matches(&mut self) {
        let query = self.file_query.to_lowercase();
        self.file_matches = self
            .snapshot
            .files
            .iter()
            .enumerate()
            .filter(|(_, entry)| {
                if query.is_empty() {
                    return true;
                }
                let path = entry.path.to_lowercase();
                path.contains(&query) || subsequence(&query, &path)
            })
            .map(|(index, _)| index)
            .collect();
        if self.file_pick >= self.file_matches.len() {
            self.file_pick = self.file_matches.len().saturating_sub(1);
        }
    }

    fn handle_file_picker(&mut self, pressed: &str, key: KeyEvent) {
        if pressed == "Esc" || pressed == self.config.key("cancel") {
            self.mode = Mode::Normal;
            self.file_query.clear();
            return;
        }
        if pressed == "Up" || pressed == self.config.key("up") {
            self.file_pick = self.file_pick.saturating_sub(1);
            return;
        }
        if pressed == "Down" || pressed == self.config.key("down") {
            self.file_pick = (self.file_pick + 1)
                .min(self.file_matches.len().saturating_sub(1));
            return;
        }
        if pressed == "Backspace" {
            self.file_query.pop();
            self.update_file_matches();
            return;
        }
        if pressed == "Enter" || pressed == self.config.key("confirm") {
            if let Some(&index) = self.file_matches.get(self.file_pick) {
                self.selected = index;
                self.cursor = 0;
                let path = self.snapshot.files[index].path.clone();
                self.status = format!("jumped to {}", path);
            }
            self.mode = Mode::Normal;
            self.file_query.clear();
            return;
        }
        if let KeyCode::Char(character) = key.code {
            if !key.modifiers.contains(KeyModifiers::CONTROL)
                && !key.modifiers.contains(KeyModifiers::ALT)
            {
                self.file_query.push(character);
                self.file_pick = 0;
                self.update_file_matches();
            }
        }
    }

    fn is_newline_key(key: KeyEvent) -> bool {
        match key.code {
            KeyCode::Enter => key.modifiers.intersects(
                KeyModifiers::SHIFT | KeyModifiers::ALT | KeyModifiers::CONTROL,
            ),
            KeyCode::Char('j') => key.modifiers.contains(KeyModifiers::CONTROL),
            KeyCode::Char('\n') | KeyCode::Char('\r') => true,
            _ => false,
        }
    }

    fn handle_input(&mut self, pressed: &str, key: KeyEvent) {
        if pressed == "Esc" || pressed == self.config.key("cancel") {
            self.mode = Mode::Normal;
            self.input.clear();
            self.pending_comment = None;
            self.editing_id = None;
            return;
        }
        if Self::is_newline_key(key)
            || pressed == "S-Enter"
            || pressed == self.config.key("newline")
        {
            if self.mode == Mode::CommentInput || self.mode == Mode::CommitInput {
                self.input.push('\n');
            }
            return;
        }
        if pressed == "C-u" {
            self.input.clear();
            return;
        }
        if pressed == "Backspace" {
            self.input.pop();
            return;
        }
        if let KeyCode::Char(character) = key.code {
            if !key.modifiers.contains(KeyModifiers::CONTROL) {
                self.input.push(character);
                return;
            }
        }
        if pressed != "Enter" && pressed != self.config.key("confirm") {
            return;
        }
        if self.mode == Mode::CommentInput {
            if let Some(id) = self.editing_id.take() {
                if !self.input.trim().is_empty() {
                    self.comments.edit_text(id, self.input.trim().to_string());
                    let _ = self.comments.save(&self.repo);
                    self.status = "comment updated".to_string();
                }
                self.mode = Mode::Normal;
                self.input.clear();
                return;
            }
            if let Some((path, section, side, line_no, line_text, hash)) =
                self.pending_comment.take()
            {
                if !self.input.trim().is_empty() {
                    self.comments.add(
                        path,
                        section,
                        side,
                        line_no,
                        line_text,
                        hash,
                        self.input.trim().to_string(),
                    );
                    let _ = self.comments.save(&self.repo);
                    self.status = "comment added".to_string();
                }
            }
            self.mode = Mode::Normal;
            self.input.clear();
            return;
        }
        let message = self.input.clone();
        if git::has_conflicts(&self.repo) {
            self.status = "unmerged paths; resolve conflicts first".to_string();
            self.mode = Mode::Normal;
            return;
        }
        if !self.guard() {
            self.confirm_action = Some(format!("commit:{}", message));
            self.mode = Mode::Confirm;
            self.status = "tree changed since render; Enter commits anyway".to_string();
            return;
        }
        self.do_commit(message);
    }

    fn handle_target_picker(&mut self, pressed: &str) {
        if pressed == "Esc" || pressed == self.config.key("cancel") {
            self.mode = Mode::Normal;
            self.pending_send = false;
            self.status = "send cancelled".to_string();
            return;
        }
        if pressed == "Up" || pressed == self.config.key("up") {
            self.selected_target = self.selected_target.saturating_sub(1);
            return;
        }
        if pressed == "Down" || pressed == self.config.key("down") {
            self.selected_target = (self.selected_target + 1)
                .min(self.candidates.len().saturating_sub(1));
            return;
        }
        if pressed == "Enter" || pressed == self.config.key("confirm") {
            let Some(candidate) = self.candidates.get(self.selected_target).cloned() else {
                self.mode = Mode::Normal;
                self.pending_send = false;
                return;
            };
            self.target = Some(candidate.target.clone());
            self.status = format!("target: {}", candidate.label);
            self.mode = Mode::Normal;
            let should_send = self.pending_send;
            self.pending_send = false;
            if should_send {
                self.deliver(&candidate.target);
            }
        }
    }

    fn handle_confirm(&mut self, pressed: &str) {
        if pressed == "Esc" || pressed == self.config.key("cancel") {
            self.confirm_action = None;
            self.mode = Mode::Normal;
            return;
        }
        if pressed == "Enter" || pressed == self.config.key("confirm") {
            self.mode = Mode::Normal;
            if let Some(action) = self.confirm_action.take() {
                if let Some(message) = action.strip_prefix("commit:") {
                    self.do_commit(message.to_string());
                } else if action == "stage_all" {
                    if self.guard() {
                        self.report(git::stage_all(&self.repo), "staged all files");
                    } else {
                        self.status = "tree changed since render; press r to refresh".to_string();
                    }
                } else if action == "unstage_all" {
                    if self.guard() {
                        self.report(git::unstage_all(&self.repo), "unstaged all files");
                    } else {
                        self.status = "tree changed since render; press r to refresh".to_string();
                    }
                } else if let Some(name) = action.strip_prefix("rename:") {
                    match git::rename_branch(&self.repo, name) {
                        Ok(_) => {
                            self.status = format!("renamed to {}", name);
                            let _ = self.refresh();
                        }
                        Err(error) => self.status = format!("rename failed: {}", error),
                    }
                } else if action == "pull" {
                    self.command_title = "git pull".to_string();
                    match git::pull_branch(&self.repo) {
                        Ok(output) => {
                            self.command_output = output;
                            self.status = "pull finished".to_string();
                        }
                        Err(error) => {
                            self.command_output = error.to_string();
                            self.status = "pull failed".to_string();
                        }
                    }
                    let _ = self.refresh();
                    self.mode = Mode::CommandOutput;
                } else if action == "push" || action == "force_push" {
                    let force = action == "force_push";
                    self.command_title = if force {
                        "git push -u origin HEAD --force-with-lease".to_string()
                    } else {
                        "git push -u origin HEAD".to_string()
                    };
                    match git::push_branch(&self.repo, force) {
                        Ok(output) => {
                            self.command_output = output;
                            self.status = if force {
                                "force push finished".to_string()
                            } else {
                                "push finished".to_string()
                            };
                        }
                        Err(error) => {
                            self.command_output = error.to_string();
                            self.status = if force {
                                "force push failed".to_string()
                            } else {
                                "push failed".to_string()
                            };
                        }
                    }
                    let _ = self.refresh();
                    self.mode = Mode::CommandOutput;
                }
            }
        }
    }
}
