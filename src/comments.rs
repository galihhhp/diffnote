use crate::diff::{DiffDoc, LineKind};
use crate::git::Section;
use std::collections::HashMap;
use std::path::Path;

#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum Side {
    Left,
    Right,
}

impl Side {
    pub fn tag(self) -> &'static str {
        match self {
            Side::Left => "left",
            Side::Right => "right",
        }
    }
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Comment {
    pub id: u64,
    pub path: String,
    pub section: Section,
    pub side: Side,
    pub line_no: usize,
    pub line_text: String,
    pub content_hash: String,
    pub text: String,
    pub sent: bool,
    pub orphan: bool,
    #[serde(default)]
    pub end_line: Option<usize>,
    #[serde(default)]
    pub code: Vec<String>,
}

impl Comment {
    pub fn label(&self) -> String {
        match self.end_line {
            Some(end) if end != self.line_no => format!("{}-{}", self.line_no, end),
            _ => self.line_no.to_string(),
        }
    }
}

pub struct Draft {
    pub path: String,
    pub section: Section,
    pub side: Side,
    pub line_no: usize,
    pub end_line: Option<usize>,
    pub line_text: String,
    pub code: Vec<String>,
    pub content_hash: String,
}

#[derive(Default, Debug, serde::Serialize, serde::Deserialize)]
pub struct CommentStore {
    pub items: Vec<Comment>,
    next_id: u64,
}

impl CommentStore {
    pub fn add(&mut self, draft: Draft, text: String) {
        self.next_id += 1;
        self.items.push(Comment {
            id: self.next_id,
            path: draft.path,
            section: draft.section,
            side: draft.side,
            line_no: draft.line_no,
            line_text: draft.line_text,
            content_hash: draft.content_hash,
            text,
            sent: false,
            orphan: false,
            end_line: draft.end_line,
            code: draft.code,
        });
    }

    pub fn unsent(&self) -> Vec<&Comment> {
        self.items.iter().filter(|comment| !comment.sent).collect()
    }

    pub fn mark_sent(&mut self) {
        for comment in &mut self.items {
            comment.sent = true;
        }
    }

    pub fn for_line(&self, path: &str, line_no: usize) -> Vec<&Comment> {
        self.items
            .iter()
            .filter(|comment| {
                comment.path == path
                    && (comment.line_no == line_no
                        || comment
                            .end_line
                            .is_some_and(|end| (comment.line_no.min(end)..=comment.line_no.max(end)).contains(&line_no)))
            })
            .collect()
    }

    pub fn payload(&self) -> String {
        let unsent = self.unsent();
        self.payload_from(&unsent)
    }

    pub fn payload_all(&self) -> String {
        let all: Vec<&Comment> = self.items.iter().collect();
        self.payload_from(&all)
    }

    fn payload_from(&self, comments: &[&Comment]) -> String {
        let mut blocks = vec!["Code review comments from my editor:".to_string()];
        for (index, comment) in comments.iter().enumerate() {
            let mut block = format!("{}. {}:{}", index + 1, comment.path, comment.label());
            if comment.code.is_empty() && !comment.line_text.is_empty() {
                block.push_str(&format!("\n   │ {}", comment.line_text));
            }
            for line in &comment.code {
                block.push_str(&format!("\n   │ {}", line));
            }
            let mut text = comment.text.lines();
            if let Some(first) = text.next() {
                block.push_str(&format!("\n   │ Comment: {}", first));
            }
            for line in text {
                block.push_str(&format!("\n   │ {}", line));
            }
            if comment.orphan {
                block.push_str("\n   │ (line moved or removed)");
            }
            blocks.push(block);
        }
        blocks.push("Make the requested changes and answer any questions. Reply per item.".to_string());
        format!("{}\n", blocks.join("\n\n"))
    }

    pub fn reanchor(
        &mut self,
        docs: &HashMap<String, DiffDoc>,
        hashes: &HashMap<String, String>,
    ) {
        for comment in &mut self.items {
            let key = format!("{}:{}", comment.path, comment.section.tag());
            let Some(new_hash) = hashes.get(&key) else {
                comment.orphan = true;
                continue;
            };
            if new_hash == &comment.content_hash {
                comment.orphan = false;
                continue;
            }
            let found = docs.get(&key).and_then(|doc| {
                doc.lines.iter().find(|line| {
                    line.text == comment.line_text
                        && line.kind != LineKind::Header
                        && line.kind != LineKind::Hunk
                })
            });
            match found {
                Some(line) => {
                    let next = line.new_no.or(line.old_no).unwrap_or(comment.line_no);
                    let delta = next as isize - comment.line_no as isize;
                    comment.line_no = next;
                    comment.end_line = comment
                        .end_line
                        .map(|end| ((end as isize) + delta).max(1) as usize);
                    comment.orphan = false;
                }
                None => comment.orphan = true,
            }
            comment.content_hash = new_hash.clone();
        }
    }

    pub fn save(&self, repo: &Path) -> std::io::Result<()> {
        let dir = repo.join(".diffnote");
        std::fs::create_dir_all(&dir)?;
        crate::git::ensure_excluded(repo);
        let text = serde_json::to_string_pretty(self).unwrap_or_default();
        std::fs::write(dir.join("comments.json"), text)
    }

    pub fn load(repo: &Path) -> CommentStore {
        let path = repo.join(".diffnote/comments.json");
        let legacy = repo.join(".reviewr/comments.json");
        let Ok(text) = std::fs::read_to_string(&path).or_else(|_| std::fs::read_to_string(&legacy)) else {
            return CommentStore::default();
        };
        let Ok(mut store) = serde_json::from_str::<CommentStore>(&text) else {
            return CommentStore::default();
        };
        store.next_id = store.items.iter().map(|item| item.id).max().unwrap_or(0);
        store
    }

    pub fn edit_text(&mut self, id: u64, text: String) -> bool {
        match self.items.iter_mut().find(|item| item.id == id) {
            Some(comment) => {
                comment.text = text;
                comment.sent = false;
                true
            }
            None => false,
        }
    }

    pub fn resend_all(&mut self) -> usize {
        let mut count = 0;
        for comment in &mut self.items {
            if comment.sent {
                comment.sent = false;
                count += 1;
            }
        }
        count
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn draft() -> Draft {
        Draft {
            path: "a.rs".to_string(),
            section: Section::Unstaged,
            side: Side::Right,
            line_no: 4,
            end_line: Some(6),
            line_text: "let x = 1;".to_string(),
            code: vec!["let x = 1;".to_string(), "let y = 2;".to_string()],
            content_hash: "abc".to_string(),
        }
    }

    #[test]
    fn range_comment_payload_includes_range_and_code() {
        let mut store = CommentStore::default();
        store.add(draft(), "rename this".to_string());
        let payload = store.payload();
        assert!(payload.starts_with("Code review comments from my editor:"));
        assert!(payload.contains("1. a.rs:4-6"));
        assert!(payload.contains("│ let y = 2;"));
        assert!(payload.contains("│ Comment: rename this"));
        assert!(payload.ends_with("Make the requested changes and answer any questions. Reply per item.\n"));
    }

    #[test]
    fn payload_all_includes_sent_comments() {
        let mut store = CommentStore::default();
        store.add(draft(), "note".to_string());
        store.mark_sent();
        assert!(!store.payload().contains("note"));
        assert!(store.payload_all().contains("note"));
    }

    #[test]
    fn for_line_matches_inside_range() {
        let mut store = CommentStore::default();
        store.add(draft(), "rename this".to_string());
        assert_eq!(store.for_line("a.rs", 5).len(), 1);
        assert_eq!(store.for_line("a.rs", 9).len(), 0);
    }

    #[test]
    fn legacy_json_without_new_fields_loads() {
        let raw = r#"{"items":[{"id":1,"path":"a.rs","section":"Unstaged","side":"Right","line_no":2,"line_text":"x","content_hash":"h","text":"note","sent":false,"orphan":false}],"next_id":1}"#;
        let store: CommentStore = serde_json::from_str(raw).unwrap();
        assert_eq!(store.items.len(), 1);
        assert_eq!(store.items[0].end_line, None);
        assert!(store.items[0].code.is_empty());
    }
}
