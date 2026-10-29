use crate::image::{describe_attachments, extract_image_references, format_prompt_with_images};
use crate::models::{ItemStatus, State};
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
                let mut current_row = 0;
                for item in self.supervisor.queue.all_items() {
                    let item_height = if item.attachments_summary.is_some() {
                        2
                    } else {
                        1
                    };
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

            let usage_summary = app.supervisor.get_target_usage_summary();
            let header_height = if size.height < 24 {
                if usage_summary.is_some() {
                    7
                } else {
                    6
                }
            } else if usage_summary.is_some() {
                8
            } else {
                7
            };

            // Main vertical layout:
            // 1. Header (height: header_height)
            // 2. Middle: Queue (left) & Logs (right)
            // 3. Input Box (height: 3)
            // 4. Footer (height: 1)
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .margin(1)
                .constraints([
                    Constraint::Length(header_height),
                    Constraint::Min(8),
                    Constraint::Length(3),
                    Constraint::Length(1),
                ])
                .split(size);

            let state_badge = match app.supervisor.state_machine.current() {
                State::Watching => Span::styled(
                    " ● WATCHING ",
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                ),
                State::AgentBusy => Span::styled(
                    " ⚙️ BUSY ",
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                ),
                State::AgentReady => Span::styled(
                    " ✓ READY ",
                    Style::default()
                        .fg(Color::Green)
                        .add_modifier(Modifier::BOLD),
                ),
                State::Submitting => Span::styled(
                    " ➤ SENDING ",
                    Style::default()
                        .fg(Color::Magenta)
                        .add_modifier(Modifier::BOLD),
                ),
                State::PromptAcknowledged => Span::styled(
                    " ✓ DELIVERED ",
                    Style::default()
                        .fg(Color::Green)
                        .add_modifier(Modifier::BOLD),
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
                    Span::styled(
                        reset_text,
                        Style::default()
                            .fg(Color::LightRed)
                            .add_modifier(Modifier::BOLD),
                    )
                }
                State::DeliveryUncertain => Span::styled(
                    " ! UNCERTAIN ",
                    Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
                ),
                State::Detached => {
                    Span::styled(" ✓ DETACHED ", Style::default().fg(Color::DarkGray))
                }
                _ => Span::styled(" ● ACTIVE ", Style::default().fg(Color::Blue)),
            };

            let mut header_lines = Vec::new();

            // Line 1: App title & Host
            header_lines.push(Line::from(vec![
                Span::styled(
                    "Harness Queue (hq) v0.1",
                    Style::default()
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled("  •  Host: ", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    app.supervisor.terminal.terminal_name(),
                    Style::default().fg(Color::Yellow),
                ),
            ]));

            // Line 2: Mode & Status
            header_lines.push(Line::from(vec![
                Span::styled("Mode: ", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    app.supervisor.mode.as_str(),
                    Style::default().fg(Color::Cyan),
                ),
                Span::styled("  •  Status: ", Style::default().fg(Color::DarkGray)),
                state_badge,
            ]));

            // Line 3: Target
            if let Some(target) = &app.supervisor.target {
                header_lines.push(Line::from(vec![
                    Span::styled("Target: ", Style::default().fg(Color::DarkGray)),
                    Span::styled(
                        format!(
                            "{} ({})",
                            target.harness.display_name(),
                            target.short_session_id()
                        ),
                        Style::default().fg(Color::White),
                    ),
                ]));

                // Line 4: Worktree
                header_lines.push(Line::from(vec![
                    Span::styled("Worktree: ", Style::default().fg(Color::DarkGray)),
                    Span::styled(&target.cwd, Style::default().fg(Color::DarkGray)),
                ]));
            } else {
                header_lines.push(Line::from(vec![
                    Span::styled("Target: ", Style::default().fg(Color::DarkGray)),
                    Span::styled("None", Style::default().fg(Color::DarkGray)),
                ]));
            }

            // Line 5: Usage (Codex, Claude Code, AGY only)
            if let Some(ref usage) = usage_summary {
                header_lines.push(Line::from(vec![
                    Span::styled("Usage: ", Style::default().fg(Color::DarkGray)),
                    Span::styled(
                        usage,
                        Style::default()
                            .fg(Color::LightCyan)
                            .add_modifier(Modifier::BOLD),
                    ),
                ]));
            }

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

            // 2. Middle: Queue (left) & Logs (right)
            let middle_chunks = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
                .split(chunks[1]);

            // Track queue panel coordinates for mouse click detection
            app.queue_rect = middle_chunks[0];

            // Queue List
            let all_items = app.supervisor.queue.all_items();
            let queue_title = format!(" Prompt Queue ({}) ", all_items.len());
            let queue_items: Vec<ListItem> = if all_items.is_empty() {
                vec![ListItem::new(Line::from(Span::styled(
                    " (No prompts in queue. Type below and press Enter)",
                    Style::default().fg(Color::DarkGray),
                )))]
            } else {
                all_items
                    .iter()
                    .map(|item| {
                        let is_editing = app.editing_item_id == Some(item.id);
                        let (status_badge, status_style) = if is_editing {
                            (
                                "[✎ EDITING]",
                                Style::default()
                                    .fg(Color::Yellow)
                                    .add_modifier(Modifier::BOLD),
                            )
                        } else {
                            match item.status {
                                ItemStatus::Queued => {
                                    ("[⏳ QUEUED]", Style::default().fg(Color::Yellow))
                                }
                                ItemStatus::Submitting => {
                                    ("[➤ SENDING]", Style::default().fg(Color::Magenta))
                                }
                                ItemStatus::Acknowledged => {
                                    ("[✓ SENT]", Style::default().fg(Color::Green))
                                }
                                ItemStatus::Uncertain => {
                                    ("[✕ PAUSED]", Style::default().fg(Color::Red))
                                }
                            }
                        };

                        let prompt_snippet = if item.prompt.len() > 40 {
                            format!("{}…", &item.prompt[..40])
                        } else {
                            item.prompt.clone()
                        };

                        let mut lines = vec![Line::from(vec![
                            Span::styled(format!("{} #{} ", status_badge, item.id), status_style),
                            Span::raw(prompt_snippet),
                        ])];

                        if let Some(att) = &item.attachments_summary {
                            lines.push(Line::from(vec![
                                Span::raw("   "),
                                Span::styled(att.clone(), Style::default().fg(Color::Cyan)),
                            ]));
                        }

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
            f.render_widget(queue_widget, middle_chunks[0]);

            // Event Logs List
            let log_title = " Real-time Event Log ";
            let max_logs_visible = middle_chunks[1].height.saturating_sub(2) as usize;
            let display_logs: Vec<ListItem> = app
                .logs
                .iter()
                .rev()
                .take(max_logs_visible)
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .map(|log_str| {
                    let style = if log_str.contains("✓") {
                        Style::default().fg(Color::Green)
                    } else if log_str.contains("➤") {
                        Style::default().fg(Color::Magenta)
                    } else if log_str.contains("◷") || log_str.contains("⏳") {
                        Style::default().fg(Color::Yellow)
                    } else if log_str.contains('!') {
                        Style::default().fg(Color::Red)
                    } else if log_str.contains('✎') {
                        Style::default().fg(Color::Cyan)
                    } else {
                        Style::default().fg(Color::Gray)
                    };
                    ListItem::new(Line::from(Span::styled(log_str.clone(), style)))
                })
                .collect();

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
            let log_widget = List::new(display_logs).block(log_block);
            f.render_widget(log_widget, middle_chunks[1]);

            // 3. Input Box
            let (input_title, border_color) = if let Some(id) = app.editing_item_id {
                (
                    format!(
                        " › [EDITING PROMPT #{}] (Enter to save, Esc to cancel) ",
                        id
                    ),
                    Color::Yellow,
                )
            } else {
                (
                    " › Add prompt or paste image path (Enter to queue) ".to_string(),
                    Color::Cyan,
                )
            };

            let input_text = if app.input.is_empty() {
                Span::styled(
                    "Type prompt or paste @/path/to/img.png here...",
                    Style::default().fg(Color::DarkGray),
                )
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
            let footer_text = if app.editing_item_id.is_some() {
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
