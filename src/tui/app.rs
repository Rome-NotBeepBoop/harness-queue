use crate::image::{describe_attachments, extract_image_references, format_prompt_with_images};
use crate::models::{ItemStatus, State};
use crate::supervisor::Supervisor;
use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, List, ListItem, Paragraph},
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
        let id = self
            .supervisor
            .enqueue_prompt_with_attachments(formatted, attachments_desc.clone());

        if let Some(desc) = attachments_desc {
            self.add_log(format!("✓ Prompt #{} queued {}", id, desc));
        } else {
            self.add_log(format!("✓ Prompt #{} queued", id));
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

        match key_event.code {
            KeyCode::Esc => {
                self.running = false;
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
        let _ = disable_raw_mode();
        let _ = execute!(stdout(), LeaveAlternateScreen);
    }
}

pub fn run_tui(supervisor: &mut Supervisor) -> io::Result<()> {
    enable_raw_mode()?;
    let mut stdout = stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let _cleanup = TerminalCleanup;

    let mut app = TuiApp::new(supervisor);

    while app.running {
        terminal.draw(|f| {
            let size = f.area();

            // Main vertical layout:
            // 1. Header (height: 4)
            // 2. Middle: Queue (left) & Logs (right)
            // 3. Input Box (height: 3)
            // 4. Footer (height: 1)
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .margin(1)
                .constraints([
                    Constraint::Length(4),
                    Constraint::Min(8),
                    Constraint::Length(3),
                    Constraint::Length(1),
                ])
                .split(size);

            // 1. Header
            let target_desc = if let Some(target) = &app.supervisor.target {
                format!(
                    "Target: {} ({})  •  Worktree: {}",
                    target.harness.display_name(),
                    target.short_session_id(),
                    target.cwd
                )
            } else {
                "Target: None".to_string()
            };

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

            let header_lines = vec![
                Line::from(vec![
                    Span::styled(
                        "Harness Queue (hq) v0.1",
                        Style::default()
                            .fg(Color::White)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::raw("  •  Host: "),
                    Span::styled(
                        app.supervisor.terminal.terminal_name(),
                        Style::default().fg(Color::Yellow),
                    ),
                    Span::raw("  •  Mode: "),
                    Span::styled(
                        app.supervisor.mode.as_str(),
                        Style::default().fg(Color::Cyan),
                    ),
                    Span::raw("  •  Status: "),
                    state_badge,
                ]),
                Line::from(Span::styled(
                    target_desc,
                    Style::default().fg(Color::DarkGray),
                )),
            ];

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
            let header_paragraph = Paragraph::new(header_lines).block(header_block);
            f.render_widget(header_paragraph, chunks[0]);

            // 2. Middle: Queue (left) & Logs (right)
            let middle_chunks = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
                .split(chunks[1]);

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
                        let (status_badge, status_style) = match item.status {
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

            let queue_block = Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(Color::DarkGray))
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
            let input_title = " › Add prompt or paste image path (Enter to queue) ";
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
                .border_style(Style::default().fg(Color::Cyan))
                .title(Span::styled(
                    input_title,
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                ));
            let input_widget = Paragraph::new(Line::from(input_text)).block(input_block);
            f.render_widget(input_widget, chunks[2]);

            // Position cursor in the input box
            let cursor_x = chunks[2].x + 1 + app.cursor_position as u16;
            let cursor_y = chunks[2].y + 1;
            f.set_cursor_position((cursor_x, cursor_y));

            // 4. Footer
            let footer_text = vec![
                Span::styled(
                    " [Enter] ",
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw("Queue prompt  • "),
                Span::styled(
                    " [Esc] ",
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw("or "),
                Span::styled(
                    " [Ctrl+C] ",
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw("Detach cleanly"),
            ];
            let footer_paragraph = Paragraph::new(Line::from(footer_text));
            f.render_widget(footer_paragraph, chunks[3]);
        })?;

        // Handle events with 100ms timeout
        if event::poll(Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                app.handle_key(key);
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
