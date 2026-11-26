use anyhow::{anyhow, Result};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum Section {
    Staged,
    Unstaged,
    Untracked,
    Review,
}

impl Section {
    pub fn tag(self) -> &'static str {
        match self {
            Section::Staged => "staged",
            Section::Unstaged => "unstaged",
            Section::Untracked => "untracked",
            Section::Review => "review",
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            Section::Staged => "Staged",
            Section::Unstaged => "Unstaged",
            Section::Untracked => "Untracked",
            Section::Review => "Review",
        }
    }
}

#[derive(Clone, Debug)]
pub struct FileEntry {
    pub path: String,
    pub section: Section,
    pub status: String,
}

#[derive(Clone, Debug)]
pub struct Snapshot {
    pub files: Vec<FileEntry>,
    pub head: String,
}

fn run(repo: &Path, args: &[&str]) -> Result<String> {
    let output = Command::new("git")
        .args(args)
        .current_dir(repo)
        .output()?;
    if !output.status.success() {
        return Err(anyhow!(
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

pub fn root_in(repo: &Path) -> Result<PathBuf> {
    Ok(PathBuf::from(
        run(repo, &["rev-parse", "--show-toplevel"])?.trim().to_string(),
    ))
}

pub fn status_lines(repo: &Path) -> Vec<String> {
    run(repo, &["status", "--porcelain=v1"])
        .map(|text| text.lines().map(str::to_string).collect())
        .unwrap_or_default()
}

pub fn snapshot(repo: &Path) -> Result<Snapshot> {
    let mut files = Vec::new();
    for line in status_lines(repo) {
        if line.len() < 4 {
            continue;
        }
        let status = line[..2].to_string();
        let raw_path = &line[3..];
        let path = if let Some((_, renamed)) = raw_path.split_once(" -> ") {
            renamed.trim_matches('"').to_string()
        } else {
            raw_path.trim_matches('"').to_string()
        };
        if status == "??" {
            files.push(FileEntry {
                path,
                section: Section::Untracked,
                status,
            });
            continue;
        }
        let bytes = status.as_bytes();
        let x = bytes[0] as char;
        let y = bytes[1] as char;
        if x != ' ' && x != '?' {
            files.push(FileEntry {
                path: path.clone(),
                section: Section::Staged,
                status: status.clone(),
            });
        }
        if y != ' ' && y != '?' {
            files.push(FileEntry {
                path,
                section: Section::Unstaged,
                status,
            });
        }
    }
    files.sort_by(|a, b| {
        (a.section as usize, a.path.as_str()).cmp(&(b.section as usize, b.path.as_str()))
    });
    let head = run(repo, &["rev-parse", "--short", "HEAD"])
        .unwrap_or_else(|_| "no-head".to_string())
        .trim()
        .to_string();
    Ok(Snapshot { files, head })
}

pub fn content_hash(repo: &Path, path: &str, section: Section) -> String {
    let text = match section {
        Section::Staged => {
            let spec = format!(":{}", path);
            run(repo, &["show", spec.as_str()]).unwrap_or_default()
        }
        _ => std::fs::read_to_string(repo.join(path)).unwrap_or_default(),
    };
    let mut hasher = DefaultHasher::new();
    text.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

pub fn tree_token(repo: &Path) -> String {
    let mut hasher = DefaultHasher::new();
    for line in status_lines(repo) {
        line.hash(&mut hasher);
        if line.len() >= 4 {
            let path = line[3..].split_once(" -> ").map(|(_, p)| p).unwrap_or(&line[3..]);
            if let Ok(bytes) = std::fs::read(repo.join(path.trim_matches('"'))) {
                bytes.hash(&mut hasher);
            }
        }
    }
    format!("{:016x}", hasher.finish())
}

pub fn has_conflicts(repo: &Path) -> bool {
    status_lines(repo).iter().any(|line| {
        line.len() >= 2
            && (line[..2].contains('U') || &line[..2] == "AA" || &line[..2] == "DD")
    })
}

pub fn stage_file(repo: &Path, path: &str) -> Result<()> {
    run(repo, &["add", "--", path]).map(|_| ())
}

pub fn unstage_file(repo: &Path, path: &str) -> Result<()> {
    run(repo, &["restore", "--staged", "--", path]).map(|_| ())
}

pub fn stage_patch(repo: &Path, patch: &str, recount: bool) -> Result<()> {
    let mut args = vec!["apply", "--cached"];
    if recount {
        args.push("--recount");
    }
    args.push("-");
    let mut child = Command::new("git")
        .args(&args)
        .current_dir(repo)
        .stdin(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(patch.as_bytes())?;
    }
    let output = child.wait_with_output()?;
    if !output.status.success() {
        return Err(anyhow!(
            "git apply --cached failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(())
}

pub fn stage_hunk(repo: &Path, patch: &str) -> Result<()> {
    stage_patch(repo, patch, false)
}

pub fn stage_all(repo: &Path) -> Result<()> {
    run(repo, &["add", "-A"]).map(|_| ())
}

pub fn unstage_all(repo: &Path) -> Result<()> {
    match run(repo, &["rev-parse", "--verify", "HEAD"]) {
        Ok(_) => run(repo, &["reset", "HEAD", "--"]).map(|_| ()),
        Err(_) => run(repo, &["rm", "-r", "--cached", "."]).map(|_| ()),
    }
}

#[derive(Clone, Debug)]
pub struct CommitEntry {
    pub sha: String,
    pub subject: String,
    pub author: String,
    pub date: String,
}

#[derive(Clone, Debug, Default)]
pub struct BranchState {
    pub current: String,
    pub base: String,
    pub base_exists: bool,
    pub ahead: usize,
    pub behind: usize,
    pub updated: bool,
}

pub fn list_branches(repo: &Path) -> Result<Vec<String>> {
    let text = run(
        repo,
        &[
            "for-each-ref",
            "--format=%(refname:short)",
            "--sort=refname",
            "refs/heads/",
        ],
    )?;
    Ok(text
        .lines()
        .map(str::to_string)
        .filter(|name| !name.is_empty())
        .collect())
}

pub fn rename_branch(repo: &Path, name: &str) -> Result<String> {
    run(repo, &["branch", "-m", name])
}

pub fn pull_branch(repo: &Path) -> Result<String> {
    let output = Command::new("git")
        .args(["pull"])
        .current_dir(repo)
        .output()?;
    let mut text = String::from_utf8_lossy(&output.stdout).into_owned();
    text.push_str(&String::from_utf8_lossy(&output.stderr));
    if !output.status.success() {
        return Err(anyhow!("{}", text.trim()));
    }
    Ok(text)
}

pub fn push_branch(repo: &Path, force_with_lease: bool) -> Result<String> {
    let mut args = vec!["push", "-u", "origin", "HEAD"];
    if force_with_lease {
        args.push("--force-with-lease");
    }
    let output = Command::new("git")
        .args(&args)
        .current_dir(repo)
        .output()?;
    let mut text = String::from_utf8_lossy(&output.stdout).into_owned();
    text.push_str(&String::from_utf8_lossy(&output.stderr));
    if !output.status.success() {
        return Err(anyhow!("{}", text.trim()));
    }
    Ok(text)
}

pub fn is_detached(repo: &Path) -> bool {
    run(repo, &["symbolic-ref", "-q", "--short", "HEAD"]).is_err()
}

pub fn branch_state(repo: &Path, base: &str) -> BranchState {
    let current = run(repo, &["symbolic-ref", "--short", "HEAD"])
        .unwrap_or_default()
        .trim()
        .to_string();
    let mut state = BranchState {
        current,
        base: base.to_string(),
        ..Default::default()
    };
    let base_ref = format!("refs/heads/{}", base);
    if run(repo, &["show-ref", "--verify", "--quiet", &base_ref]).is_err() {
        return state;
    }
    state.base_exists = true;
    let Ok(count) = run(
        repo,
        &[
            "rev-list",
            "--left-right",
            "--count",
            &format!("{}...HEAD", base),
        ],
    ) else {
        return state;
    };
    let parts: Vec<&str> = count.split_whitespace().collect();
    if parts.len() >= 2 {
        state.behind = parts[0].parse().unwrap_or(0);
        state.ahead = parts[1].parse().unwrap_or(0);
    }
    state.updated = state.behind == 0;
    state
}

pub fn run_shell(repo: &Path, command: &str) -> Result<String> {
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string());
    let output = Command::new(shell)
        .args(["-ic", command])
        .current_dir(repo)
        .output()?;
    let mut text = String::from_utf8_lossy(&output.stdout).into_owned();
    text.push_str(&String::from_utf8_lossy(&output.stderr));
    if !output.status.success() {
        return Err(anyhow!("{}", text.trim()));
    }
    Ok(text)
}

pub fn log(repo: &Path, limit: usize) -> Result<Vec<CommitEntry>> {
    let text = run(
        repo,
        &[
            "log",
            &format!("-{}", limit),
            "--format=%h%x1f%s%x1f%an%x1f%ad",
            "--date=short",
        ],
    )?;
    Ok(text
        .lines()
        .filter_map(|line| {
            let fields: Vec<&str> = line.split('\x1f').collect();
            if fields.len() < 4 {
                return None;
            }
            Some(CommitEntry {
                sha: fields[0].to_string(),
                subject: fields[1].to_string(),
                author: fields[2].to_string(),
                date: fields[3].to_string(),
            })
        })
        .collect())
}

pub fn ensure_excluded(repo: &Path) {
    let Ok(output) = Command::new("git")
        .args(["rev-parse", "--git-path", "info/exclude"])
        .current_dir(repo)
        .output()
    else {
        return;
    };
    if !output.status.success() {
        return;
    }
    let raw = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if raw.is_empty() {
        return;
    }
    let path = if Path::new(&raw).is_absolute() {
        std::path::PathBuf::from(raw)
    } else {
        repo.join(raw)
    };
    let text = std::fs::read_to_string(&path).unwrap_or_default();
    if text.lines().any(|line| line.trim() == ".diffnote/") {
        return;
    }
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let mut next = text;
    if !next.is_empty() && !next.ends_with('\n') {
        next.push('\n');
    }
    next.push_str("\n# diffnote local state\n.diffnote/\n");
    let _ = std::fs::write(path, next);
}

pub fn worktree_line(repo: &Path, path: &str, index_line: usize) -> usize {
    let Ok(raw) = run(
        repo,
        &["diff", "--no-ext-diff", "--unified=0", "--", path],
    ) else {
        return index_line;
    };
    let mut offset: isize = 0;
    for line in raw.lines() {
        let Some(header) = line.strip_prefix("@@") else {
            continue;
        };
        let tokens: Vec<&str> = header.split_whitespace().collect();
        let Some(old) = tokens.first() else { continue };
        let Some(new) = tokens.get(1) else { continue };
        let old_start = old
            .trim_start_matches('-')
            .split(',')
            .next()
            .and_then(|value| value.parse::<usize>().ok());
        let old_count = old
            .trim_start_matches('-')
            .split(',')
            .nth(1)
            .and_then(|value| value.parse::<isize>().ok())
            .unwrap_or(1);
        let new_count = new
            .trim_start_matches('+')
            .split(',')
            .nth(1)
            .and_then(|value| value.parse::<isize>().ok())
            .unwrap_or(1);
        let Some(start) = old_start else { continue };
        let applies = if old_count == 0 {
            start < index_line
        } else {
            start + old_count.max(0) as usize <= index_line
        };
        if applies {
            offset += new_count - old_count;
        }
    }
    ((index_line as isize) + offset).max(1) as usize
}

pub fn commit_diff(repo: &Path, sha: &str) -> Result<String> {
    run(
        repo,
        &[
            "show",
            "--format=",
            "--no-ext-diff",
            "--unified=3",
            sha,
        ],
    )
}

pub fn diff_text(repo: &Path, path: &str, section: Section, context: usize) -> Result<String> {
    if section == Section::Untracked {
        let content = std::fs::read_to_string(repo.join(path)).unwrap_or_default();
        let count = content.lines().count().max(1);
        let mut patch = format!(
            "diff --git a/{p} b/{p}\nnew file mode 100644\n--- /dev/null\n+++ b/{p}\n@@ -0,0 +1,{n} @@\n",
            p = path,
            n = count
        );
        for line in content.lines() {
            patch.push('+');
            patch.push_str(line);
            patch.push('\n');
        }
        return Ok(patch);
    }
    let mut args: Vec<String> = vec!["diff".to_string()];
    if section == Section::Staged {
        args.push("--cached".to_string());
    }
    args.push("--no-ext-diff".to_string());
    args.push(format!("--unified={}", context));
    args.push("--".to_string());
    args.push(path.to_string());
    let refs: Vec<&str> = args.iter().map(String::as_str).collect();
    run(repo, &refs)
}

pub fn stash_list(repo: &Path) -> Vec<(String, String)> {
    let Ok(text) = run(
        repo,
        &["stash", "list", "--format=%gd%x1f%gs"],
    ) else {
        return Vec::new();
    };
    text.lines()
        .filter_map(|line| {
            let mut parts = line.splitn(2, '\u{1f}');
            let reference = parts.next()?.trim().to_string();
            let message = parts.next().unwrap_or("").trim().to_string();
            Some((reference, message))
        })
        .collect()
}

pub fn stash_push(repo: &Path, title: &str) -> Result<String> {
    let message = if title.trim().is_empty() {
        "diffnote stash".to_string()
    } else {
        title.trim().to_string()
    };
    run(repo, &["stash", "push", "-u", "-m", &message])
}

pub fn stash_pop(repo: &Path, reference: &str) -> Result<String> {
    run(repo, &["stash", "pop", reference])
}

pub fn commit(repo: &Path, message: &str) -> Result<String> {
    run(repo, &["commit", "-m", message])
}

#[cfg(test)]
mod tests {
    use super::{run, stage_patch, worktree_line};
    use std::path::Path;
    use std::process::Command;

    fn scratch() -> std::path::PathBuf {
        let path = std::env::temp_dir().join(format!("dn-map-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).unwrap();
        Command::new("git")
            .args(["init", "-q"])
            .current_dir(&path)
            .status()
            .unwrap();
        Command::new("git")
            .args(["config", "user.email", "t@t"])
            .current_dir(&path)
            .status()
            .unwrap();
        Command::new("git")
            .args(["config", "user.name", "t"])
            .current_dir(&path)
            .status()
            .unwrap();
        std::fs::write(path.join("a.txt"), "l1\nl2\nl3\nl4\nl5\nl6\nl7\nl8\n").unwrap();
        Command::new("git")
            .args(["add", "."])
            .current_dir(&path)
            .status()
            .unwrap();
        Command::new("git")
            .args(["commit", "-qm", "init"])
            .current_dir(&path)
            .status()
            .unwrap();
        std::fs::write(
            path.join("a.txt"),
            "l1\nNEW-A\nNEW-B\nl2\nl3\nl4\nl5\nl6\nl7\nl8\n",
        )
        .unwrap();
        Command::new("git")
            .args(["add", "a.txt"])
            .current_dir(&path)
            .status()
            .unwrap();
        std::fs::write(
            path.join("a.txt"),
            "l1\nNEW-A\nNEW-B\nl2\nEXTRA\nl3\nl4\nl5\nl6\nl7\nl8\n",
        )
        .unwrap();
        path
    }

    #[test]
    fn maps_staged_line_past_an_insertion() {
        let repo = scratch();
        assert_eq!(worktree_line(&repo, "a.txt", 1), 1);
        assert_eq!(worktree_line(&repo, "a.txt", 4), 4);
        assert_eq!(worktree_line(&repo, "a.txt", 5), 6);
        assert_eq!(worktree_line(&repo, "a.txt", 10), 11);
        let _ = std::fs::remove_dir_all(&repo);
    }

    #[test]
    fn selection_patch_applies_only_selected_lines() {
        let repo = std::env::temp_dir().join(format!("dn-sel-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&repo);
        std::fs::create_dir_all(&repo).unwrap();
        for args in [
            vec!["init", "-q"],
            vec!["config", "user.email", "t@t"],
            vec!["config", "user.name", "t"],
        ] {
            Command::new("git").args(&args).current_dir(&repo).status().unwrap();
        }
        std::fs::write(repo.join("a.txt"), "l1\nl2\nl3\n").unwrap();
        Command::new("git").args(["add", "."]).current_dir(&repo).status().unwrap();
        Command::new("git").args(["commit", "-qm", "init"]).current_dir(&repo).status().unwrap();
        std::fs::write(repo.join("a.txt"), "l1\nNEW-A\nNEW-B\nl2\n").unwrap();
        let raw = run(&repo, &["diff", "--no-ext-diff", "--unified=3", "--", "a.txt"]).unwrap();
        let doc = crate::diff::parse(&raw, "a.txt");
        let remove_index = doc.lines.iter().position(|line| line.kind == crate::diff::LineKind::Remove).unwrap();
        let add_index = doc.lines.iter().position(|line| line.kind == crate::diff::LineKind::Add).unwrap();
        let patch = crate::diff::patch_for_selection(&doc, remove_index, add_index).unwrap();
        stage_patch(&repo, &patch, true).unwrap();
        let staged = run(&repo, &["show", ":a.txt"]).unwrap();
        assert!(staged.contains("NEW-A"), "{}", staged);
        let _ = std::fs::remove_dir_all(&repo);
    }

    #[test]
    fn maps_untracked_path_unchanged() {
        let path = Path::new("/nonexistent");
        assert_eq!(worktree_line(path, "a.txt", 7), 7);
    }
}
