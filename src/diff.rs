#[derive(Clone, Copy, Debug, PartialEq)]
pub enum LineKind {
    Header,
    Hunk,
    Context,
    Add,
    Remove,
}

#[derive(Clone, Debug)]
pub struct DiffLine {
    pub kind: LineKind,
    pub old_no: Option<usize>,
    pub new_no: Option<usize>,
    pub text: String,
    pub hunk: usize,
}

#[derive(Clone, Debug)]
pub struct Hunk {
    pub header: String,
    pub start: usize,
    pub end: usize,
}

#[derive(Clone, Debug)]
pub struct DiffDoc {
    pub path: String,
    pub lines: Vec<DiffLine>,
    pub hunks: Vec<Hunk>,
}

fn hunk_numbers(line: &str) -> (usize, usize) {
    let tokens: Vec<&str> = line.split_whitespace().collect();
    let old = tokens
        .get(1)
        .and_then(|token| token.trim_start_matches('-').split(',').next())
        .and_then(|number| number.parse().ok())
        .unwrap_or(1);
    let new = tokens
        .get(2)
        .and_then(|token| token.trim_start_matches('+').split(',').next())
        .and_then(|number| number.parse().ok())
        .unwrap_or(1);
    (old, new)
}

fn is_header(line: &str) -> bool {
    line.starts_with("diff ")
        || line.starts_with("index ")
        || line.starts_with("---")
        || line.starts_with("+++")
        || line.starts_with("new file")
        || line.starts_with("deleted file")
        || line.starts_with("similarity")
        || line.starts_with("rename ")
        || line.starts_with("old mode")
        || line.starts_with("new mode")
}

pub fn parse(raw: &str, path: &str) -> DiffDoc {
    let mut lines: Vec<DiffLine> = Vec::new();
    let mut hunks: Vec<Hunk> = Vec::new();
    let mut old_no = 1usize;
    let mut new_no = 1usize;
    for raw_line in raw.lines() {
        let hunk = hunks.len().saturating_sub(1);
        if raw_line.starts_with("@@") {
            if let Some(last) = hunks.last_mut() {
                last.end = lines.len();
            }
            let (old, new) = hunk_numbers(raw_line);
            old_no = old;
            new_no = new;
            hunks.push(Hunk {
                header: raw_line.to_string(),
                start: lines.len(),
                end: lines.len(),
            });
            lines.push(DiffLine {
                kind: LineKind::Hunk,
                old_no: None,
                new_no: None,
                text: raw_line.to_string(),
                hunk,
            });
        } else if is_header(raw_line) {
            lines.push(DiffLine {
                kind: LineKind::Header,
                old_no: None,
                new_no: None,
                text: raw_line.to_string(),
                hunk,
            });
        } else if let Some(rest) = raw_line.strip_prefix('+') {
            lines.push(DiffLine {
                kind: LineKind::Add,
                old_no: None,
                new_no: Some(new_no),
                text: rest.to_string(),
                hunk,
            });
            new_no += 1;
        } else if let Some(rest) = raw_line.strip_prefix('-') {
            lines.push(DiffLine {
                kind: LineKind::Remove,
                old_no: Some(old_no),
                new_no: None,
                text: rest.to_string(),
                hunk,
            });
            old_no += 1;
        } else {
            let rest = raw_line.strip_prefix(' ').unwrap_or(raw_line);
            lines.push(DiffLine {
                kind: LineKind::Context,
                old_no: Some(old_no),
                new_no: Some(new_no),
                text: rest.to_string(),
                hunk,
            });
            old_no += 1;
            new_no += 1;
        }
    }
    if let Some(last) = hunks.last_mut() {
        last.end = lines.len();
    }
    if lines.is_empty() {
        lines.push(DiffLine {
            kind: LineKind::Header,
            old_no: None,
            new_no: None,
            text: "no diff".to_string(),
            hunk: 0,
        });
    }
    DiffDoc {
        path: path.to_string(),
        lines,
        hunks,
    }
}

pub fn patch_for_hunk(doc: &DiffDoc, index: usize) -> Option<String> {
    let hunk = doc.hunks.get(index)?;
    let mut patch = format!(
        "diff --git a/{p} b/{p}\n--- a/{p}\n+++ b/{p}\n",
        p = doc.path
    );
    patch.push_str(&hunk.header);
    patch.push('\n');
    for line in &doc.lines[hunk.start..hunk.end] {
        match line.kind {
            LineKind::Header | LineKind::Hunk => continue,
            LineKind::Context => patch.push(' '),
            LineKind::Add => patch.push('+'),
            LineKind::Remove => patch.push('-'),
        }
        patch.push_str(&line.text);
        patch.push('\n');
    }
    Some(patch)
}

#[derive(Clone, Debug)]
pub struct SplitCell {
    pub kind: LineKind,
    pub number: Option<usize>,
    pub text: String,
}

#[derive(Clone, Debug)]
pub struct SplitRow {
    pub left: Option<SplitCell>,
    pub right: Option<SplitCell>,
    pub doc_index: usize,
}

fn cell(line: &DiffLine) -> SplitCell {
    SplitCell {
        kind: line.kind,
        number: line.new_no.or(line.old_no),
        text: line.text.clone(),
    }
}

fn flush_rows(
    rows: &mut Vec<SplitRow>,
    removes: &mut Vec<(usize, SplitCell)>,
    adds: &mut Vec<(usize, SplitCell)>,
) {
    let count = removes.len().max(adds.len());
    for index in 0..count {
        let left = removes.get(index);
        let right = adds.get(index);
        rows.push(SplitRow {
            left: left.map(|(_, cell)| cell.clone()),
            right: right.map(|(_, cell)| cell.clone()),
            doc_index: left
                .or(right)
                .map(|(index, _)| *index)
                .unwrap_or(0),
        });
    }
    removes.clear();
    adds.clear();
}

pub fn split_rows(doc: &DiffDoc) -> Vec<SplitRow> {
    let mut rows = Vec::new();
    let mut removes: Vec<(usize, SplitCell)> = Vec::new();
    let mut adds: Vec<(usize, SplitCell)> = Vec::new();
    for (index, line) in doc.lines.iter().enumerate() {
        match line.kind {
            LineKind::Header => continue,
            LineKind::Hunk => {
                flush_rows(&mut rows, &mut removes, &mut adds);
                rows.push(SplitRow {
                    left: Some(cell(line)),
                    right: None,
                    doc_index: index,
                });
            }
            LineKind::Context => {
                flush_rows(&mut rows, &mut removes, &mut adds);
                rows.push(SplitRow {
                    left: Some(cell(line)),
                    right: Some(cell(line)),
                    doc_index: index,
                });
            }
            LineKind::Remove => removes.push((index, cell(line))),
            LineKind::Add => adds.push((index, cell(line))),
        }
    }
    flush_rows(&mut rows, &mut removes, &mut adds);
    rows
}

pub fn patch_for_selection(doc: &DiffDoc, start: usize, end: usize) -> Option<String> {
    let low = start.min(end);
    let high = start.max(end);
    let mut patch = format!(
        "diff --git a/{p} b/{p}\n--- a/{p}\n+++ b/{p}\n",
        p = doc.path
    );
    let mut used = false;
    for hunk in &doc.hunks {
        if hunk.start > high || hunk.end <= low {
            continue;
        }
        let mut changed = false;
        let mut body = String::new();
        for (index, line) in doc.lines.iter().enumerate().take(hunk.end).skip(hunk.start) {
            let chosen = (low..=high).contains(&index);
            let marker = match line.kind {
                LineKind::Header | LineKind::Hunk => continue,
                LineKind::Context => ' ',
                LineKind::Add => {
                    if chosen {
                        changed = true;
                        '+'
                    } else {
                        continue;
                    }
                }
                LineKind::Remove => {
                    if chosen {
                        changed = true;
                        '-'
                    } else {
                        ' '
                    }
                }
            };
            body.push(marker);
            body.push_str(&line.text);
            body.push('\n');
        }
        if !changed || body.is_empty() {
            continue;
        }
        let (old_start, new_start) = hunk_numbers(&hunk.header);
        patch.push_str(&format!("@@ -{},0 +{},0 @@\n", old_start, new_start));
        patch.push_str(&body);
        used = true;
    }
    if used {
        Some(patch)
    } else {
        None
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> DiffDoc {
        parse(
            "diff --git a/a.txt b/a.txt\n--- a/a.txt\n+++ b/a.txt\n@@ -1,3 +1,3 @@\n one\n-two\n+three\n end\n",
            "a.txt",
        )
    }

    #[test]
    fn selection_keeps_only_chosen_lines() {
        let doc = sample();
        let remove_index = doc
            .lines
            .iter()
            .position(|line| line.kind == LineKind::Remove)
            .unwrap();
        let patch = patch_for_selection(&doc, remove_index, remove_index + 1).unwrap();
        assert!(patch.contains("+three"));
        assert!(patch.contains("-two"));
        assert_eq!(patch.lines().filter(|line| line.starts_with("@@")).count(), 1);
    }

}
