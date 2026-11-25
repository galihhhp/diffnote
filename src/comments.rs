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
}

#[derive(Default, Debug, serde::Serialize, serde::Deserialize)]
pub struct CommentStore {
    pub items: Vec<Comment>,
    next_id: u64,
}

impl CommentStore {
    pub fn add(
        &mut self,
        path: String,
        section: Section,
        side: Side,
        line_no: usize,
        line_text: String,
        content_hash: String,
        text: String,
    ) {
        self.next_id += 1;
        self.items.push(Comment {
            id: self.next_id,
            path,
            section,
            side,
            line_no,
            line_text,
            content_hash,
            text,
            sent: false,
            orphan: false,
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
            .filter(|comment| comment.path == path && comment.line_no == line_no)
            .collect()
    }

    pub fn payload(&self, repo: &Path) -> String {
        let mut out = format!("diffnote review\nrepo: {}\n\n", repo.display());
        for comment in self.unsent() {
            out.push_str(&format!(
                "{}:{}-{} — {}\n",
                comment.path,
                comment.line_no,
                comment.side.tag(),
                comment.text
            ));
            if !comment.line_text.is_empty() {
                out.push_str(&format!("    | {}\n", comment.line_text));
            }
            if comment.orphan {
                out.push_str("    | (line moved or removed)\n");
            }
        }
        out
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
                    comment.line_no = line.new_no.or(line.old_no).unwrap_or(comment.line_no);
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
