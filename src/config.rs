use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

#[derive(Clone, Debug, PartialEq)]
pub enum SendMode {
    Queue,
    Submit,
}

#[derive(Clone, Debug)]
pub struct Config {
    pub keys: HashMap<String, String>,
    pub send_mode: SendMode,
    pub adapters: Vec<String>,
    pub agents: Vec<String>,
    pub split: bool,
    pub show_comments: bool,
    pub new_tab_agent: String,
    pub new_tab_command: String,
    pub templates: HashMap<String, String>,
    pub clipboard_command: String,
    pub editor_command: String,
    pub base_branch: String,
    pub update_command: String,
    pub branchup_command: String,
}

fn default_keys() -> HashMap<String, String> {
    let pairs = [
        ("quit", "q"),
        ("toggle_view", "Tab"),
        ("down", "j"),
        ("up", "k"),
        ("next_hunk", "]"),
        ("prev_hunk", "["),
        ("next_file", "f"),
        ("prev_file", "F"),
        ("comment", "c"),
        ("delete", "d"),
        ("send", "s"),
        ("pick_target", "S"),
        ("copy", "y"),
        ("refresh", "gr"),
        ("stage", "ga"),
        ("stage_all", "gA"),
        ("unstage", "gu"),
        ("unstage_all", "gU"),
        ("stage_hunk", "gh"),
        ("commit", "gc"),
        ("help", "?"),
        ("toggle_split", "v"),
        ("toggle_comments", "b"),
        ("goto_file", "o"),
        ("focus_comments", "p"),
        ("toggle_whole_file", "M"),
        ("edit_comment", "m"),
        ("resend_comments", "R"),
        ("history", "L"),
        ("update_branch", "gs"),
        ("branchup", "gb"),
        ("search", "/"),
        ("next_match", "n"),
        ("prev_match", "N"),
        ("open_editor_tab", "e"),
        ("open_editor_float", "E"),
        ("newline", "C-j"),
        ("confirm", "Enter"),
        ("cancel", "Esc"),
        ("scroll_left", "h"),
        ("scroll_right", "l"),
        ("scroll_reset", "0"),
        ("pick_base", "gB"),
        ("rename_branch", "gn"),
        ("push", "gp"),
        ("stash_push", "gz"),
        ("stash_pop", "gZ"),
        ("force_push", "gP"),
        ("pull", "gl"),
        ("visual_select", "V"),
        ("stage_selection", "A"),
    ];
    pairs
        .iter()
        .map(|(action, key)| (action.to_string(), key.to_string()))
        .collect()
}

pub fn default_agents() -> Vec<String> {
    [
        "pi", "claude", "codex", "opencode", "aider", "goose", "cline",
    ]
    .iter()
    .map(|name| name.to_string())
    .collect()
}

pub fn default_templates() -> HashMap<String, String> {
    [
        (
            "pi",
            r#"pi -- @{file} "Please address these review comments""#,
        ),
        (
            "claude",
            r#"claude "Please address the review comments in {file}""#,
        ),
        (
            "codex",
            r#"codex "Please address the review comments in {file}""#,
        ),
        (
            "opencode",
            r#"opencode run "Please address the review comments in {file}""#,
        ),
        ("aider", "aider --message-file {file}"),
        ("goose", "goose run --instructions {file}"),
        (
            "cline",
            r#"cline "Please address the review comments in {file}""#,
        ),
    ]
    .iter()
    .map(|(name, command)| (name.to_string(), command.to_string()))
    .collect()
}

fn config_path() -> PathBuf {
    if let Ok(path) = std::env::var("DIFFNOTE_CONFIG") {
        return PathBuf::from(path);
    }
    let base = std::env::var("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
            PathBuf::from(home).join(".config")
        });
    base.join("diffnote/config.toml")
}

fn parse_value(raw: &str) -> String {
    let text = raw.trim();
    if text.len() >= 2 && text.starts_with('\'') && text.ends_with('\'') {
        return text[1..text.len() - 1].to_string();
    }
    if text.len() >= 2 && text.starts_with('"') && text.ends_with('"') {
        return text[1..text.len() - 1]
            .replace("\\\"", "\"")
            .replace("\\\\", "\\");
    }
    text.to_string()
}

fn parse_list(value: &str) -> Vec<String> {
    value
        .trim_matches(|c| c == '[' || c == ']')
        .split(',')
        .map(|item| item.trim().trim_matches('"').trim_matches('\'').to_string())
        .filter(|item| !item.is_empty())
        .collect()
}

fn default_clipboard() -> String {
    for program in ["pbcopy", "wl-copy", "xclip", "xsel"] {
        if which(program) {
            return program.to_string();
        }
    }
    "pbcopy".to_string()
}

fn default_editor() -> String {
    if let Ok(editor) = std::env::var("DIFFNOTE_NVIM") {
        return editor;
    }
    if let Ok(editor) = std::env::var("VISUAL") {
        return editor;
    }
    if let Ok(editor) = std::env::var("EDITOR") {
        return editor;
    }
    "nvim".to_string()
}

fn which(program: &str) -> bool {
    let path = std::env::var("PATH").unwrap_or_default();
    path.split(':').any(|dir| std::path::Path::new(dir).join(program).is_file())
}

impl Config {
    pub fn load() -> (Config, String) {
        let mut config = Config {
            keys: default_keys(),
            send_mode: SendMode::Submit,
            adapters: vec![
                "rex".to_string(),
                "tmux".to_string(),
                "inbox".to_string(),
                "clipboard".to_string(),
            ],
            agents: default_agents(),
            split: true,
            show_comments: true,
            new_tab_agent: "pi".to_string(),
            new_tab_command: default_templates()
                .get("pi")
                .cloned()
                .unwrap_or_default(),
            templates: default_templates(),
            clipboard_command: default_clipboard(),
            editor_command: default_editor(),
            base_branch: "main".to_string(),
            update_command: "{update_script} {base}".to_string(),
            branchup_command: "{branchup_script} {branch} --base {base}".to_string(),
        };
        let path = config_path();
        let text = match std::fs::read_to_string(&path) {
            Ok(text) => text,
            Err(_) => {
                return (
                    config,
                    format!("no config at {}, using defaults", path.display()),
                )
            }
        };
        let mut section = String::new();
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if line.starts_with('[') && line.ends_with(']') {
                section = line[1..line.len() - 1].trim().to_string();
                continue;
            }
            let Some((name, value)) = line.split_once('=') else {
                continue;
            };
            let name = name.trim();
            let value = parse_value(value);
            if section == "keys" {
                config.keys.insert(name.to_string(), value.to_string());
                continue;
            }
            if section == "templates" || section == "new_tab_templates" {
                config
                    .templates
                    .insert(name.to_string(), value.to_string());
                continue;
            }
            match name {
                "send_mode" => {
                    config.send_mode = if value == "submit" {
                        SendMode::Submit
                    } else {
                        SendMode::Queue
                    }
                }
                "split" => config.split = value == "true",
                "show_comments" => config.show_comments = value == "true",
                "agents" => {
                    let list = parse_list(&value);
                    if !list.is_empty() {
                        config.agents = list;
                    }
                }
                "new_tab_agent" => config.new_tab_agent = value.to_string(),
                "new_tab_command" => config.new_tab_command = value.to_string(),
                "clipboard_command" => config.clipboard_command = value.to_string(),
                "editor_command" => config.editor_command = value.to_string(),
                "base_branch" => config.base_branch = value.to_string(),
                "update_command" => config.update_command = value.to_string(),
                "branchup_command" => config.branchup_command = value.to_string(),
                "adapters" => {
                    let list = parse_list(&value);
                    if !list.is_empty() {
                        config.adapters = list;
                    }
                }
                _ => {}
            }
        }
        if let Some(template) = config.templates.get(&config.new_tab_agent).cloned() {
            let default_pi = default_templates().get("pi").cloned().unwrap_or_default();
            if config.new_tab_command.is_empty() || config.new_tab_command == default_pi {
                config.new_tab_command = template;
            }
        }
        let warning = match config.validate() {
            Ok(()) => String::new(),
            Err(error) => error,
        };
        (config, warning)
    }

    pub fn validate(&self) -> Result<(), String> {
        let mut seen = HashSet::new();
        for (action, key) in &self.keys {
            if !seen.insert(key.clone()) {
                return Err(format!("duplicate key {} for action {}", key, action));
            }
        }
        Ok(())
    }

    pub fn action_for(&self, key: &str) -> Option<String> {
        self.keys
            .iter()
            .find(|(_, bound)| bound.as_str() == key)
            .map(|(action, _)| action.clone())
    }

    pub fn key(&self, action: &str) -> &str {
        self.keys.get(action).map(String::as_str).unwrap_or("")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_single_quoted_template_with_embedded_quotes() {
        assert_eq!(
            parse_value(r#"'pi -- @{file} "Please address these review comments"'"#),
            r#"pi -- @{file} "Please address these review comments""#
        );
    }

    #[test]
    fn default_new_tab_command_is_pi() {
        let (config, _) = {
            let c = Config {
                keys: default_keys(),
                send_mode: SendMode::Submit,
                adapters: vec!["rex".to_string()],
                agents: default_agents(),
                split: true,
                show_comments: true,
                new_tab_agent: "pi".to_string(),
                new_tab_command: default_templates().get("pi").cloned().unwrap(),
                templates: default_templates(),
                clipboard_command: "pbcopy".to_string(),
                editor_command: "nvim".to_string(),
                base_branch: "main".to_string(),
                update_command: "git_update {base}".to_string(),
                branchup_command: "branchup {branch} --base {base}".to_string(),
            };
            c.validate().unwrap();
            (c, String::new())
        };
        assert!(config.new_tab_command.starts_with("pi -- @{file}"));
        assert_eq!(config.templates.len(), 7);
    }

    #[test]
    fn parse_list_handles_quoted_names() {
        assert_eq!(
            parse_list(r#"["pi", "claude", "codex"]"#),
            vec!["pi".to_string(), "claude".to_string(), "codex".to_string()]
        );
    }
}
