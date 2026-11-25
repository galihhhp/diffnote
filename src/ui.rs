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
                .fg(Color::Black)
                .bg(accent)
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
        Section::Untracked => Color::Cyan,
    }
}

fn status_color(status: &str) -> Color {
    let code = status.trim();
    if code == "??" {
        return Color::Cyan;
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

fn split_cell_line(
    cell: Option<&SplitCell>,
    highlight: bool,
    mark_count: usize,
) -> Line<'static> {
    let Some(cell) = cell else {
        return Line::from(Span::styled("~", Style::default().fg(Color::DarkGray)));
    };
    let (prefix, color) = match cell.kind {
        LineKind::Header => ("  ", Color::DarkGray),
        LineKind::Hunk => ("", Color::Cyan),
        LineKind::Context => ("  ", Color::Gray),
        LineKind::Add => ("+ ", Color::Green),
        LineKind::Remove => ("- ", Color::Red),
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
            .fg(Color::Black)
            .bg(Color::LightYellow)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default()
    };
    let mut style = Style::default().fg(color);
    if mark_count > 0 {
        style = style.bg(Color::Rgb(58, 38, 8));
    }
    if highlight {
        style = style.bg(Color::DarkGray);
    }
    Line::from(vec![
        Span::styled(badge, badge_style),
        Span::styled(number, Style::default().fg(Color::DarkGray)),
        Span::styled(format!("{}{}", prefix, cell.text), style),
    ])
}

fn split_panes(app: &App, doc: &DiffDoc, rows: &[SplitRow]) -> (Vec<Line<'static>>, Vec<Line<'static>>) {
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
        let mark_count = app.comments.for_line(&doc.path, number).len();
        left.push(split_cell_line(row.left.as_ref(), highlight, mark_count));
        right.push(split_cell_line(row.right.as_ref(), highlight, mark_count));
    }
    (left, right)
}

fn diff_lines(app: &App, doc: &DiffDoc) -> Vec<Line<'static>> {
    let hits: std::collections::HashSet<usize> =
        app.search_hits.iter().copied().collect();
    doc.lines
        .iter()
        .enumerate()
        .map(|(index, line)| {
            let (prefix, color) = match line.kind {
                LineKind::Header => ("  ", Color::DarkGray),
                LineKind::Hunk => ("", Color::Cyan),
                LineKind::Context => ("  ", Color::Gray),
                LineKind::Add => ("+ ", Color::Green),
                LineKind::Remove => ("- ", Color::Red),
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
                    .fg(Color::Black)
                    .bg(Color::LightYellow)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            let mut style = Style::default().fg(color);
            if mark_count > 0 {
                style = style.bg(Color::Rgb(58, 38, 8));
            }
            if hits.contains(&index) {
                style = style.bg(Color::Rgb(70, 55, 20));
            }
            if index == app.cursor {
                style = style.bg(Color::DarkGray).add_modifier(Modifier::BOLD);
            }
            Line::from(vec![
                Span::styled(badge, badge_style),
                Span::styled(number, Style::default().fg(Color::DarkGray)),
                Span::styled(format!("{}{}", prefix, line.text), style),
            ])
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
            let section_style = Style::default()
                .fg(section_color(entry.section))
                .add_modifier(Modifier::BOLD);
            let comment_count = app
                .comments
                .items
                .iter()
                .filter(|comment| comment.path == entry.path)
                .count();
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
                Span::styled(entry.section.title(), section_style),
                Span::raw(" "),
                Span::styled(
                    format!("{:>2}", entry.status),
                    Style::default().fg(status_color(&entry.status)),
                ),
                Span::raw(" "),
                Span::styled(
                    entry.path.clone(),
                    if selected {
                        Style::default().add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(Color::Gray)
                    },
                ),
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
    for entry in &app.snapshot.files {
        match entry.section {
            Section::Staged => staged += 1,
            Section::Unstaged => unstaged += 1,
            Section::Untracked => untracked += 1,
        }
    }
    let title = format!(
        "Files  +{} ~{} ?{}",
        staged, unstaged, untracked
    );
    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title(title))
        .highlight_style(
            Style::default()
                .bg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
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
    let section = app
        .current_file()
        .map(|entry| entry.section.title())
        .unwrap_or("Diff");
    let block = Block::default()
        .borders(Borders::ALL)
        .title(if app.split {
            format!("{} [{}]  |  OLD  /  NEW", doc.path, section)
        } else {
            format!("{} [{}]", doc.path, section)
        });
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let offset = app.cursor.saturating_sub(inner.height as usize / 2) as u16;
    if app.split {
        let rows = crate::diff::split_rows(doc);
        let (left_lines, right_lines) = split_panes(app, doc, &rows);
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
            Paragraph::new(left_lines)
                .scroll((offset, 0)),
            columns[0],
        );
        frame.render_widget(
            Paragraph::new(right_lines)
                .scroll((offset, 0)),
            columns[1],
        );
    } else {
        frame.render_widget(
            Paragraph::new(diff_lines(app, doc)).scroll((offset, 0)),
            inner,
        );
    }
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
                    .bg(Color::DarkGray)
                    .add_modifier(Modifier::BOLD)
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
                        comment.line_no,
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
                .fg(Color::Black)
                .bg(color)
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
        (c.key("goto_file"), "jump"),
        (c.key("search"), "find"),
        ("n/N", "match"),
    ];
    let review = vec![
        (c.key("comment"), "note"),
        (c.key("edit_comment"), "edit"),
        (c.key("focus_comments"), "notes"),
        (c.key("resend_comments"), "resend"),
    ];
    let git_ops = vec![
        (c.key("stage"), "stage"),
        (c.key("stage_all"), "all"),
        (c.key("unstage"), "unstage"),
        (c.key("unstage_all"), "all"),
        (c.key("commit"), "commit"),
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
        (c.key("toggle_comments"), "comments"),
        (c.key("history"), "log"),
        (c.key("help"), "help"),
        (c.key("quit"), "quit"),
    ];
    let mut row1 = group_spans("NAV", Color::Cyan, &nav);
    row1.push(Span::styled("  │  ", Style::default().fg(Color::DarkGray)));
    row1.extend(group_spans("REVIEW", Color::LightYellow, &review));
    let mut row2 = group_spans("GIT", Color::Green, &git_ops);
    row2.push(Span::styled("  │  ", Style::default().fg(Color::DarkGray)));
    row2.extend(group_spans("AGENT", Color::LightBlue, &agent));
    let row3 = group_spans("VIEW", Color::Magenta, &view);
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
        Line::from(row3),
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
            vec![("Enter", "commit anyway"), ("Esc", "cancel")],
        ),
        Mode::TargetPicker => (
            " Where to send ",
            76,
            (app.candidates.len() as u16 + 7).min(24),
            vec![("Enter", "send"), ("Esc", "cancel")],
        ),
        Mode::Help => (
            " Keys ",
            62,
            (app.config.keys.len() as u16 + 5).min(30),
            vec![("?", "close")],
        ),
        Mode::FilePicker => (
            " Jump to file ",
            78,
            (app.file_matches.len() as u16 + 9).min(30),
            vec![("Enter", "jump"), ("Esc", "cancel")],
        ),
        Mode::PendingG => (
            " Commit ",
            48,
            7,
            vec![("c", "continue"), ("Esc", "cancel")],
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
        .style(Style::default().bg(Color::Black));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let field_width = inner.width as usize;
    let mut lines: Vec<Line<'static>> = Vec::new();
    match app.mode {
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
                _ => (
                    "The tree changed since the last render.",
                    "Refresh with r, or commit anyway.",
                ),
            };
            lines.push(Line::from(Span::styled(
                head,
                Style::default().fg(Color::Yellow),
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
            for (action, key) in entries {
                lines.push(Line::from(vec![
                    Span::styled(
                        format!(" {:>8} ", key),
                        Style::default()
                            .fg(Color::Black)
                            .bg(accent)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        format!("  {}", action),
                        Style::default().fg(Color::Gray),
                    ),
                ]));
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
                "Press c to open the commit message.",
                Style::default().fg(Color::Gray),
            )));
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
            Constraint::Length(5),
        ])
        .split(area);
    let view = match app.view {
        View::Review => "Review",
        View::Git => "Git",
    };
    let paired = match &app.target {
        Some(target) => format!("-> {}", target.label()),
        None => "no agent".to_string(),
    };
    let header = Paragraph::new(vec![
        Line::from(format!(
            "diffnote | {} | {} | {} | {}",
            app.repo.display(),
            app.snapshot.head,
            view,
            paired
        )),
        Line::from(app.status.clone()),
    ])
    .block(Block::default().borders(Borders::BOTTOM));
    frame.render_widget(header, chunks[0]);
    let body = if app.show_comments {
        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(6), Constraint::Length(9)])
            .split(chunks[1]);
        draw_comments(frame, app, rows[1]);
        rows[0]
    } else {
        chunks[1]
    };
    match app.view {
        View::Git => {
            let columns = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([
                    Constraint::Percentage(30),
                    Constraint::Percentage(70),
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
