use crate::app::{App, Mode, View};
use crate::diff::{DiffDoc, LineKind, SplitCell, SplitRow};
use crate::git::Section;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Block, BorderType, Borders, Clear, List, ListItem, ListState, Paragraph,
};
use ratatui::Frame;

fn dialog_rect(width_pct: u16, height: u16, area: Rect) -> Rect {
    let width = (area.width * width_pct / 100).min(area.width);
    let height = height.min(area.height.saturating_sub(2));
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length((area.height.saturating_sub(height)) / 2),
            Constraint::Length(height),
            Constraint::Min(0),
        ])
        .split(area);
    let horizontal = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length((area.width.saturating_sub(width)) / 2),
            Constraint::Length(width),
            Constraint::Min(0),
        ])
        .split(vertical[1]);
    horizontal[1]
}

fn mode_accent(mode: &Mode) -> Color {
    match mode {
        Mode::CommentInput | Mode::CommitInput | Mode::PendingG => Color::Cyan,
        Mode::Confirm => Color::Yellow,
        Mode::TargetPicker => Color::Green,
        Mode::Help => Color::Magenta,
        Mode::FilePicker => Color::LightBlue,
        Mode::History => Color::LightYellow,
        Mode::DiffSearch => Color::LightCyan,
        Mode::CommentPanel => Color::LightMagenta,
        Mode::CommandOutput => Color::LightGreen,
        Mode::BranchInput => Color::LightCyan,
        Mode::BasePicker => Color::LightBlue,
        Mode::RenameInput => Color::LightYellow,
        Mode::Loading => Color::LightGreen,
        Mode::Visual => Color::LightYellow,
        Mode::StashPicker => Color::LightBlue,
        Mode::StashTitle => Color::LightCyan,
        Mode::Normal => Color::Gray,
    }
}

fn field_lines<'a>(value: &str, width: usize, accent: Color) -> Vec<Line<'a>> {
    let inner = width.saturating_sub(4).max(8);
    let text_width = inner.saturating_sub(2);
    let mut rows: Vec<String> = Vec::new();
    for raw in value.split('\n') {
        if raw.is_empty() {
            rows.push(String::new());
            continue;
        }
        let mut current = String::new();
        for character in raw.chars() {
            if current.chars().count() >= text_width {
                rows.push(current);
                current = String::new();
            }
            current.push(character);
        }
        rows.push(current);
    }
    if let Some(last) = rows.last_mut() {
        last.push('▌');
    }
    let mut lines = vec![Line::from(Span::styled(
        format!("╭{}╮", "─".repeat(inner)),
        Style::default().fg(accent),
    ))];
    for row in rows {
        lines.push(Line::from(vec![
            Span::styled("│", Style::default().fg(accent)),
            Span::styled(
                format!(" {:<width$} ", row, width = text_width),
                Style::default().fg(Color::White),
            ),
            Span::styled("│", Style::default().fg(accent)),
        ]));
    }
    lines.push(Line::from(Span::styled(
        format!("╰{}╯", "─".repeat(inner)),
        Style::default().fg(accent),
    )));
    lines
}

fn footer_hint<'a>(keys: &[(&str, &str)], accent: Color) -> Line<'a> {
    let mut spans = Vec::new();
    for (index, (key, label)) in keys.iter().enumerate() {
        if index > 0 {
            spans.push(Span::styled("   ", Style::default().fg(Color::DarkGray)));
        }
        spans.push(Span::styled(
            format!(" {} ", key),
            Style::default()
                .fg(accent)
                .add_modifier(Modifier::BOLD),
        ));
        spans.push(Span::styled(
            format!(" {}", label),
            Style::default().fg(Color::Gray),
        ));
    }
    Line::from(spans)
}
fn section_color(section: Section) -> Color {
    match section {
        Section::Staged => Color::Green,
        Section::Unstaged => Color::Yellow,
        Section::Unmerged => Color::LightRed,
        Section::Untracked => Color::Cyan,
        Section::Review => Color::LightMagenta,
    }
}

fn status_color(status: &str) -> Color {
    let code = status.trim();
    if code == "??" {
        return Color::Cyan;
    }
    if code.contains('U') {
        return Color::LightRed;
    }
    if code.contains('D') {
        return Color::Red;
    }
    if code.contains('A') {
        return Color::Green;
    }
    if code.contains('M') || code.contains('R') {
        return Color::Yellow;
    }
    Color::Gray
}

const SELECT_BG: Color = Color::Rgb(38, 50, 68);

fn selected_range(app: &App, index: usize) -> bool {
    if app.mode != Mode::Visual {
        return false;
    }
    app.visual_anchor
        .map(|anchor| (anchor.min(app.cursor)..=anchor.max(app.cursor)).contains(&index))
        .unwrap_or(false)
}

fn prefix_style(kind: LineKind) -> Style {
    match kind {
        LineKind::Add => Style::default()
            .fg(Color::LightGreen)
            .add_modifier(Modifier::BOLD),
        LineKind::Remove => Style::default()
            .fg(Color::LightRed)
            .add_modifier(Modifier::BOLD),
        LineKind::Hunk => Style::default().fg(Color::Cyan),
        LineKind::Header => Style::default().fg(Color::DarkGray),
        LineKind::Context => Style::default().fg(Color::DarkGray),
        LineKind::Conflict => Style::default()
            .fg(Color::LightYellow)
            .add_modifier(Modifier::BOLD),
    }
}

fn text_spans(
    app: &App,
    key: &str,
    index: usize,
    text: &str,
    style: Style,
    kind: LineKind,
) -> Vec<Span<'static>> {
    if kind == LineKind::Context {
        if let Some(tokens) = app.tokens.get(key).and_then(|lines| lines.get(index)) {
            if !tokens.is_empty() {
                return tokens
                    .iter()
                    .map(|(chunk, color)| Span::styled(chunk.clone(), style.fg(*color)))
                    .collect();
            }
        }
    }
    vec![Span::styled(text.to_string(), style)]
}

fn split_cell_line(
    app: &App,
    key: &str,
    doc_index: usize,
    cell: Option<&SplitCell>,
    highlight: bool,
    mark_count: usize,
) -> Line<'static> {
    let Some(cell) = cell else {
        return Line::from(Span::styled("~", Style::default().fg(Color::DarkGray)));
    };
    let prefix = match cell.kind {
        LineKind::Header => "  ",
        LineKind::Hunk => "",
        LineKind::Context => "  ",
        LineKind::Add => "+ ",
        LineKind::Remove => "- ",
        LineKind::Conflict => "! ",
    };
    let color = match cell.kind {
        LineKind::Header => Color::DarkGray,
        LineKind::Hunk => Color::Cyan,
        LineKind::Context => Color::Gray,
        LineKind::Add => Color::LightGreen,
        LineKind::Remove => Color::LightRed,
        LineKind::Conflict => Color::LightYellow,
    };
    let number = cell
        .number
        .map(|value| format!("{:>5} ", value))
        .unwrap_or_else(|| "      ".to_string());
    let badge = if mark_count > 0 {
        format!(
            "◆{}",
            if mark_count > 1 {
                mark_count.to_string()
            } else {
                " ".to_string()
            }
        )
    } else {
        "  ".to_string()
    };
    let badge_style = if mark_count > 0 {
        Style::default()
            .fg(Color::LightYellow)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default()
    };
    let mut style = Style::default().fg(color);
    if mark_count > 0 {
        style = style.fg(Color::LightYellow).add_modifier(Modifier::BOLD);
    }
    if selected_range(app, doc_index) {
        style = style.bg(SELECT_BG).add_modifier(Modifier::BOLD);
    }
    if highlight {
        style = style.add_modifier(Modifier::BOLD | Modifier::UNDERLINED);
    }
    let mut number_style = Style::default().fg(match cell.kind {
        LineKind::Add => Color::Green,
        LineKind::Remove => Color::Red,
        _ => Color::DarkGray,
    });
    if highlight || selected_range(app, doc_index) {
        number_style = number_style.add_modifier(Modifier::BOLD);
    }
    if selected_range(app, doc_index) {
        number_style = number_style.bg(SELECT_BG);
    }
    let mut prefix_span_style = prefix_style(cell.kind);
    if highlight {
        prefix_span_style = prefix_span_style.add_modifier(Modifier::BOLD | Modifier::UNDERLINED);
    }
    if selected_range(app, doc_index) {
        prefix_span_style = prefix_span_style.bg(SELECT_BG).add_modifier(Modifier::BOLD);
    }
    let mut spans = vec![
        Span::styled(badge, badge_style),
        Span::styled(number, number_style),
        Span::styled(prefix.to_string(), prefix_span_style),
    ];
    spans.extend(text_spans(app, key, doc_index, &cell.text, style, cell.kind));
    Line::from(spans)
}

fn split_panes(app: &App, doc: &DiffDoc, rows: &[SplitRow], key: &str) -> (Vec<Line<'static>>, Vec<Line<'static>>) {
    let mut left = Vec::new();
    let mut right = Vec::new();
    for row in rows {
        let highlight = row.doc_index == app.cursor;
        let number = row
            .left
            .as_ref()
            .or(row.right.as_ref())
            .and_then(|cell| cell.number)
            .unwrap_or(0);
        let mark_count = app
            .comments
            .for_line(&doc.path, number)
            .len();
        left.push(split_cell_line(app, key, row.doc_index, row.left.as_ref(), highlight, mark_count));
        right.push(split_cell_line(app, key, row.doc_index, row.right.as_ref(), highlight, mark_count));
    }
    (left, right)
}

fn diff_lines(app: &App, doc: &DiffDoc, key: &str) -> Vec<Line<'static>> {
    let hits: std::collections::HashSet<usize> =
        app.search_hits.iter().copied().collect();
    doc.lines
        .iter()
        .enumerate()
        .map(|(index, line)| {
            let prefix = match line.kind {
                LineKind::Header => "  ",
                LineKind::Hunk => "",
                LineKind::Context => "  ",
                LineKind::Add => "+ ",
                LineKind::Remove => "- ",
                LineKind::Conflict => "! ",
            };
            let color = match line.kind {
                LineKind::Header => Color::DarkGray,
                LineKind::Hunk => Color::Cyan,
                LineKind::Context => Color::Gray,
                LineKind::Add => Color::LightGreen,
                LineKind::Remove => Color::LightRed,
                LineKind::Conflict => Color::LightYellow,
            };
            let number = line
                .new_no
                .or(line.old_no)
                .map(|value| format!("{:>5} ", value))
                .unwrap_or_else(|| "      ".to_string());
            let mark_count = app
                .comments
                .for_line(&doc.path, line.new_no.or(line.old_no).unwrap_or(0))
                .len();
            let badge = if mark_count > 0 {
                format!(
                    "◆{}",
                    if mark_count > 1 {
                        mark_count.to_string()
                    } else {
                        " ".to_string()
                    }
                )
            } else {
                "  ".to_string()
            };
            let badge_style = if mark_count > 0 {
                Style::default()
                    .fg(Color::LightYellow)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            let mut style = Style::default().fg(color);
            if mark_count > 0 {
                style = style.fg(Color::LightYellow).add_modifier(Modifier::BOLD);
            }
            if hits.contains(&index) {
                style = style.add_modifier(Modifier::BOLD | Modifier::UNDERLINED);
            }
            let mut number_style = Style::default().fg(match line.kind {
                LineKind::Add => Color::Green,
                LineKind::Remove => Color::Red,
                _ => Color::DarkGray,
            });
            let mut prefix_span_style = prefix_style(line.kind);
            if index == app.cursor {
                style = style.add_modifier(Modifier::BOLD | Modifier::UNDERLINED);
                number_style = Style::default()
                    .fg(Color::Gray)
                    .add_modifier(Modifier::BOLD);
                prefix_span_style = prefix_style(line.kind)
                    .add_modifier(Modifier::BOLD | Modifier::UNDERLINED);
            }
            if selected_range(app, index) {
                style = style.bg(SELECT_BG).add_modifier(Modifier::BOLD);
                number_style = number_style.bg(SELECT_BG);
                prefix_span_style = prefix_span_style.bg(SELECT_BG);
            }
            let mut spans = vec![
                Span::styled(badge, badge_style),
                Span::styled(number, number_style),
                Span::styled(prefix.to_string(), prefix_span_style),
            ];
            spans.extend(text_spans(app, key, index, &line.text, style, line.kind));
            Line::from(spans)
        })
        .collect()
}

fn draw_files(frame: &mut Frame, app: &App, area: Rect) {
    let items: Vec<ListItem> = app
        .snapshot
        .files
        .iter()
        .enumerate()
        .map(|(index, entry)| {
            let selected = index == app.selected;
            let marker = if selected { ">" } else { " " };
            let color = section_color(entry.section);
            let comment_count = app
                .comments
                .items
                .iter()
                .filter(|comment| comment.path == entry.path)
                .count();
            let path_style = if selected {
                Style::default().fg(color).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(color)
            };
            let mut spans = vec![
                Span::styled(
                    marker,
                    if selected {
                        Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(Color::DarkGray)
                    },
                ),
                Span::raw(" "),
                Span::styled(
                    "▌",
                    Style::default().fg(color).add_modifier(Modifier::BOLD),
                ),
                Span::raw(" "),
                Span::styled(
                    format!("{:>2}", entry.status),
                    Style::default().fg(status_color(&entry.status)),
                ),
                Span::raw(" "),
                Span::styled(entry.path.clone(), path_style),
            ];
            if comment_count > 0 {
                spans.push(Span::styled(
                    format!("  ◆{}", comment_count),
                    Style::default()
                        .fg(Color::LightYellow)
                        .add_modifier(Modifier::BOLD),
                ));
            }
            ListItem::new(Line::from(spans))
        })
        .collect();
    let mut staged = 0usize;
    let mut unstaged = 0usize;
    let mut untracked = 0usize;
    let mut conflicts = 0usize;
    for entry in &app.snapshot.files {
        match entry.section {
            Section::Staged => staged += 1,
            Section::Unstaged => unstaged += 1,
            Section::Unmerged => conflicts += 1,
            Section::Untracked => untracked += 1,
            Section::Review => {}
        }
    }
    let title = if conflicts > 0 {
        format!("Files  +{} ~{} ?{} !{}", staged, unstaged, untracked, conflicts)
    } else {
        format!("Files  +{} ~{} ?{}", staged, unstaged, untracked)
    };
    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title(title))
        .highlight_style(
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
        );
    let mut state = ListState::default();
    state.select(Some(app.selected));
    frame.render_stateful_widget(list, area, &mut state);
}

fn draw_diff(frame: &mut Frame, app: &App, area: Rect) {
    let Some(doc) = app.current_doc() else {
        let empty = Paragraph::new("no changes")
            .block(Block::default().borders(Borders::ALL).title("Diff"));
        frame.render_widget(empty, area);
        return;
    };
    let section = match app.current_file() {
        Some(entry) if entry.section == Section::Unmerged => {
            format!("Conflicts · {}", crate::git::conflict_label(&entry.status))
        }
        Some(entry) => entry.section.title().to_string(),
        None => "Diff".to_string(),
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .title(if app.split {
            format!("{} [{}]  |  OLD  /  NEW", doc.path, section)
        } else {
            format!("{} [{}]", doc.path, section)
        });
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let key = app.current_file().map(crate::app::doc_key).unwrap_or_default();
    let mut offset = app.cursor.saturating_sub(inner.height as usize / 2) as u16;
    if app.split {
        let rows = crate::diff::split_rows(doc);
        let row_cursor = rows
            .iter()
            .position(|row| row.doc_index >= app.cursor)
            .unwrap_or(rows.len().saturating_sub(1));
        offset = row_cursor.saturating_sub(inner.height as usize / 2) as u16;
        let (left_lines, right_lines) = split_panes(app, doc, &rows, &key);
        let columns = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(inner);
        frame.render_widget(
            Block::default()
                .borders(Borders::RIGHT)
                .border_style(Style::default().fg(Color::DarkGray)),
            columns[0],
        );
        frame.render_widget(
            Paragraph::new(left_lines).scroll((offset, app.h_scroll)),
            columns[0],
        );
        frame.render_widget(
            Paragraph::new(right_lines).scroll((offset, app.h_scroll)),
            columns[1],
        );
    } else {
        frame.render_widget(
            Paragraph::new(diff_lines(app, doc, &key)).scroll((offset, 0)),
            inner,
        );
    }
}


fn draw_stashes(frame: &mut Frame, app: &App, area: Rect) {
    let focused = app.mode == Mode::StashPicker;
    let lines: Vec<Line<'static>> = if app.stashes.is_empty() {
        vec![Line::from(Span::styled(
            "no stashes",
            Style::default().fg(Color::DarkGray),
        ))]
    } else {
        app.stashes
            .iter()
            .enumerate()
            .map(|(index, (reference, message))| {
                let selected = focused && index == app.stash_pick;
                let marker = if selected { "▸" } else { " " };
                let style = if selected {
                    Style::default()
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD | Modifier::UNDERLINED)
                } else {
                    Style::default().fg(Color::Gray)
                };
                Line::from(vec![
                    Span::styled(
                        marker,
                        if selected {
                            Style::default().fg(Color::LightBlue)
                        } else {
                            Style::default().fg(Color::DarkGray)
                        },
                    ),
                    Span::raw(" "),
                    Span::styled(
                        format!("{} ", reference.trim_start_matches("stash@")),
                        Style::default().fg(Color::Cyan),
                    ),
                    Span::styled(message.clone(), style),
                ])
            })
            .collect()
    };
    let paragraph = Paragraph::new(lines).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(if focused {
                Color::LightBlue
            } else {
                Color::DarkGray
            }))
            .title(format!("Stash ({})", app.stashes.len())),
    );
    frame.render_widget(paragraph, area);
}

fn draw_comments(frame: &mut Frame, app: &App, area: Rect) {
    let focused = app.mode == Mode::CommentPanel;
    let lines: Vec<Line<'static>> = app
        .comments
        .items
        .iter()
        .enumerate()
        .map(|(index, comment)| {
            let state = if comment.sent {
                "sent"
            } else if comment.orphan {
                "orphan"
            } else {
                "unsent"
            };
            let selected = focused && index == app.comment_pick;
            let marker = if selected { "▸" } else { " " };
            let style = if selected {
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD | Modifier::UNDERLINED)
            } else {
                Style::default().fg(Color::Gray)
            };
            Line::from(vec![
                Span::styled(
                    marker,
                    if selected {
                        Style::default().fg(Color::LightMagenta)
                    } else {
                        Style::default().fg(Color::DarkGray)
                    },
                ),
                Span::raw(" "),
                Span::styled(
                    format!(
                        "[{}] {}:{}-{} {}",
                        state,
                        comment.path,
                        comment.label(),
                        comment.side.tag(),
                        comment.text
                    ),
                    style,
                ),
            ])
        })
        .collect();
    let title = format!(
        "Comments ({}){}",
        app.comments.unsent().len(),
        if focused { "  ·  focused" } else { "" }
    );
    let paragraph = Paragraph::new(lines).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(if focused {
                Color::LightMagenta
            } else {
                Color::DarkGray
            }))
            .title(title),
    );
    frame.render_widget(paragraph, area);
}

fn group_spans<'a>(
    label: &str,
    color: Color,
    items: &[(&str, &str)],
) -> Vec<Span<'a>> {
    let mut spans = vec![
        Span::styled(
            format!(" {} ", label),
            Style::default()
                .fg(color)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" "),
    ];
    for (index, (key, description)) in items.iter().enumerate() {
        if index > 0 {
            spans.push(Span::styled(" · ", Style::default().fg(Color::DarkGray)));
        }
        spans.push(Span::styled(
            (*key).to_string(),
            Style::default().fg(color).add_modifier(Modifier::BOLD),
        ));
        spans.push(Span::styled(
            format!(" {}", description),
            Style::default().fg(Color::Gray),
        ));
    }
    spans
}

fn draw_footer(frame: &mut Frame, app: &App, area: Rect) {
    let c = &app.config;
    let nav = vec![
        (c.key("toggle_view"), "view"),
        ("f/F", "file"),
        ("]/[", "hunk"),
        ("h/l/0", "pan"),
        (c.key("goto_file"), "jump"),
        (c.key("search"), "find"),
        ("n/N", "match"),
    ];
    let review = vec![
        (c.key("comment"), "note"),
        (c.key("visual_select"), "select"),
        (c.key("stage_selection"), "stage"),
        (c.key("edit_comment"), "edit"),
        (c.key("focus_comments"), "notes"),
        (c.key("resend_comments"), "resend"),
    ];
    let git_ops = vec![
        ("g", "git menu"),
        (c.key("refresh"), "refresh"),
    ];
    let agent = vec![
        (c.key("send"), "send"),
        (c.key("pick_target"), "target"),
        (c.key("new_tab"), "new tab"),
        (c.key("open_editor_tab"), "nvim"),
        (c.key("open_editor_float"), "float"),
    ];
    let view = vec![
        (c.key("toggle_split"), "split"),
        (c.key("toggle_whole_file"), "whole"),
        (c.key("toggle_comments"), "comments"),
        (c.key("history"), "log"),
        (c.key("help"), "help"),
        (c.key("quit"), "quit"),
    ];
    let mut row1 = group_spans("NAV", Color::Cyan, &nav);
    row1.push(Span::styled("  │  ", Style::default().fg(Color::DarkGray)));
    row1.extend(group_spans("REVIEW", Color::LightYellow, &review));
    row1.push(Span::styled("  │  ", Style::default().fg(Color::DarkGray)));
    row1.extend(group_spans("VIEW", Color::Magenta, &view));
    let mut row2 = group_spans("GIT", Color::Green, &git_ops);
    row2.push(Span::styled("  │  ", Style::default().fg(Color::DarkGray)));
    row2.extend(group_spans("AGENT", Color::LightBlue, &agent));
    let detail = Line::from(vec![
        Span::styled(" mode ", Style::default().fg(Color::DarkGray)),
        Span::styled(format!("{:?}", app.mode), Style::default().fg(Color::White)),
        Span::styled("  ·  ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            format!("{:?}", app.config.send_mode),
            Style::default().fg(Color::LightBlue),
        ),
        Span::styled("  ·  ", Style::default().fg(Color::DarkGray)),
        Span::styled(format!("split:{}", app.split), Style::default().fg(Color::Gray)),
        Span::styled("  ·  ", Style::default().fg(Color::DarkGray)),
        Span::styled(app.status.clone(), Style::default().fg(Color::LightYellow)),
    ]);
    let footer = Paragraph::new(vec![
        Line::from(row1),
        Line::from(row2),
        detail,
    ])
    .block(
        Block::default()
            .borders(Borders::TOP)
            .border_style(Style::default().fg(Color::DarkGray)),
    );
    frame.render_widget(footer, area);
}

fn draw_overlay(frame: &mut Frame, app: &App) {
    let accent = mode_accent(&app.mode);
    let (title, width_pct, base_height, keys): (&str, u16, u16, Vec<(&str, &str)>) =
        match app.mode {
        Mode::CommentInput => (
            if app.editing_id.is_some() {
                " Edit comment "
            } else {
                " New comment "
            },
            64,
            10,
            vec![("Enter", "save"), ("C-j / S-Enter", "new line"), ("Esc", "cancel")],
        ),
        Mode::CommitInput => (
            " Commit message ",
            64,
            10,
            vec![("Enter", "commit"), ("C-j / S-Enter", "new line"), ("Esc", "cancel")],
        ),
        Mode::Confirm => (
            " Confirm ",
            58,
            8,
            vec![("Enter", "confirm"), ("Esc", "cancel")],
        ),
        Mode::TargetPicker => (
            " Where to send ",
            76,
            (app.candidates.len() as u16 + 7).min(24),
            vec![("Enter", "send"), ("Esc", "cancel")],
        ),
        Mode::Help => (
            " Keys ",
            84,
            (app.config.keys.len().div_ceil(2) as u16 + 6).min(34),
            vec![("?", "close")],
        ),
        Mode::FilePicker => (
            " Jump to file ",
            78,
            (app.file_matches.len() as u16 + 9).min(30),
            vec![("Enter", "jump"), ("Esc", "cancel")],
        ),
        Mode::PendingG => (
            " Git commands ",
            72,
            (app
                .config
                .keys
                .values()
                .filter(|key| key.starts_with('g') && key.len() > 1)
                .count() as u16
                / 2
                + 12)
                .min(28),
            vec![("Esc", "cancel")],
        ),
        Mode::History => (
            " Commit history ",
            84,
            26,
            vec![("↑/↓", "select"), ("Enter", "close"), ("Esc", "close")],
        ),
        Mode::DiffSearch => (
            " Search in diff ",
            72,
            9,
            vec![("Enter", "close"), ("n/N", "next/prev"), ("Esc", "close")],
        ),
        Mode::CommandOutput => (
            " Command output ",
            84,
            24,
            vec![("Esc", "close")],
        ),
        Mode::BranchInput => (
            " New branch from base ",
            64,
            10,
            vec![("Enter", "create"), ("Esc", "cancel")],
        ),
        Mode::BasePicker => (
            " Choose base branch ",
            62,
            (app.base_branches.len() as u16 + 9).clamp(12, 26),
            vec![("Enter", "select"), ("Esc", "cancel")],
        ),
        Mode::RenameInput => (
            " Rename branch ",
            64,
            10,
            vec![("Enter", "continue"), ("Esc", "cancel")],
        ),
        Mode::Loading => (
            " Working ",
            62,
            8,
            vec![("Esc", "wait")],
        ),
        Mode::StashPicker => (
            " Pop which stash ",
            76,
            (app.stashes.len() as u16 + 7).min(24),
            vec![("j/k", "move"), ("Enter", "pop"), ("Esc", "cancel")],
        ),
        Mode::StashTitle => (
            " Stash title ",
            64,
            9,
            vec![("Enter", "stash"), ("Esc", "cancel")],
        ),
        Mode::Visual => return,
        Mode::CommentPanel => return,
        Mode::Normal => return,
        };
    let height = if app.mode == Mode::CommentInput || app.mode == Mode::CommitInput {
        let extra = app.input.lines().count().saturating_sub(1) as u16;
        base_height.saturating_add(extra).min(30)
    } else {
        base_height
    };
    let area = dialog_rect(width_pct, height, frame.area());
    frame.render_widget(Clear, area);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(accent))
        .title(Line::from(Span::styled(
            title,
            Style::default()
                .fg(accent)
                .add_modifier(Modifier::BOLD),
        )))
        .style(Style::default());
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let field_width = inner.width as usize;
    let mut lines: Vec<Line<'static>> = Vec::new();
    match app.mode {
        Mode::Visual => return,
        Mode::StashTitle => {
            lines.push(Line::from(Span::styled(
                "Stash the working tree (including untracked) under this title.",
                Style::default().fg(Color::Gray),
            )));
            lines.push(Line::from(""));
            lines.push(Line::from(vec![
                Span::styled("> ", Style::default().fg(accent)),
                Span::raw(app.input.clone()),
                Span::styled("▏", Style::default().fg(accent)),
            ]));
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                if app.input.trim().is_empty() {
                    "empty title uses \"diffnote stash\""
                } else {
                    ""
                },
                Style::default().fg(Color::DarkGray),
            )));
        }
        Mode::StashPicker => {
            lines.push(Line::from(Span::styled(
                "Choose a stash to pop into the working tree.",
                Style::default().fg(Color::Gray),
            )));
            lines.push(Line::from(""));
            for (index, (reference, message)) in app.stashes.iter().enumerate() {
                let selected = index == app.stash_pick;
                lines.push(Line::from(vec![
                    Span::styled(
                        if selected { "▸ " } else { "  " },
                        Style::default().fg(if selected { accent } else { Color::DarkGray }),
                    ),
                    Span::styled(
                        format!("{:<10}", reference),
                        Style::default().fg(Color::Cyan),
                    ),
                    Span::styled(
                        message.clone(),
                        if selected {
                            Style::default().fg(Color::White).add_modifier(Modifier::BOLD)
                        } else {
                            Style::default().fg(Color::Gray)
                        },
                    ),
                ]));
            }
        }
        Mode::CommentInput | Mode::CommitInput => {
            let label = if app.mode == Mode::CommentInput {
                if app.editing_id.is_some() {
                    "Edit this comment. It will be queued again."
                } else {
                    "Comment on the selected diff line."
                }
            } else {
                "Message for the staged changes."
            };
            lines.push(Line::from(Span::styled(
                label,
                Style::default().fg(Color::Gray),
            )));
            lines.push(Line::from(""));
            lines.extend(field_lines(&app.input, field_width, accent));
            lines.push(Line::from(""));
            lines.push(Line::from(vec![
                Span::styled("·", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    format!(" {} chars", app.input.chars().count()),
                    Style::default().fg(Color::DarkGray),
                ),
            ]));
        }
        Mode::Confirm => {
            let (head, sub) = match app.confirm_action.as_deref() {
                Some("stage_all") => ("Stage every changed file?", "This adds all files to the index."),
                Some("unstage_all") => ("Unstage every staged file?", "Your working files are not changed."),
                Some("pull") => ("Pull into this branch?", "Runs git pull against the configured upstream."),
                Some("push") => ("Push this branch to origin?", "Runs git push -u origin HEAD."),
                Some("force_push") => (
                    "Force-with-lease push to origin?",
                    "Runs git push -u origin HEAD --force-with-lease. Rewrites remote history.",
                ),
                Some("abort_op") => (
                    "Abort the operation?",
                    "Drops the in-progress merge, rebase, cherry-pick, or revert and every conflict resolution you made.",
                ),
                Some(action) if action.starts_with("rename:") => (
                    "Rename the current branch?",
                    "Only the local branch name changes.",
                ),
                Some(action) if action.starts_with("discard:") => (
                    "Throw away every change in this file?",
                    "Runs git restore, or git clean for an untracked file. The changes cannot be recovered.",
                ),
                _ => (
                    "The tree changed since the last render.",
                    "Refresh with r, or commit anyway.",
                ),
            };
            let head_color = if app.confirm_action.as_deref() == Some("force_push")
                || app.confirm_action.as_deref() == Some("abort_op")
                || app
                    .confirm_action
                    .as_deref()
                    .is_some_and(|action| action.starts_with("discard:"))
            {
                Color::Red
            } else {
                Color::Yellow
            };
            lines.push(Line::from(Span::styled(
                head,
                Style::default().fg(head_color).add_modifier(Modifier::BOLD),
            )));
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                sub,
                Style::default().fg(Color::Gray),
            )));
            lines.push(Line::from(""));
        }
        Mode::TargetPicker => {
            let hint = if app.pending_send {
                "Choose a destination for your comments."
            } else {
                "Choose the default target."
            };
            lines.push(Line::from(Span::styled(
                hint,
                Style::default().fg(Color::Gray),
            )));
            lines.push(Line::from(""));
            for (index, candidate) in app.candidates.iter().enumerate() {
                let selected = index == app.selected_target;
                let marker = if selected { "▸" } else { " " };
                let marker_style = if selected {
                    Style::default().fg(accent).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::DarkGray)
                };
                let label_style = if selected {
                    Style::default().fg(Color::White).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::Gray)
                };
                let mut spans = vec![
                    Span::styled(marker, marker_style),
                    Span::raw(" "),
                    Span::styled(candidate.label.clone(), label_style),
                ];
                if candidate.active {
                    spans.push(Span::styled(
                        "  active",
                        Style::default().fg(Color::Green),
                    ));
                }
                lines.push(Line::from(spans));
            }
            lines.push(Line::from(""));
        }
        Mode::Help => {
            let mut entries: Vec<(&String, &String)> = app.config.keys.iter().collect();
            entries.sort_by(|a, b| a.0.cmp(b.0));
            let half = entries.len().div_ceil(2);
            for row in 0..half {
                let mut spans = Vec::new();
                for column in 0..2 {
                    let Some((action, key)) = entries.get(row + column * half) else {
                        continue;
                    };
                    if column > 0 {
                        spans.push(Span::raw("   "));
                    }
                    spans.push(Span::styled(
                        format!(" {:>8} ", key),
                        Style::default()
                            .fg(accent)
                            .add_modifier(Modifier::BOLD),
                    ));
                    spans.push(Span::styled(
                        format!("  {:<22}", action),
                        Style::default().fg(Color::Gray),
                    ));
                }
                lines.push(Line::from(spans));
            }
            lines.push(Line::from(""));
        }
        Mode::FilePicker => {
            lines.push(Line::from(Span::styled(
                format!(
                    "Type a filename  ·  {} of {} files",
                    app.file_matches.len(),
                    app.snapshot.files.len()
                ),
                Style::default().fg(Color::Gray),
            )));
            lines.push(Line::from(""));
            lines.extend(field_lines(&app.file_query, field_width, accent));
            lines.push(Line::from(""));
            let visible = (height as usize).saturating_sub(9).min(app.file_matches.len());
            let start = app
                .file_pick
                .saturating_sub(visible.saturating_sub(1));
            for (position, &index) in app.file_matches[start..].iter().take(visible).enumerate() {
                let selected = start + position == app.file_pick;
                let entry = &app.snapshot.files[index];
                let marker = if selected { "▸" } else { " " };
                lines.push(Line::from(vec![
                    Span::styled(
                        marker,
                        if selected {
                            Style::default().fg(accent).add_modifier(Modifier::BOLD)
                        } else {
                            Style::default().fg(Color::DarkGray)
                        },
                    ),
                    Span::raw(" "),
                    Span::styled(
                        entry.section.title(),
                        Style::default().fg(section_color(entry.section)),
                    ),
                    Span::raw(" "),
                    Span::styled(
                        entry.path.clone(),
                        if selected {
                            Style::default()
                                .fg(Color::White)
                                .add_modifier(Modifier::BOLD)
                        } else {
                            Style::default().fg(Color::Gray)
                        },
                    ),
                ]));
            }
            lines.push(Line::from(""));
        }
        Mode::PendingG => {
            lines.push(Line::from(Span::styled(
                "Git commands",
                Style::default().fg(Color::Gray),
            )));
            lines.push(Line::from(""));
            let mut entries: Vec<(String, String)> = app
                .config
                .keys
                .iter()
                .filter(|(_, key)| key.starts_with('g') && key.len() > 1)
                .map(|(action, key)| (key.clone(), action.clone()))
                .collect();
            entries.sort_by(|a, b| {
                git_group(&a.1)
                    .cmp(&git_group(&b.1))
                    .then(a.0.cmp(&b.0))
            });
            let mut groups: Vec<(usize, Vec<(String, String)>)> = Vec::new();
            for (key, action) in entries {
                let group = git_group(&action);
                match groups.last_mut() {
                    Some((current, items)) if *current == group => items.push((key, action)),
                    _ => groups.push((group, vec![(key, action)])),
                }
            }
            for (group, items) in groups.iter() {
                lines.push(Line::from(Span::styled(
                    format!(" {}", GIT_GROUPS[*group]),
                    Style::default()
                        .fg(Color::DarkGray)
                        .add_modifier(Modifier::BOLD),
                )));
                let half = items.len().div_ceil(2);
                for row in 0..half {
                    let mut spans = Vec::new();
                    for item in [items.get(row), items.get(row + half)] {
                        match item {
                            Some((key, action)) => {
                                spans.push(Span::styled(
                                    format!(" {:<3}", key),
                                    Style::default().fg(accent).add_modifier(Modifier::BOLD),
                                ));
                                spans.push(Span::styled(
                                    format!("{:<17}", action),
                                    Style::default().fg(Color::Gray),
                                ));
                            }
                            None => spans.push(Span::raw(" ".repeat(20))),
                        }
                    }
                    lines.push(Line::from(spans));
                }
            }
            lines.push(Line::from(""));
        }
        Mode::History => {
            let rows = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Min(4), Constraint::Length(1)])
                .split(inner);
            let columns = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(42), Constraint::Percentage(58)])
                .split(rows[0]);
            let visible = columns[0].height as usize;
            let mut list_lines: Vec<Line<'static>> = Vec::new();
            let mut line_of_pick = 0usize;
            let mut last_date = String::new();
            for (index, entry) in app.history.iter().enumerate() {
                if entry.date != last_date {
                    list_lines.push(Line::from(vec![
                        Span::styled("  ── ", Style::default().fg(Color::DarkGray)),
                        Span::styled(
                            entry.date.clone(),
                            Style::default()
                                .fg(Color::LightBlue)
                                .add_modifier(Modifier::BOLD),
                        ),
                        Span::styled(" ──", Style::default().fg(Color::DarkGray)),
                    ]));
                    last_date = entry.date.clone();
                }
                if index == app.history_pick {
                    line_of_pick = list_lines.len();
                }
                let selected = index == app.history_pick;
                let marker = if selected { "▸" } else { " " };
                list_lines.push(Line::from(vec![
                    Span::styled(
                        marker,
                        if selected {
                            Style::default().fg(accent)
                        } else {
                            Style::default().fg(Color::DarkGray)
                        },
                    ),
                    Span::raw(" "),
                    Span::styled(entry.sha.clone(), Style::default().fg(Color::Yellow)),
                    Span::raw(" "),
                    Span::styled(
                        entry.subject.clone(),
                        if selected {
                            Style::default()
                                .fg(Color::White)
                                .add_modifier(Modifier::BOLD)
                        } else {
                            Style::default().fg(Color::Gray)
                        },
                    ),
                ]));
                if selected {
                    list_lines.push(Line::from(Span::styled(
                        format!("    {} · {}", entry.author, entry.date),
                        Style::default().fg(Color::DarkGray),
                    )));
                }
            }
            let offset = if line_of_pick >= visible {
                line_of_pick - visible + 1
            } else {
                0
            };
            frame.render_widget(
                Paragraph::new(list_lines).scroll((offset as u16, 0)),
                columns[0],
            );
            let mut diff_lines: Vec<Line<'static>> = Vec::new();
            if let Some(entry) = app.history.get(app.history_pick) {
                diff_lines.push(Line::from(vec![
                    Span::styled(
                        entry.sha.clone(),
                        Style::default().fg(Color::Yellow),
                    ),
                    Span::styled(
                        format!("  {}", entry.subject),
                        Style::default().fg(Color::White),
                    ),
                ]));
                diff_lines.push(Line::from(""));
            }
            if let Some(doc) = &app.history_diff {
                for diff_line in &doc.lines {
                    let (prefix, color) = match diff_line.kind {
                        LineKind::Header => ("  ", Color::DarkGray),
                        LineKind::Hunk => ("", Color::Cyan),
                        LineKind::Context => ("  ", Color::Gray),
                        LineKind::Add => ("+ ", Color::Green),
                        LineKind::Remove => ("- ", Color::Red),
                        LineKind::Conflict => ("! ", Color::LightYellow),
                    };
                    diff_lines.push(Line::from(Span::styled(
                        format!("{}{}", prefix, diff_line.text),
                        Style::default().fg(color),
                    )));
                }
            } else {
                diff_lines.push(Line::from(Span::styled(
                    "no diff",
                    Style::default().fg(Color::DarkGray),
                )));
            }
            frame.render_widget(
                Paragraph::new(diff_lines).scroll((app.history_scroll as u16, 0)),
                columns[1],
            );
            let hint = Line::from(vec![
                Span::styled(
                    format!("  {}/{}", app.history_pick + 1, app.history.len()),
                    Style::default().fg(Color::LightBlue),
                ),
                Span::styled("   ", Style::default()),
                Span::styled("↑/↓", Style::default().fg(accent)),
                Span::styled(" select   ", Style::default().fg(Color::Gray)),
                Span::styled("J/K", Style::default().fg(accent)),
                Span::styled(" scroll diff   ", Style::default().fg(Color::Gray)),
                Span::styled("Enter/Esc", Style::default().fg(accent)),
                Span::styled(" close", Style::default().fg(Color::Gray)),
            ]);
            frame.render_widget(Paragraph::new(hint), rows[1]);
            return;
        }
        Mode::DiffSearch => {
            lines.push(Line::from(Span::styled(
                format!("{} match(es)", app.search_hits.len()),
                Style::default().fg(Color::Gray),
            )));
            lines.push(Line::from(""));
            lines.extend(field_lines(&app.search_query, field_width, accent));
            lines.push(Line::from(""));
        }
        Mode::CommandOutput => {
            lines.push(Line::from(Span::styled(
                app.command_title.clone(),
                Style::default().fg(Color::LightBlue),
            )));
            lines.push(Line::from(""));
            let body = if app.command_output.trim().is_empty() {
                "(no output)".to_string()
            } else {
                app.command_output.clone()
            };
            for text in body.lines() {
                lines.push(Line::from(Span::styled(
                    text.to_string(),
                    Style::default().fg(Color::Gray),
                )));
            }
        }
        Mode::BranchInput => {
            lines.push(Line::from(Span::styled(
                format!("Create a branch from '{}'", app.config.base_branch),
                Style::default().fg(Color::Gray),
            )));
            lines.push(Line::from(""));
            lines.extend(field_lines(&app.branch_input, field_width, accent));
            lines.push(Line::from(""));
        }
        Mode::BasePicker => {
            lines.push(Line::from(Span::styled(
                format!("Comparing against '{}'", app.config.base_branch),
                Style::default().fg(Color::Gray),
            )));
            lines.push(Line::from(""));
            let visible = (height as usize).saturating_sub(7).min(app.base_branches.len()).max(1);
            let start = app.base_pick.saturating_sub(visible.saturating_sub(1));
            for (position, name) in app.base_branches[start..].iter().take(visible).enumerate() {
                let selected = start + position == app.base_pick;
                let marker = if selected { "▸" } else { " " };
                lines.push(Line::from(vec![
                    Span::styled(
                        marker,
                        if selected {
                            Style::default().fg(accent)
                        } else {
                            Style::default().fg(Color::DarkGray)
                        },
                    ),
                    Span::raw(" "),
                    Span::styled(
                        name.clone(),
                        if selected {
                            Style::default()
                                .fg(Color::White)
                                .add_modifier(Modifier::BOLD)
                        } else {
                            Style::default().fg(Color::Gray)
                        },
                    ),
                    if name == &app.config.base_branch {
                        Span::styled("  current", Style::default().fg(Color::Green))
                    } else {
                        Span::raw("")
                    },
                ]));
            }
            lines.push(Line::from(""));
        }
        Mode::RenameInput => {
            lines.push(Line::from(Span::styled(
                format!("Rename '{}' to:", app.branch_state.current),
                Style::default().fg(Color::Gray),
            )));
            lines.push(Line::from(""));
            lines.extend(field_lines(&app.rename_input, field_width, accent));
            lines.push(Line::from(""));
        }
        Mode::Loading => {
            lines.push(Line::from(Span::styled(
                "Running git command, please wait...",
                Style::default().fg(Color::LightGreen).add_modifier(Modifier::BOLD),
            )));
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                app.loading_label.clone(),
                Style::default().fg(Color::Gray),
            )));
            lines.push(Line::from(""));
        }
        Mode::CommentPanel => return,
        Mode::Normal => return,
    }
    lines.push(footer_hint(&keys, accent));
    frame.render_widget(Paragraph::new(lines), inner);
}

pub fn draw(frame: &mut Frame, app: &App) {
    let area = frame.area();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2),
            Constraint::Min(4),
            Constraint::Length(4),
        ])
        .split(area);
    let view = match app.view {
        View::Review => "Review",
        View::Git => "Git",
    };
    let sep = || Span::styled("  ›  ", Style::default().fg(Color::DarkGray));
    let project = app
        .repo
        .file_name()
        .and_then(|name| name.to_str()).unwrap_or("diffnote").to_string();
    let mut header_spans = vec![
        Span::styled(
            format!(" {} ", project),
            Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
        ),
        sep(),
        Span::styled(
            app.snapshot.head.clone(),
            Style::default().fg(Color::DarkGray),
        ),
        sep(),
    ];
    if app.branch_state.current.is_empty() {
        header_spans.push(Span::styled(
            "detached",
            Style::default().fg(Color::Red),
        ));
    } else {
        let (text, color) = if !app.branch_state.base_exists {
            (
                format!("{} ⚠ {} missing", app.branch_state.current, app.branch_state.base),
                Color::Yellow,
            )
        } else if app.branch_state.updated {
            (
                format!("{} ✓ up to date", app.branch_state.current),
                Color::Green,
            )
        } else {
            (
                format!(
                    "{} ↓{} ↑{}",
                    app.branch_state.current, app.branch_state.behind, app.branch_state.ahead
                ),
                Color::Yellow,
            )
        };
        header_spans.push(Span::styled(text, Style::default().fg(color)));
    }
    if let Some(state) = &app.merge_state {
        header_spans.push(sep());
        header_spans.push(Span::styled(
            format!("{} · {} unmerged", state, app.conflict_count),
            Style::default().fg(Color::LightRed).add_modifier(Modifier::BOLD),
        ));
    }
    header_spans.push(sep());
    header_spans.push(Span::styled(view, Style::default().fg(Color::Cyan)));
    header_spans.push(sep());
    header_spans.push(Span::styled(
        format!("base {}", app.config.base_branch),
        Style::default().fg(Color::Gray),
    ));
    header_spans.push(sep());
    let paired = match &app.target {
        Some(target) => format!("→ {}", target.short_label()),
        None => "no agent".to_string(),
    };
    header_spans.push(Span::styled(
        paired,
        Style::default().fg(Color::LightBlue),
    ));
    let header = Paragraph::new(vec![
        Line::from(header_spans),
        Line::from(app.status.clone()),
    ])
    .block(Block::default().borders(Borders::BOTTOM));
    frame.render_widget(header, chunks[0]);
    let body = if app.show_comments {
        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(6), Constraint::Length(9)])
            .split(chunks[1]);
        let columns = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(70), Constraint::Percentage(30)])
            .split(rows[1]);
        draw_comments(frame, app, columns[0]);
        draw_stashes(frame, app, columns[1]);
        rows[0]
    } else {
        chunks[1]
    };
    match app.view {
        View::Git => {
            let columns = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([
                    Constraint::Percentage(26),
                    Constraint::Percentage(74),
                ])
                .split(body);
            draw_files(frame, app, columns[0]);
            draw_diff(frame, app, columns[1]);
        }
        View::Review => draw_diff(frame, app, body),
    }
    draw_footer(frame, app, chunks[2]);
    if app.mode != Mode::Normal && app.mode != Mode::CommentPanel {
        draw_overlay(frame, app);
    }
}

const GIT_GROUPS: [&str; 6] = [
    "Staging",
    "Commit & branch",
    "Remote",
    "Conflicts",
    "Stash",
    "Other",
];

fn git_group(action: &str) -> usize {
    match action {
        "stage" | "stage_all" | "unstage" | "unstage_all" | "stage_hunk" | "discard" => 0,
        "commit" | "branchup" | "rename_branch" | "update_branch" | "pick_base" | "refresh" => 1,
        "push" | "force_push" | "pull" => 2,
        "prev_conflict" | "next_conflict" | "take_ours" | "take_theirs" | "continue_op"
        | "abort_op" => 3,
        "stash_push" | "stash_pop" => 4,
        _ => 5,
    }
}
