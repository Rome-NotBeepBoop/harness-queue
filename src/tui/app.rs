use crate::image::{describe_attachments, extract_image_references, format_prompt_with_images};
use crate::models::{AgentTarget, ItemStatus, QueueItem, State};
use crate::supervisor::Supervisor;
use crossterm::{
    event::{
        self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind, KeyModifiers,
        MouseButton, MouseEvent, MouseEventKind,
    },
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, List, ListItem, Paragraph, Wrap},
    Terminal,
};
use std::io::{self, stdout};
use std::time::{Duration, Instant};

pub struct TuiApp<'a> {
    pub supervisor: &'a mut Supervisor,
    pub input: String,
    pub cursor_position: usize,
    pub logs: Vec<String>,
    pub running: bool,
    pub last_tick: Instant,
    pub editing_item_id: Option<usize>,
    pub queue_rect: Rect,
}

impl<'a> TuiApp<'a> {
    pub fn new(supervisor: &'a mut Supervisor) -> Self {
        let mut app = Self {
            supervisor,
            input: String::new(),
            cursor_position: 0,
            logs: Vec::new(),
            running: true,
            last_tick: Instant::now(),
            editing_item_id: None,
            queue_rect: Rect::default(),
        };

        let now_str = current_time_str();
        app.logs
            .push(format!("{} ✓ Supervisor initialized", now_str));

        if let Some(target) = &app.supervisor.target {
            app.logs.push(format!(
                "{} ● Watching {} session {}",
                now_str,
                target.harness.display_name(),
                target.short_session_id()
            ));
        }

        app
    }

    pub fn add_log(&mut self, msg: String) {
        let timestamp = current_time_str();
        self.logs.push(format!("{} {}", timestamp, msg));
    }

    pub fn start_editing(&mut self, id: usize) {
        if let Some(item) = self.supervisor.queue.get(id) {
            if item.status == ItemStatus::Queued {
                self.editing_item_id = Some(id);
                self.input = item.prompt.clone();
                self.cursor_position = self.input.len();
                self.add_log(format!(
                    "✎ Editing prompt #{} (Press Enter to save, Esc to cancel)",
                    id
                ));
            } else {
                self.add_log(format!(
                    "! Cannot edit prompt #{} (already {:?})",
                    id, item.status
                ));
            }
        }
    }

    pub fn cancel_editing(&mut self) {
        if let Some(id) = self.editing_item_id.take() {
            self.input.clear();
            self.cursor_position = 0;
            self.add_log(format!("Cancelled edit for prompt #{}", id));
        }
    }

    pub fn edit_previous_queued_item(&mut self) {
        let queued = self.supervisor.queue.queued_ids();
        if queued.is_empty() {
            self.add_log("No queued prompts to edit".to_string());
            return;
        }

        let next_to_edit = match self.editing_item_id {
            None => queued.last().copied(),
            Some(current_id) => {
                if let Some(idx) = queued.iter().position(|&id| id == current_id) {
                    if idx > 0 {
                        Some(queued[idx - 1])
                    } else {
                        queued.last().copied() // Wrap around
                    }
                } else {
                    queued.last().copied()
                }
            }
        };

        if let Some(id) = next_to_edit {
            self.start_editing(id);
        }
    }

    pub fn submit_input(&mut self) {
        let trimmed = self.input.trim().to_string();
        if trimmed.is_empty() {
            return;
        }

        let harness = self
            .supervisor
            .target
            .as_ref()
            .map(|t| t.harness.clone())
            .unwrap_or(crate::models::HarnessKind::Codex);

        let images = extract_image_references(&trimmed);
        let attachments_desc = if !images.is_empty() {
            Some(describe_attachments(&images))
        } else {
            None
        };

        let formatted = format_prompt_with_images(&trimmed, &images, &harness);

        if let Some(edit_id) = self.editing_item_id.take() {
            // Update existing queued prompt
            match self
                .supervisor
                .update_queued_prompt(edit_id, formatted, attachments_desc.clone())
            {
                Ok(_) => {
                    if let Some(desc) = attachments_desc {
                        self.add_log(format!("✓ Prompt #{} updated {}", edit_id, desc));
                    } else {
                        self.add_log(format!("✓ Prompt #{} updated", edit_id));
                    }
                }
                Err(e) => {
                    self.add_log(format!("! Failed to update prompt #{}: {}", edit_id, e));
                }
            }
        } else {
            // Enqueue new prompt
            let id = self
                .supervisor
                .enqueue_prompt_with_attachments(formatted, attachments_desc.clone());

            if let Some(desc) = attachments_desc {
                self.add_log(format!("✓ Prompt #{} queued {}", id, desc));
            } else {
                self.add_log(format!("✓ Prompt #{} queued", id));
            }
        }

        self.input.clear();
        self.cursor_position = 0;
    }

    pub fn handle_key(&mut self, key_event: event::KeyEvent) {
        if key_event.kind != KeyEventKind::Press {
            return;
        }

        if key_event.modifiers.contains(KeyModifiers::CONTROL)
            && key_event.code == KeyCode::Char('c')
        {
            self.running = false;
            return;
        }

        // Shift + Left Arrow: Edit last/previous queued prompt
        if key_event.modifiers.contains(KeyModifiers::SHIFT) && key_event.code == KeyCode::Left {
            self.edit_previous_queued_item();
            return;
        }

        match key_event.code {
            KeyCode::Esc => {
                if self.editing_item_id.is_some() {
                    self.cancel_editing();
                } else {
                    self.running = false;
                }
            }
            KeyCode::Enter => {
                self.submit_input();
            }
            KeyCode::Char(c) => {
                self.input.insert(self.cursor_position, c);
                self.cursor_position += 1;
            }
            KeyCode::Backspace => {
                if self.cursor_position > 0 {
                    self.cursor_position -= 1;
                    self.input.remove(self.cursor_position);
                }
            }
            KeyCode::Delete => {
                if self.cursor_position < self.input.len() {
                    self.input.remove(self.cursor_position);
                }
            }
            KeyCode::Left => {
                if self.cursor_position > 0 {
                    self.cursor_position -= 1;
                }
            }
            KeyCode::Right => {
                if self.cursor_position < self.input.len() {
                    self.cursor_position += 1;
                }
            }
            KeyCode::Home => {
                self.cursor_position = 0;
            }
            KeyCode::End => {
                self.cursor_position = self.input.len();
            }
            _ => {}
        }
    }

    pub fn handle_mouse(&mut self, mouse: MouseEvent) {
        if mouse.kind == MouseEventKind::Down(MouseButton::Left) {
            let col = mouse.column;
            let row = mouse.row;

            // Check if click was inside queue block (excluding borders)
            if col > self.queue_rect.x
                && col < self.queue_rect.x + self.queue_rect.width.saturating_sub(1)
                && row > self.queue_rect.y
                && row < self.queue_rect.y + self.queue_rect.height.saturating_sub(1)
            {
                let relative_row = (row - self.queue_rect.y - 1) as usize;
                let inner_width = self.queue_rect.width.saturating_sub(2) as usize;
                let mut current_row = 0;
                for item in self.supervisor.queue.all_items() {
                    let item_height = compute_item_height(item, inner_width);
                    if relative_row >= current_row && relative_row < current_row + item_height {
                        if item.status == ItemStatus::Queued {
                            self.start_editing(item.id);
                        } else {
                            self.add_log(format!(
                                "! Cannot edit prompt #{} (status: {:?})",
                                item.id, item.status
                            ));
                        }
                        return;
                    }
                    current_row += item_height;
                }
            }
        }
    }

    pub fn tick_supervisor(&mut self) {
        match self.supervisor.tick() {
            Ok(events) => {
                for ev in events {
                    self.add_log(ev);
                }
            }
            Err(e) => {
                self.add_log(format!("! Supervisor: {}", e));
            }
        }
    }
}

fn current_time_str() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let hours = (now / 3600) % 24;
    let mins = (now / 60) % 60;
    let secs = now % 60;
    format!("{:02}:{:02}:{:02}", hours, mins, secs)
}

struct TerminalCleanup;

impl Drop for TerminalCleanup {
    fn drop(&mut self) {
        let _ = execute!(stdout(), LeaveAlternateScreen, DisableMouseCapture);
        let _ = disable_raw_mode();
    }
}

struct HeaderItem<'a> {
    spans: Vec<Span<'a>>,
    width: usize,
}

fn pack_header_group<'a>(items: Vec<HeaderItem<'a>>, inner_width: usize) -> Vec<Line<'a>> {
    let mut lines = Vec::new();
    let mut current_spans: Vec<Span<'a>> = Vec::new();
    let mut current_width = 0;
    let sep_str = "  •  ";
    let sep_len = 5;

    for item in items {
        if current_spans.is_empty() {
            current_width = item.width;
            current_spans = item.spans;
        } else if current_width + sep_len + item.width <= inner_width {
            current_spans.push(Span::styled(sep_str, Style::default().fg(Color::DarkGray)));
            current_spans.extend(item.spans);
            current_width += sep_len + item.width;
        } else {
            lines.push(Line::from(current_spans));
            current_width = item.width;
            current_spans = item.spans;
        }
    }

    if !current_spans.is_empty() {
        lines.push(Line::from(current_spans));
    }

    lines
}

pub fn compute_header_lines<'a>(
    terminal_name: &'a str,
    mode_str: &'a str,
    state_badge: Span<'a>,
    state_badge_len: usize,
    target: Option<&'a AgentTarget>,
    usage_summary: Option<&'a str>,
    inner_width: usize,
) -> Vec<Line<'a>> {
    let item_title = HeaderItem {
        spans: vec![Span::styled(
            "Harness Queue (hq) v0.1",
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        )],
        width: 23,
    };

    let item_host = HeaderItem {
        spans: vec![
            Span::styled("Host: ", Style::default().fg(Color::DarkGray)),
            Span::styled(terminal_name, Style::default().fg(Color::Yellow)),
        ],
        width: 6 + terminal_name.chars().count(),
    };

    let item_mode = HeaderItem {
        spans: vec![
            Span::styled("Mode: ", Style::default().fg(Color::DarkGray)),
            Span::styled(mode_str, Style::default().fg(Color::Cyan)),
        ],
        width: 6 + mode_str.chars().count(),
    };

    let item_status = HeaderItem {
        spans: vec![
            Span::styled("Status: ", Style::default().fg(Color::DarkGray)),
            state_badge,
        ],
        width: 8 + state_badge_len,
    };

    let group1 = vec![item_title, item_host, item_mode, item_status];

    let mut group2 = Vec::new();
    if let Some(t) = target {
        let target_display = format!("{} ({})", t.harness.display_name(), t.short_session_id());
        let target_width = 8 + target_display.chars().count();
        group2.push(HeaderItem {
            spans: vec![
                Span::styled("Target: ", Style::default().fg(Color::DarkGray)),
                Span::styled(target_display, Style::default().fg(Color::White)),
            ],
            width: target_width,
        });

        let cwd_width = 10 + t.cwd.chars().count();
        group2.push(HeaderItem {
            spans: vec![
                Span::styled("Worktree: ", Style::default().fg(Color::DarkGray)),
                Span::styled(t.cwd.clone(), Style::default().fg(Color::DarkGray)),
            ],
            width: cwd_width,
        });
    } else {
        group2.push(HeaderItem {
            spans: vec![
                Span::styled("Target: ", Style::default().fg(Color::DarkGray)),
                Span::styled("None", Style::default().fg(Color::DarkGray)),
            ],
            width: 12,
        });
    }

    if let Some(usage) = usage_summary {
        let usage_width = 7 + usage.chars().count();
        group2.push(HeaderItem {
            spans: vec![
                Span::styled("Usage: ", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    usage.to_string(),
                    Style::default()
                        .fg(Color::LightCyan)
                        .add_modifier(Modifier::BOLD),
                ),
            ],
            width: usage_width,
        });
    }

    let mut header_lines = pack_header_group(group1, inner_width);
    let mut group2_lines = pack_header_group(group2, inner_width);
    header_lines.append(&mut group2_lines);

    header_lines
}

fn safe_char_take(s: &str, max_chars: usize) -> (&str, &str) {
    if max_chars == 0 {
        return ("", s);
    }
    match s.char_indices().nth(max_chars) {
        Some((byte_idx, _)) => (&s[..byte_idx], &s[byte_idx..]),
        None => (s, ""),
    }
}

pub fn wrap_text(text: &str, max_width: usize) -> Vec<String> {
    if max_width == 0 {
        return vec![text.to_string()];
    }
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return vec![String::new()];
    }

    let mut lines = Vec::new();
    let mut current_line = String::new();
    let mut current_len = 0;

    for word in trimmed.split_whitespace() {
        let word_chars = word.chars().count();
        if current_line.is_empty() {
            if word_chars <= max_width {
                current_line.push_str(word);
                current_len = word_chars;
            } else {
                let mut rem = word;
                while !rem.is_empty() {
                    let (head, tail) = safe_char_take(rem, max_width);
                    if tail.is_empty() {
                        current_line.push_str(head);
                        current_len = head.chars().count();
                    } else {
                        lines.push(head.to_string());
                    }
                    rem = tail;
                }
            }
        } else if current_len + 1 + word_chars <= max_width {
            current_line.push(' ');
            current_line.push_str(word);
            current_len += 1 + word_chars;
        } else {
            lines.push(current_line);
            current_line = String::new();
            current_len = 0;

            if word_chars <= max_width {
                current_line.push_str(word);
                current_len = word_chars;
            } else {
                let mut rem = word;
                while !rem.is_empty() {
                    let (head, tail) = safe_char_take(rem, max_width);
                    if tail.is_empty() {
                        current_line.push_str(head);
                        current_len = head.chars().count();
                    } else {
                        lines.push(head.to_string());
                    }
                    rem = tail;
                }
            }
        }
    }

    if !current_line.is_empty() {
        lines.push(current_line);
    }
    if lines.is_empty() {
        lines.push(String::new());
    }
    lines
}

pub fn wrap_log_line(log_str: &str, max_width: usize) -> Vec<Line<'static>> {
    let style = if log_str.contains('✓') {
        Style::default().fg(Color::Green)
    } else if log_str.contains('➤') {
        Style::default().fg(Color::Magenta)
    } else if log_str.contains('◷') || log_str.contains('⏳') {
        Style::default().fg(Color::Yellow)
    } else if log_str.contains('!') {
        Style::default().fg(Color::Red)
    } else if log_str.contains('✎') {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(Color::Gray)
    };

    if max_width == 0 {
        return vec![Line::from(Span::styled(log_str.to_string(), style))];
    }
    if log_str.chars().count() <= max_width {
        return vec![Line::from(Span::styled(log_str.to_string(), style))];
    }

    let has_timestamp = log_str.len() >= 9
        && log_str.as_bytes()[2] == b':'
        && log_str.as_bytes()[5] == b':'
        && log_str.as_bytes()[8] == b' ';

    if has_timestamp && max_width > 15 {
        let timestamp = &log_str[..9];
        let rest = &log_str[9..];
        let sub_width = max_width.saturating_sub(9);
        let wrapped_rest = wrap_text(rest, sub_width);
        let mut lines = Vec::new();
        for (idx, seg) in wrapped_rest.into_iter().enumerate() {
            if idx == 0 {
                lines.push(Line::from(vec![
                    Span::styled(timestamp.to_string(), style),
                    Span::styled(seg, style),
                ]));
            } else {
                lines.push(Line::from(vec![
                    Span::styled("         ".to_string(), style),
                    Span::styled(seg, style),
                ]));
            }
        }
        lines
    } else {
        let wrapped = wrap_text(log_str, max_width);
        wrapped
            .into_iter()
            .map(|s| Line::from(Span::styled(s, style)))
            .collect()
    }
}

pub fn compute_queue_item_lines(
    item: &QueueItem,
    is_editing: bool,
    inner_width: usize,
) -> Vec<Line<'static>> {
    let (status_badge, status_style) = if is_editing {
        (
            "[✎ EDITING]",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )
    } else {
        match item.status {
            ItemStatus::Queued => ("[⏳ QUEUED]", Style::default().fg(Color::Yellow)),
            ItemStatus::Submitting => ("[➤ SENDING]", Style::default().fg(Color::Magenta)),
            ItemStatus::Acknowledged => ("[✓ SENT]", Style::default().fg(Color::Green)),
            ItemStatus::Uncertain => ("[✕ PAUSED]", Style::default().fg(Color::Red)),
        }
    };

    let prefix = format!("{} #{} ", status_badge, item.id);
    let prefix_len = prefix.chars().count();

    let mut lines = Vec::new();
    let prompt_text = item.prompt.trim();

    if inner_width <= prefix_len + 4 {
        lines.push(Line::from(Span::styled(prefix, status_style)));
        let wrapped_prompt = wrap_text(prompt_text, inner_width.saturating_sub(2).max(1));
        for seg in wrapped_prompt {
            lines.push(Line::from(Span::raw(format!("  {}", seg))));
        }
    } else {
        let available_first_line = inner_width.saturating_sub(prefix_len);
        let continuation_indent_len = prefix_len.min(6);
        let continuation_indent = " ".repeat(continuation_indent_len);
        let available_next_lines = inner_width.saturating_sub(continuation_indent_len).max(1);

        let mut words = prompt_text.split_whitespace().peekable();
        let mut line1 = String::new();
        while let Some(&w) = words.peek() {
            let add_len = if line1.is_empty() {
                w.chars().count()
            } else {
                line1.chars().count() + 1 + w.chars().count()
            };
            if add_len <= available_first_line {
                if !line1.is_empty() {
                    line1.push(' ');
                }
                line1.push_str(w);
                words.next();
            } else {
                break;
            }
        }

        if line1.is_empty() {
            if let Some(w) = words.next() {
                let (part, _) = safe_char_take(w, available_first_line);
                line1.push_str(part);
            }
        }

        lines.push(Line::from(vec![
            Span::styled(prefix, status_style),
            Span::raw(line1),
        ]));

        let remaining_words: Vec<&str> = words.collect();
        if !remaining_words.is_empty() {
            let rem_text = remaining_words.join(" ");
            let wrapped_rem = wrap_text(&rem_text, available_next_lines);
            for seg in wrapped_rem {
                lines.push(Line::from(vec![
                    Span::raw(continuation_indent.clone()),
                    Span::raw(seg),
                ]));
            }
        }
    }

    if let Some(att) = &item.attachments_summary {
        let att_wrapped = wrap_text(att, inner_width.saturating_sub(4).max(1));
        for (i, seg) in att_wrapped.into_iter().enumerate() {
            let indent = if i == 0 { "   📎 " } else { "      " };
            lines.push(Line::from(vec![
                Span::raw(indent),
                Span::styled(seg, Style::default().fg(Color::Cyan)),
            ]));
        }
    }

    lines
}

pub fn compute_item_height(item: &QueueItem, inner_width: usize) -> usize {
    compute_queue_item_lines(item, false, inner_width).len()
}

pub fn run_tui(supervisor: &mut Supervisor) -> io::Result<()> {
    enable_raw_mode()?;
    let mut stdout = stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let _cleanup = TerminalCleanup;

    let mut app = TuiApp::new(supervisor);

    while app.running {
        terminal.draw(|f| {
            let size = f.area();
            let is_narrow = size.width < 80;
            let is_short = size.height < 24;

            let margin = if is_short || size.width < 75 { 0 } else { 1 };
            let inner_width = (size.width.saturating_sub(2 + (margin * 2))).max(20) as usize;
            let usage_summary = app.supervisor.get_target_usage_summary();

            let (state_badge, state_badge_len) = match app.supervisor.state_machine.current() {
                State::Watching => (
                    Span::styled(
                        " ● WATCHING ",
                        Style::default()
                            .fg(Color::Cyan)
                            .add_modifier(Modifier::BOLD),
                    ),
                    12,
                ),
                State::AgentBusy => (
                    Span::styled(
                        " ⚙️ BUSY ",
                        Style::default()
                            .fg(Color::Yellow)
                            .add_modifier(Modifier::BOLD),
                    ),
                    9,
                ),
                State::AgentReady => (
                    Span::styled(
                        " ✓ READY ",
                        Style::default()
                            .fg(Color::Green)
                            .add_modifier(Modifier::BOLD),
                    ),
                    9,
                ),
                State::Submitting => (
                    Span::styled(
                        " ➤ SENDING ",
                        Style::default()
                            .fg(Color::Magenta)
                            .add_modifier(Modifier::BOLD),
                    ),
                    11,
                ),
                State::PromptAcknowledged => (
                    Span::styled(
                        " ✓ DELIVERED ",
                        Style::default()
                            .fg(Color::Green)
                            .add_modifier(Modifier::BOLD),
                    ),
                    13,
                ),
                State::RateLimited | State::WaitingForRecovery => {
                    let reset_text = if let Some(ts) = app.supervisor.quota_resets_at {
                        let now = std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .map(|d| d.as_secs())
                            .unwrap_or(0);
                        if ts > now {
                            let diff = ts - now;
                            format!(" ⏳ QUOTA WAIT ({}h {}m) ", diff / 3600, (diff % 3600) / 60)
                        } else {
                            " ⏳ QUOTA WAIT (due) ".to_string()
                        }
                    } else {
                        " ⏳ QUOTA WAIT ".to_string()
                    };
                    let len = reset_text.chars().count();
                    (
                        Span::styled(
                            reset_text,
                            Style::default()
                                .fg(Color::LightRed)
                                .add_modifier(Modifier::BOLD),
                        ),
                        len,
                    )
                }
                State::DeliveryUncertain => (
                    Span::styled(
                        " ! UNCERTAIN ",
                        Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
                    ),
                    13,
                ),
                State::Detached => (
                    Span::styled(" ✓ DETACHED ", Style::default().fg(Color::DarkGray)),
                    12,
                ),
                _ => (
                    Span::styled(" ● ACTIVE ", Style::default().fg(Color::Blue)),
                    10,
                ),
            };

            let header_lines = compute_header_lines(
                app.supervisor.terminal.terminal_name(),
                app.supervisor.mode.as_str(),
                state_badge,
                state_badge_len,
                app.supervisor.target.as_ref(),
                usage_summary.as_deref(),
                inner_width,
            );

            let total_rows: usize = header_lines
                .iter()
                .map(|line| line.width().div_ceil(inner_width).max(1))
                .sum();

            let footer_height = if size.height < 16 { 0 } else { 1 };
            let input_height = 3;
            let min_middle_height = if size.height < 20 { 4 } else { 6 };
            let reserved = input_height + footer_height + min_middle_height + (margin * 2);
            let max_header = size.height.saturating_sub(reserved).max(3);
            let header_height = (total_rows as u16 + 2).min(max_header);

            // Main vertical layout:
            // 1. Header (height: header_height, dynamically computed from rows needed)
            // 2. Middle: Queue & Logs
            // 3. Input Box (height: 3)
            // 4. Footer (height: footer_height)
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .margin(margin)
                .constraints([
                    Constraint::Length(header_height),
                    Constraint::Min(min_middle_height),
                    Constraint::Length(input_height),
                    Constraint::Length(footer_height),
                ])
                .split(size);

            let header_block = Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(Color::DarkGray))
                .title(Span::styled(
                    " Session Overview ",
                    Style::default()
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD),
                ));
            let header_paragraph = Paragraph::new(header_lines)
                .block(header_block)
                .wrap(Wrap { trim: true });
            f.render_widget(header_paragraph, chunks[0]);

            // 2. Middle: Queue & Logs
            let all_items = app.supervisor.queue.all_items();
            let middle_rect = chunks[1];

            let middle_chunks = if is_narrow {
                // On narrow screens (e.g. mobile or split panes), stack vertically so both boxes have full width!
                let queue_inner_width = middle_rect.width.saturating_sub(2) as usize;
                let queue_needed_height = if all_items.is_empty() {
                    let placeholder_lines = wrap_text(
                        "(No prompts in queue. Type below and press Enter)",
                        queue_inner_width.saturating_sub(1).max(1),
                    )
                    .len();
                    (placeholder_lines as u16 + 2).min(middle_rect.height.saturating_sub(3).max(3))
                } else {
                    let total_item_lines: usize = all_items
                        .iter()
                        .map(|item| compute_item_height(item, queue_inner_width))
                        .sum();
                    let max_queue_h = (middle_rect.height / 2).max(3);
                    ((total_item_lines as u16) + 2).min(max_queue_h).max(3)
                };

                Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([Constraint::Length(queue_needed_height), Constraint::Min(3)])
                    .split(middle_rect)
            } else {
                // On wide screens, display side-by-side (50% / 50%)
                Layout::default()
                    .direction(Direction::Horizontal)
                    .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
                    .split(middle_rect)
            };

            // Track queue panel coordinates for mouse click detection
            app.queue_rect = middle_chunks[0];
            let queue_rect = middle_chunks[0];
            let log_rect = middle_chunks[1];

            // Queue List
            let queue_title = format!(" Prompt Queue ({}) ", all_items.len());
            let queue_inner_width = queue_rect.width.saturating_sub(2) as usize;
            let queue_items: Vec<ListItem> = if all_items.is_empty() {
                let placeholder = "(No prompts in queue. Type below and press Enter)";
                let wrapped_placeholder =
                    wrap_text(placeholder, queue_inner_width.saturating_sub(1).max(1));
                vec![ListItem::new(
                    wrapped_placeholder
                        .into_iter()
                        .map(|line| {
                            Line::from(Span::styled(
                                format!(" {}", line),
                                Style::default().fg(Color::DarkGray),
                            ))
                        })
                        .collect::<Vec<_>>(),
                )]
            } else {
                all_items
                    .iter()
                    .map(|item| {
                        let is_editing = app.editing_item_id == Some(item.id);
                        let lines = compute_queue_item_lines(item, is_editing, queue_inner_width);
                        ListItem::new(lines)
                    })
                    .collect()
            };

            let queue_border_style = if app.editing_item_id.is_some() {
                Style::default().fg(Color::Yellow)
            } else {
                Style::default().fg(Color::DarkGray)
            };

            let queue_block = Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(queue_border_style)
                .title(Span::styled(
                    queue_title,
                    Style::default()
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD),
                ));
            let queue_widget = List::new(queue_items).block(queue_block);
            f.render_widget(queue_widget, queue_rect);

            // Event Logs List
            let log_title = " Real-time Event Log ";
            let log_inner_width = log_rect.width.saturating_sub(2) as usize;
            let log_inner_height = log_rect.height.saturating_sub(2) as usize;

            let mut display_items: Vec<ListItem> = Vec::new();
            let mut total_rendered_lines = 0;

            for log_str in app.logs.iter().rev() {
                let lines = wrap_log_line(log_str, log_inner_width);
                let count = lines.len();
                if total_rendered_lines + count > log_inner_height && total_rendered_lines > 0 {
                    break;
                }
                total_rendered_lines += count;
                display_items.push(ListItem::new(lines));
                if total_rendered_lines >= log_inner_height {
                    break;
                }
            }

            display_items.reverse();

            let log_block = Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(Color::DarkGray))
                .title(Span::styled(
                    log_title,
                    Style::default()
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD),
                ));
            let log_widget = List::new(display_items).block(log_block);
            f.render_widget(log_widget, log_rect);

            // 3. Input Box
            let (input_title, border_color) = if let Some(id) = app.editing_item_id {
                let title = if size.width < 60 {
                    format!(" › [EDITING #{}] (Enter: save, Esc: cancel) ", id)
                } else {
                    format!(
                        " › [EDITING PROMPT #{}] (Enter to save, Esc to cancel) ",
                        id
                    )
                };
                (title, Color::Yellow)
            } else {
                let title = if size.width < 60 {
                    " › Add prompt (Enter to queue) ".to_string()
                } else {
                    " › Add prompt or paste image path (Enter to queue) ".to_string()
                };
                (title, Color::Cyan)
            };

            let input_text = if app.input.is_empty() {
                let placeholder = if size.width < 50 {
                    "Type prompt or @/path/to/img..."
                } else {
                    "Type prompt or paste @/path/to/img.png here..."
                };
                Span::styled(placeholder, Style::default().fg(Color::DarkGray))
            } else {
                Span::styled(app.input.clone(), Style::default().fg(Color::White))
            };

            let input_block = Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(border_color))
                .title(Span::styled(
                    input_title,
                    Style::default()
                        .fg(border_color)
                        .add_modifier(Modifier::BOLD),
                ));
            let input_widget = Paragraph::new(Line::from(input_text)).block(input_block);
            f.render_widget(input_widget, chunks[2]);

            // Position cursor in the input box
            let cursor_x = chunks[2].x + 1 + app.cursor_position as u16;
            let cursor_y = chunks[2].y + 1;
            f.set_cursor_position((cursor_x, cursor_y));

            // 4. Footer
            if footer_height > 0 {
                let footer_text = if app.editing_item_id.is_some() {
                    if size.width < 65 {
                        vec![
                            Span::styled(
                                " [Enter] ",
                                Style::default()
                                    .fg(Color::Yellow)
                                    .add_modifier(Modifier::BOLD),
                            ),
                            Span::raw("Save  • "),
                            Span::styled(
                                " [Esc] ",
                                Style::default()
                                    .fg(Color::Yellow)
                                    .add_modifier(Modifier::BOLD),
                            ),
                            Span::raw("Cancel"),
                        ]
                    } else {
                        vec![
                            Span::styled(
                                " [Enter] ",
                                Style::default()
                                    .fg(Color::Yellow)
                                    .add_modifier(Modifier::BOLD),
                            ),
                            Span::raw("Save update  • "),
                            Span::styled(
                                " [Esc] ",
                                Style::default()
                                    .fg(Color::Yellow)
                                    .add_modifier(Modifier::BOLD),
                            ),
                            Span::raw("Cancel edit  • "),
                            Span::styled(
                                " [Ctrl+C] ",
                                Style::default()
                                    .fg(Color::Yellow)
                                    .add_modifier(Modifier::BOLD),
                            ),
                            Span::raw("Detach cleanly"),
                        ]
                    }
                } else if size.width < 65 {
                    vec![
                        Span::styled(
                            " [Enter] ",
                            Style::default()
                                .fg(Color::Yellow)
                                .add_modifier(Modifier::BOLD),
                        ),
                        Span::raw("Queue  • "),
                        Span::styled(
                            " [Shift+←] ",
                            Style::default()
                                .fg(Color::Cyan)
                                .add_modifier(Modifier::BOLD),
                        ),
                        Span::raw("Edit  • "),
                        Span::styled(
                            " [Esc] ",
                            Style::default()
                                .fg(Color::Yellow)
                                .add_modifier(Modifier::BOLD),
                        ),
                        Span::raw("Detach"),
                    ]
                } else {
                    vec![
                        Span::styled(
                            " [Enter] ",
                            Style::default()
                                .fg(Color::Yellow)
                                .add_modifier(Modifier::BOLD),
                        ),
                        Span::raw("Queue prompt  • "),
                        Span::styled(
                            " [Shift+←] ",
                            Style::default()
                                .fg(Color::Cyan)
                                .add_modifier(Modifier::BOLD),
                        ),
                        Span::raw("or "),
                        Span::styled(
                            "Click queue item ",
                            Style::default()
                                .fg(Color::Cyan)
                                .add_modifier(Modifier::BOLD),
                        ),
                        Span::raw("to edit  • "),
                        Span::styled(
                            " [Esc] ",
                            Style::default()
                                .fg(Color::Yellow)
                                .add_modifier(Modifier::BOLD),
                        ),
                        Span::raw("Detach"),
                    ]
                };
                let footer_paragraph = Paragraph::new(Line::from(footer_text));
                f.render_widget(footer_paragraph, chunks[3]);
            }
        })?;

        // Handle keyboard and mouse events with 100ms timeout
        if event::poll(Duration::from_millis(100))? {
            match event::read()? {
                Event::Key(key) => {
                    app.handle_key(key);
                }
                Event::Mouse(mouse) => {
                    app.handle_mouse(mouse);
                }
                _ => {}
            }
        }

        // Supervisor tick every 500ms
        if app.last_tick.elapsed() >= Duration::from_millis(500) {
            app.tick_supervisor();
            app.last_tick = Instant::now();
        }
    }

    let _ = app.supervisor.state_machine.transition(State::Detached);
    Ok(())
}
