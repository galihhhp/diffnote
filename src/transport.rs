use crate::config::SendMode;
use anyhow::{anyhow, Result};
use std::collections::HashMap;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

const LIST_SCRIPT: &str = r#"
local out = {}
local sessions = rex.call("session.list", {})
for _, s in ipairs(sessions.sessions or {}) do
  local ok, blocks = pcall(rex.call, "session.list_blocks", { session_id = s.session_id })
  for _, b in ipairs(ok and type(blocks) == "table" and blocks.blocks or {}) do
    local function call(method)
      local ok2, res = pcall(rex.call, "block.method",
        { session_id = s.session_id, block_id = b.block_id, method = method })
      return ok2 and type(res) == "table" and res or {}
    end
    local records = call("program_status").records or {}
    local app, state, updated = "", "", ""
    for _, r in ipairs(records) do
      if r.id == nil or r.id == "" then state = r.state or state end
      local a = r.effective_app or r.app
      if a and a ~= "" then app = a end
      if r.updated_at and r.updated_at > updated then updated = r.updated_at end
    end
    local pwd = call("pwd").pwd or ""
    pwd = pwd:gsub("^file://[^/]*", "")
    out[#out + 1] = {
      session_id = s.session_id,
      session_label = s.label or "",
      block_id = b.block_id,
      block_label = b.label or "",
      window_id = b.window_id,
      foreground = call("process").foreground,
      title = call("title").title or "",
      app = app,
      state = state,
      updated_at = updated,
      cwd = pwd,
    }
  end
end
return out
"#;

#[derive(Clone, Debug)]
pub enum Target {
    Rex { session: String, block: String },
    Tmux { pane: String },
    NewTab,
    Inbox,
    Clipboard,
}

impl Target {
    pub fn short_label(&self) -> String {
        match self {
            Target::Rex { .. } => "rex".to_string(),
            Target::Tmux { pane } => format!("tmux {}", pane),
            Target::NewTab => "new tab".to_string(),
            Target::Inbox => "inbox".to_string(),
            Target::Clipboard => "clipboard".to_string(),
        }
    }


}

#[derive(Clone, Debug)]
pub struct Candidate {
    pub label: String,
    pub active: bool,
    pub target: Target,
}

fn agent_word<'a>(word: &str, agents: &'a [String]) -> Option<&'a str> {
    let base = word.rsplit('/').next().unwrap_or(word);
    agents
        .iter()
        .find(|agent| agent.as_str() == base)
        .map(String::as_str)
}

fn child_agent(pid: u64, agents: &[String]) -> Option<String> {
    let output = Command::new("ps")
        .args(["-Ao", "pid=,ppid=,args="])
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&output.stdout);
    let mut kids: HashMap<String, Vec<(String, String)>> = HashMap::new();
    for line in text.lines() {
        let mut fields = line.split_whitespace();
        let Some(id) = fields.next() else { continue };
        let Some(parent) = fields.next() else { continue };
        let args = fields.collect::<Vec<_>>().join(" ");
        kids.entry(parent.to_string())
            .or_default()
            .push((id.to_string(), args));
    }
    let mut queue = vec![pid.to_string()];
    let mut index = 0;
    while index < queue.len() {
        let children = kids.get(&queue[index]).cloned().unwrap_or_default();
        for (id, args) in children {
            let mut words = args.split_whitespace();
            let first = words.next().unwrap_or("");
            let second = words.next().unwrap_or("");
            if let Some(agent) = agent_word(first, agents).or_else(|| agent_word(second, agents))
            {
                return Some(agent.to_string());
            }
            queue.push(id);
        }
        index += 1;
    }
    None
}

fn run_with_stdin(program: &str, args: &[&str], input: &str) -> Result<Vec<u8>> {
    let mut child = Command::new(program)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(input.as_bytes())?;
    }
    let output = child.wait_with_output()?;
    if !output.status.success() {
        return Err(anyhow!(
            "{} {} failed: {}",
            program,
            args.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(output.stdout)
}

fn rex_candidates(_repo: &Path, agents: &[String]) -> Vec<Candidate> {
    let mut found = Vec::new();
    let self_block = std::env::var("REX_BLOCK").unwrap_or_default();
    let Ok(out) = run_with_stdin("rex", &["do", "-"], LIST_SCRIPT) else {
        return found;
    };
    let Ok(value) = serde_json::from_slice::<serde_json::Value>(&out) else {
        return found;
    };
    let Some(items) = value.as_array() else {
        return found;
    };
    for item in items {
        let session = item["session_id"].as_str().unwrap_or("");
        let block = item["block_id"].as_str().unwrap_or("");
        let label = item["block_label"].as_str().unwrap_or("");
        if session.is_empty() || block.is_empty() {
            continue;
        }
        if label.starts_with("diffnote") || label.starts_with("review") {
            continue;
        }
        let mut app = item["app"].as_str().unwrap_or("").to_string();
        if app.is_empty() {
            let foreground = &item["foreground"];
            for name in ["argv0", "name", "invoked_path"] {
                if let Some(word) = foreground[name].as_str() {
                    if let Some(agent) = agent_word(word, agents) {
                        app = agent.to_string();
                        break;
                    }
                }
            }
        }
        if app.is_empty() {
            if let Some(pid) = item["foreground"]["pid"].as_u64() {
                app = child_agent(pid, agents).unwrap_or_default();
            }
        }
        if !agents.iter().any(|agent| agent == &app) {
            continue;
        }
        let state = item["state"].as_str().unwrap_or("");
        let active = state == "working";
        let title = item["title"].as_str().unwrap_or("");
        let cwd = item["cwd"].as_str().unwrap_or("");
        let suffix = if cwd.is_empty() {
            format!("{}", &block[..block.len().min(18)])
        } else {
            cwd.to_string()
        };
        let display = if title.is_empty() {
            format!(
                "{} ({}) {}{}",
                app,
                state,
                suffix,
                if block == self_block { " [this terminal]" } else { "" }
            )
        } else {
            format!(
                "{} ({}) {} | {}{}",
                app,
                state,
                title,
                suffix,
                if block == self_block { " [this terminal]" } else { "" }
            )
        };
        found.push(Candidate {
            label: display,
            active,
            target: Target::Rex {
                session: session.to_string(),
                block: block.to_string(),
            },
        });
    }
    found
}

fn tmux_candidates(agents: &[String]) -> Vec<Candidate> {
    let mut found = Vec::new();
    let Ok(output) = Command::new("tmux")
        .args([
            "list-panes",
            "-a",
            "-F",
            "#{pane_id}\t#{pane_pid}\t#{pane_current_path}",
        ])
        .output()
    else {
        return found;
    };
    for line in String::from_utf8_lossy(&output.stdout).lines() {
        let fields: Vec<&str> = line.split('\t').collect();
        if fields.len() < 3 {
            continue;
        }
        let Ok(pid) = fields[1].parse::<u64>() else {
            continue;
        };
        let direct = Command::new("ps")
            .args(["-o", "command=", "-p", fields[1]])
            .output()
            .ok()
            .and_then(|out| {
                let text = String::from_utf8_lossy(&out.stdout);
                let mut words = text.split_whitespace();
                let first = words.next().unwrap_or("");
                let second = words.next().unwrap_or("");
                agent_word(first, agents)
                    .or_else(|| agent_word(second, agents))
                    .map(str::to_string)
            });
        let Some(agent) = direct.or_else(|| child_agent(pid, agents)) else {
            continue;
        };
        found.push(Candidate {
            label: format!("tmux {} ({})", fields[0], agent),
            active: false,
            target: Target::Tmux {
                pane: fields[0].to_string(),
            },
        });
    }
    found
}

pub fn candidates(
    repo: &Path,
    adapters: &[String],
    agents: &[String],
    agent_name: &str,
) -> Vec<Candidate> {
    let mut found = Vec::new();
    if adapters.iter().any(|name| name == "rex")
        && std::env::var_os("REX_SESSION").is_some()
    {
        found.extend(rex_candidates(repo, agents));
    }
    if adapters.iter().any(|name| name == "tmux") && std::env::var_os("TMUX").is_some() {
        found.extend(tmux_candidates(agents));
    }
    found.sort_by(|a, b| b.active.cmp(&a.active));
    if std::env::var_os("REX_SESSION").is_some() && which("rex") {
        found.push(Candidate {
            label: format!("New tab: {} with these comments", agent_name),
            active: false,
            target: Target::NewTab,
        });
    }
    if adapters.iter().any(|name| name == "inbox") {
        found.push(Candidate {
            label: ".diffnote/inbox/*.md".to_string(),
            active: false,
            target: Target::Inbox,
        });
    }
    if adapters.iter().any(|name| name == "clipboard") {
        found.push(Candidate {
            label: "clipboard".to_string(),
            active: false,
            target: Target::Clipboard,
        });
    }
    found
}

fn rex_write(session: &str, block: &str, data: &str) -> Result<()> {
    let payload = serde_json::json!({ "data": data }).to_string();
    run_with_stdin(
        "rex",
        &["block", "call", "-s", session, "-b", block, "write", "-"],
        &payload,
    )
    .map(|_| ())
}

fn lua_string(value: &str) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "\"\"".to_string())
}

fn inbox_file(repo: &Path, payload: &str) -> Result<std::path::PathBuf> {
    let dir = repo.join(".diffnote/inbox");
    std::fs::create_dir_all(&dir)?;
    crate::git::ensure_excluded(repo);
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0);
    let path = dir.join(format!("{}.md", stamp));
    std::fs::write(&path, payload)?;
    Ok(path)
}

fn which(program: &str) -> bool {
    let path = std::env::var("PATH").unwrap_or_default();
    path.split(':').any(|dir| Path::new(dir).join(program).is_file())
}

fn copy_to_clipboard(program: &str, payload: &str) -> Result<String> {
    if program == "wl-copy" || program == "pbcopy" {
        run_with_stdin(program, &[], payload)?;
    } else if program == "xclip" {
        run_with_stdin(program, &["-selection", "clipboard"], payload)?;
    } else if program == "xsel" {
        run_with_stdin(program, &["--clipboard", "--input"], payload)?;
    } else {
        let mut parts = program.split_whitespace();
        let Some(binary) = parts.next() else {
            return Err(anyhow!("empty clipboard_command"));
        };
        let args: Vec<&str> = parts.collect();
        run_with_stdin(binary, &args, payload)?;
    }
    Ok(format!("copied to clipboard via {}", program))
}

fn render_template(template: &str, file: &Path) -> String {
    let quoted = shell_quote(&file.display().to_string());
    template.replace("{file}", &quoted)
}

fn spawn_new_tab(
    repo: &Path,
    payload: &str,
    command_template: &str,
    agent_name: &str,
) -> Result<String> {
    let session = std::env::var("REX_SESSION").unwrap_or_default();
    let file = inbox_file(repo, payload)?;
    let command = render_template(command_template, &file);
    if session.is_empty() {
        if std::env::var_os("TMUX").is_none() {
            return Err(anyhow!(
                "new tab needs Rex or tmux; use the send picker for other targets"
            ));
        }
        let label = format!("diffnote {}", agent_name);
        Command::new("tmux")
            .args([
                "new-window",
                "-c",
                &repo.display().to_string(),
                "-n",
                &label,
                &command,
            ])
            .status()?;
        return Ok(format!("opened tmux window: {} ({})", agent_name, file.display()));
    }
    let label = format!("diffnote {}", agent_name);
    let script = format!(
        r#"
local cwd = {cwd}
local sid = {sid}
local cmd = {cmd}
local label = {label}
local layout = rex.layout.block{{ flavor = "com.superlogical.terminal.shell", label = label, options = {{ cwd = cwd, initial_input = cmd .. "\n" }} }}
local w, err = rex.call("session.new_window", {{ session_id = sid, window_label = label, layout = layout }})
if err ~= nil then return {{ ok = false, err = tostring(err) }} end
return {{ ok = true, window_id = w.window_id }}
"#,
        cwd = lua_string(&repo.display().to_string()),
        sid = lua_string(&session),
        cmd = lua_string(&command),
        label = lua_string(&label),
    );
    let out = run_with_stdin("rex", &["do", "-"], &script)?;
    let value: serde_json::Value =
        serde_json::from_slice(&out).unwrap_or(serde_json::Value::Null);
    if value.get("ok").and_then(|ok| ok.as_bool()) != Some(true) {
        return Err(anyhow!(
            "new tab failed: {}",
            value["err"].as_str().unwrap_or("unknown error")
        ));
    }
    Ok(format!("opened new tab: {} ({})", agent_name, file.display()))
}

#[derive(Clone, Copy, Debug)]
pub enum EditorPlacement {
    Tab,
    Float,
}

pub fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

pub fn open_editor(
    repo: &Path,
    path: &str,
    line: usize,
    placement: EditorPlacement,
    editor: &str,
) -> Result<std::path::PathBuf> {
    let session = std::env::var("REX_SESSION").unwrap_or_default();
    if session.is_empty() && std::env::var_os("TMUX").is_none() {
        return Err(anyhow!(
            "editor open needs Rex or tmux; use the inbox/clipboard targets instead"
        ));
    }
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0);
    let marker = std::env::temp_dir().join(format!("diffnote-edit-{}.done", stamp));
    let file = repo.join(path);
    let nvim = std::env::var("DIFFNOTE_NVIM").unwrap_or_else(|_| editor.to_string());
    let command = format!(
        "{} +{} {}; touch {}; exit",
        shell_quote(&nvim),
        line,
        shell_quote(&file.display().to_string()),
        shell_quote(&marker.display().to_string())
    );
    if session.is_empty() {
        Command::new("tmux")
            .args([
                "new-window",
                "-c",
                &repo.display().to_string(),
                "-n",
                "diffnote nvim",
                &command,
            ])
            .status()?;
        return Ok(marker);
    }
    let variant = match placement {
        EditorPlacement::Tab => "tab",
        EditorPlacement::Float => "float",
    };
    let script = format!(
        r#"
local sid = {sid}
local cwd = {cwd}
local cmd = {cmd}
local variant = {variant}
local self_block = os.getenv("REX_BLOCK") or ""
local layout = rex.layout.block{{ flavor = "com.superlogical.terminal.shell", label = "diffnote nvim", options = {{ cwd = cwd, initial_input = cmd .. "\n" }} }}
if variant == "float" then
  local window_id = nil
  local ok, blocks = pcall(rex.call, "session.list_blocks", {{ session_id = sid }})
  for _, b in ipairs(ok and type(blocks) == "table" and blocks.blocks or {{}}) do
    if b.block_id == self_block then window_id = b.window_id end
  end
  if not window_id then return {{ ok = false, err = "no window for float" }} end
  local _, err = rex.call("session.new_layer", {{ session_id = sid, window_id = window_id, bounds = {{ 0, 0, 1, 1 }}, focus = true, layout = layout }})
  if err ~= nil then return {{ ok = false, err = tostring(err) }} end
  return {{ ok = true }}
end
local w, err = rex.call("session.new_window", {{ session_id = sid, window_label = "diffnote nvim", layout = layout }})
if err ~= nil then return {{ ok = false, err = tostring(err) }} end
return {{ ok = true, window_id = w.window_id }}
"#,
        sid = lua_string(&session),
        cwd = lua_string(&repo.display().to_string()),
        cmd = lua_string(&command),
        variant = lua_string(variant),
    );
    let out = run_with_stdin("rex", &["do", "-"], &script)?;
    let value: serde_json::Value =
        serde_json::from_slice(&out).unwrap_or(serde_json::Value::Null);
    if value.get("ok").and_then(|ok| ok.as_bool()) != Some(true) {
        return Err(anyhow!(
            "editor launch failed: {}",
            value["err"].as_str().unwrap_or("unknown error")
        ));
    }
    Ok(marker)
}

pub fn send(
    target: &Target,
    payload: &str,
    mode: SendMode,
    repo: &Path,
    config: &crate::config::Config,
) -> Result<String> {
    let wrapped = format!("\x1b[200~{}\x1b[201~", payload);
    match target {
        Target::Rex { session, block } => {
            rex_write(session, block, &wrapped)?;
            if mode == SendMode::Submit {
                rex_write(session, block, "\r")?;
            }
            Ok(format!("sent to Rex block {}", block))
        }
        Target::Tmux { pane } => {
            run_with_stdin("tmux", &["load-buffer", "-b", "diffnote", "-"], &wrapped)?;
            Command::new("tmux")
                .args([
                    "paste-buffer",
                    "-p",
                    "-d",
                    "-b",
                    "diffnote",
                    "-t",
                    pane.as_str(),
                ])
                .status()?;
            if mode == SendMode::Submit {
                Command::new("tmux")
                    .args(["send-keys", "-t", pane.as_str(), "Enter"])
                    .status()?;
            }
            Ok(format!("sent to tmux pane {}", pane))
        }
        Target::NewTab => spawn_new_tab(
            repo,
            payload,
            &config.new_tab_command,
            &config.new_tab_agent,
        ),
        Target::Inbox => {
            let path = inbox_file(repo, payload)?;
            Ok(format!("wrote {}", path.display()))
        }
        Target::Clipboard => {
            copy_to_clipboard(&config.clipboard_command, payload)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::render_template;

    #[test]
    fn template_expands_file_placeholder() {
        let path = std::path::Path::new("/tmp/notes with space.md");
        assert_eq!(
            render_template("pi -- @{file} \"go\"", path),
            "pi -- @'/tmp/notes with space.md' \"go\""
        );
    }

    #[test]
    fn template_leaves_other_braces() {
        assert_eq!(
            render_template("claude {file}", std::path::Path::new("/a.md")),
            "claude '/a.md'"
        );
    }
}
