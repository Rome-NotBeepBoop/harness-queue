use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode};
use harness_queue::cli::{
    format_boxed_header, format_mode_prompt, format_not_detected_guide, read_initial_input,
    read_prompt_input, select_ambiguous_target, InitialChoice,
};
use harness_queue::harness::create_harness_adapter;
use harness_queue::image::{
    describe_attachments, extract_image_references, format_prompt_with_images,
};
use harness_queue::models::{AgentTarget, ItemStatus, Mode, DEFAULT_CONTINUATION_PROMPT};
use harness_queue::supervisor::Supervisor;
use harness_queue::terminal::ghostty::GhosttyAdapter;
use harness_queue::terminal::herdr::HerdrAdapter;
use harness_queue::terminal::orca::OrcaAdapter;
use harness_queue::terminal::{NeighborDetection, TerminalAdapter};
use harness_queue::tui::run_tui;
use std::env;
use std::io::{self, BufReader, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

static RUNNING: AtomicBool = AtomicBool::new(true);
static ATTACHED: AtomicBool = AtomicBool::new(false);

extern "C" fn sigint_handler(_: i32) {
    if !ATTACHED.load(Ordering::SeqCst) {
        unsafe {
            let msg = b"\n\n\xe2\x9c\x93 Cancelled\n";
            let _ = libc::write(1, msg.as_ptr() as *const libc::c_void, msg.len());
            libc::_exit(0);
        }
    } else {
        RUNNING.store(false, Ordering::SeqCst);
    }
}

struct RawModeGuard;

impl Drop for RawModeGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
    }
}

fn render_cli_prompt(out: &mut io::Stdout, editing_id: Option<usize>, buffer: &str, cursor: usize) {
    let label = if let Some(id) = editing_id {
        format!("› [EDITING #{}] (Enter: save, Esc: cancel): ", id)
    } else {
        "› Next prompt (or image path) [Shift+← to edit]: ".to_string()
    };
    let _ = write!(out, "\r\x1b[K{}{}", label, buffer);
    let cursor_offset = buffer.len().saturating_sub(cursor);
    if cursor_offset > 0 {
        let _ = write!(out, "\x1b[{}D", cursor_offset);
    }
    let _ = out.flush();
}

fn detect_terminal_environment() -> Option<Arc<dyn TerminalAdapter>> {
    let orca = Arc::new(OrcaAdapter::new());
    let herdr = Arc::new(HerdrAdapter::new());
    let ghostty = Arc::new(GhosttyAdapter::new());

    let in_orca_env = env::var("TERM_PROGRAM")
        .map(|v| v == "Orca")
        .unwrap_or(false)
        || env::var("ORCA_TERMINAL_HANDLE")
            .map(|v| !v.trim().is_empty())
            .unwrap_or(false);

    let in_herdr_env = env::var("HERDR_PANE_ID")
        .map(|v| !v.trim().is_empty())
        .unwrap_or(false)
        || env::var("HERDR_SOCKET")
            .map(|v| !v.trim().is_empty())
            .unwrap_or(false)
        || env::var("HERDR_ENV").map(|v| v == "1").unwrap_or(false);

    let in_ghostty_env = env::var("TERM_PROGRAM")
        .map(|v| v == "ghostty")
        .unwrap_or(false)
        || env::var("GHOSTTY_RESOURCES_DIR")
            .map(|v| !v.trim().is_empty())
            .unwrap_or(false);

    if in_orca_env && orca.is_available() {
        return Some(orca);
    }

    if in_herdr_env && herdr.is_available() {
        return Some(herdr);
    }

    if in_ghostty_env && ghostty.is_available() {
        return Some(ghostty);
    }

    if orca.detect_current_pane().is_ok() {
        return Some(orca);
    }

    if herdr.detect_current_pane().is_ok() {
        return Some(herdr);
    }

    if ghostty.detect_current_pane().is_ok() {
        return Some(ghostty);
    }

    None
}

fn main() {
    unsafe {
        libc::signal(libc::SIGINT, sigint_handler as *const () as usize); // SIGINT (Ctrl+C)
    }

    let args: Vec<String> = env::args().collect();
    if args.iter().any(|a| a == "-h" || a == "--help") {
        println!("Harness Queue (hq) — Terminal-based coding agent queue\n");
        println!("USAGE:");
        println!("  hq [PROMPT]        Launch hq in default CLI mode");
        println!("  hq --cli [PROMPT]  Launch hq explicitly in CLI mode");
        println!("  hq --tui [PROMPT]  Launch hq in interactive TUI mode");
        println!("  hq --help          Show this help text\n");
        println!("PROMPT EDITING:");
        println!("  • Press [Shift + ←] (or click in TUI) to edit any queued prompt.");
        println!("  • Press [Enter] to save, [Esc] to cancel.\n");
        println!("IMAGE SUPPORT:");
        println!("  Paste image paths (e.g. @/path/to/img.png or /path/to/img.png).");
        println!(
            "  Images are automatically detected, validated, and forwarded to multimodal agents.\n"
        );
        println!("ENVIRONMENTS:");
        println!("  • Orca IDE         Native terminal API & split panes");
        println!("  • Herdr            Terminal pane manager & agent IPC");
        println!("  • Ghostty          macOS terminal emulator & split panes\n");
        println!("SUPPORTED AGENTS:");
        println!("  Codex, Claude Code, OpenCode, OMP, Pi, Google Antigravity (AGY)");
        return;
    }

    let is_tui = args.iter().any(|a| a == "--tui");
    let positional_args: Vec<String> = args
        .iter()
        .skip(1)
        .filter(|a| *a != "--tui" && *a != "--cli")
        .cloned()
        .collect();

    let cli_initial_prompt: Option<String> = if !positional_args.is_empty() {
        let joined = positional_args.join(" ");
        let trimmed = joined.trim().to_string();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed)
        }
    } else {
        None
    };

    // 1. Detect terminal adapter (Orca IDE, Herdr, or Ghostty)
    let terminal = match detect_terminal_environment() {
        Some(t) => t,
        None => {
            eprint!("{}", format_not_detected_guide());
            std::process::exit(1);
        }
    };

    // 2. Discover caller terminal pane and adjacent agent
    let current_pane = match terminal.detect_current_pane() {
        Ok(cp) => cp,
        Err(e) => {
            eprintln!("Error detecting current terminal pane: {}", e);
            std::process::exit(1);
        }
    };

    let target: AgentTarget = match terminal.find_adjacent_agent(&current_pane) {
        Ok(NeighborDetection::Exact(t)) => t,
        Ok(NeighborDetection::Ambiguous(list)) => {
            let stdout = io::stdout();
            let mut out = stdout.lock();
            let stdin = io::stdin();
            let mut reader = BufReader::new(stdin.lock());
            match select_ambiguous_target(&mut reader, &mut out, &list) {
                Some(selected) => selected,
                None => {
                    let _ = writeln!(out, "\n✓ Cancelled");
                    let _ = out.flush();
                    std::process::exit(0);
                }
            }
        }
        Ok(NeighborDetection::NoneFound(msg)) => {
            eprintln!("Error: {}", msg);
            std::process::exit(1);
        }
        Err(e) => {
            eprintln!("Terminal error: {}", e);
            std::process::exit(1);
        }
    };

    let harness = Arc::from(create_harness_adapter(&target.harness));

    // --- TUI MODE ---
    if is_tui {
        let mut supervisor = Supervisor::new(terminal, harness, Mode::AutoSend);
        supervisor.target = Some(target.clone());
        let _ = supervisor
            .state_machine
            .transition(harness_queue::models::State::SelectingMode);
        let _ = supervisor
            .state_machine
            .transition(harness_queue::models::State::Watching);

        if let Some(prompt_text) = cli_initial_prompt {
            let images = extract_image_references(&prompt_text);
            let desc = if !images.is_empty() {
                Some(describe_attachments(&images))
            } else {
                None
            };
            let formatted = format_prompt_with_images(&prompt_text, &images, &target.harness);
            supervisor.enqueue_prompt_with_attachments(formatted, desc);
        }

        if let Err(e) = run_tui(&mut supervisor) {
            eprintln!("TUI error: {}", e);
            std::process::exit(1);
        }

        println!("\n✓ Detached cleanly");
        std::process::exit(0);
    }

    // --- CLI MODE (DEFAULT or --cli) ---
    let stdout = io::stdout();
    let mut out = stdout.lock();

    let usage_summary = harness.get_usage_summary(&target, terminal.as_ref());

    // Render clean boxed header
    let header = format_boxed_header(
        terminal.terminal_name(),
        &current_pane,
        &target,
        usage_summary.as_deref(),
    );
    let _ = writeln!(out, "\n{}", header);
    let _ = out.flush();

    // Mode and initial prompt determination
    let (selected_mode, initial_prompt) = if let Some(prompt) = cli_initial_prompt {
        (Mode::AutoSend, Some(prompt))
    } else {
        let _ = write!(out, "{}", format_mode_prompt());
        let _ = out.flush();

        let stdin = io::stdin();
        let mut reader = BufReader::new(stdin.lock());

        match read_initial_input(&mut reader) {
            InitialChoice::Cancelled => {
                let _ = writeln!(out, "\n✓ Cancelled");
                let _ = out.flush();
                std::process::exit(0);
            }
            InitialChoice::AutoResume => (Mode::AutoResume, None),
            InitialChoice::AutoSend(None) => {
                let p =
                    read_prompt_input(&mut reader, &mut out, "\n› Enter prompt (or image path):\n");
                if p.is_none() && !RUNNING.load(Ordering::SeqCst) {
                    let _ = writeln!(out, "\n✓ Cancelled");
                    let _ = out.flush();
                    std::process::exit(0);
                }
                (Mode::AutoSend, p)
            }
            InitialChoice::AutoSend(Some(prompt)) => (Mode::AutoSend, Some(prompt)),
        }
    };

    let mut supervisor = Supervisor::new(terminal.clone(), harness, selected_mode);
    supervisor.target = Some(target.clone());
    let _ = supervisor
        .state_machine
        .transition(harness_queue::models::State::SelectingMode);
    let _ = supervisor
        .state_machine
        .transition(harness_queue::models::State::Watching);

    match selected_mode {
        Mode::AutoResume => {
            let _ = writeln!(out, "\n✓ Mode selected: Auto-resume");
            let _ = writeln!(out, "✓ Built-in continuation prompt loaded\n");
            let _ = writeln!(
                out,
                "● Watching {} session {}",
                target.harness.display_name(),
                target.short_session_id()
            );
            if let Some(ref usage) = usage_summary {
                let _ = writeln!(out, "  {}", usage);
            }
            let _ = writeln!(out);
            supervisor.enqueue_prompt(DEFAULT_CONTINUATION_PROMPT.to_string());
        }
        Mode::AutoSend => {
            let _ = writeln!(out, "\n✓ Mode selected: Auto-send");
            if let Some(prompt_text) = initial_prompt {
                let images = extract_image_references(&prompt_text);
                let desc = if !images.is_empty() {
                    let d = describe_attachments(&images);
                    let _ = writeln!(out, "✓ Image attached: {}", d);
                    Some(d)
                } else {
                    None
                };
                let formatted = format_prompt_with_images(&prompt_text, &images, &target.harness);
                let id = supervisor.enqueue_prompt_with_attachments(formatted, desc);
                let _ = writeln!(out, "✓ Prompt #{} queued\n", id);
            }
            let _ = writeln!(
                out,
                "● Watching {} session {}",
                target.harness.display_name(),
                target.short_session_id()
            );
            if let Some(ref usage) = usage_summary {
                let _ = writeln!(out, "  {}", usage);
            }
            let _ = writeln!(out);
        }
    }
    let _ = out.flush();
    drop(out);

    // Continuous queue loop: accept prompts and allow Shift+Left editing
    ATTACHED.store(true, Ordering::SeqCst);

    let _raw_guard = RawModeGuard;
    let _ = enable_raw_mode();

    let mut stdout = io::stdout();
    let mut input_buffer = String::new();
    let mut cursor_idx: usize = 0;
    let mut cli_editing_id: Option<usize> = None;
    let mut last_tick = std::time::Instant::now();

    render_cli_prompt(&mut stdout, None, &input_buffer, cursor_idx);

    while RUNNING.load(Ordering::SeqCst) {
        if event::poll(Duration::from_millis(50)).unwrap_or(false) {
            if let Ok(Event::Key(key)) = event::read() {
                if key.kind == KeyEventKind::Press {
                    if key.modifiers.contains(KeyModifiers::CONTROL)
                        && key.code == KeyCode::Char('c')
                    {
                        break;
                    }

                    // Shift + Left: Edit last / cycle previous queued prompt
                    if key.modifiers.contains(KeyModifiers::SHIFT) && key.code == KeyCode::Left {
                        let queued = supervisor.queue.queued_ids();
                        if !queued.is_empty() {
                            let next_id = match cli_editing_id {
                                None => queued.last().copied(),
                                Some(curr) => {
                                    if let Some(pos) = queued.iter().position(|&x| x == curr) {
                                        if pos > 0 {
                                            Some(queued[pos - 1])
                                        } else {
                                            queued.last().copied()
                                        }
                                    } else {
                                        queued.last().copied()
                                    }
                                }
                            };
                            if let Some(id) = next_id {
                                if let Some(item) = supervisor.queue.get(id) {
                                    cli_editing_id = Some(id);
                                    input_buffer = item.prompt.clone();
                                    cursor_idx = input_buffer.len();
                                    render_cli_prompt(
                                        &mut stdout,
                                        cli_editing_id,
                                        &input_buffer,
                                        cursor_idx,
                                    );
                                }
                            }
                        } else {
                            let _ = write!(stdout, "\r\x1b[K! No queued prompts to edit\r\n");
                            render_cli_prompt(
                                &mut stdout,
                                cli_editing_id,
                                &input_buffer,
                                cursor_idx,
                            );
                        }
                        continue;
                    }

                    match key.code {
                        KeyCode::Enter => {
                            let trimmed = input_buffer.trim().to_string();
                            if let Some(edit_id) = cli_editing_id.take() {
                                if !trimmed.is_empty() {
                                    let images = extract_image_references(&trimmed);
                                    let desc = if !images.is_empty() {
                                        Some(describe_attachments(&images))
                                    } else {
                                        None
                                    };
                                    let formatted = format_prompt_with_images(
                                        &trimmed,
                                        &images,
                                        &target.harness,
                                    );
                                    match supervisor.update_queued_prompt(edit_id, formatted, desc)
                                    {
                                        Ok(_) => {
                                            let _ = write!(
                                                stdout,
                                                "\r\x1b[K✓ Prompt #{} updated\r\n\r\n",
                                                edit_id
                                            );
                                        }
                                        Err(e) => {
                                            let _ = write!(
                                                stdout,
                                                "\r\x1b[K! Failed to update: {}\r\n\r\n",
                                                e
                                            );
                                        }
                                    }
                                }
                                input_buffer.clear();
                                cursor_idx = 0;
                                render_cli_prompt(&mut stdout, None, &input_buffer, cursor_idx);
                            } else if trimmed == "edit" || trimmed.starts_with("edit ") {
                                let target_id = if trimmed == "edit" {
                                    supervisor.queue.last_queued_id()
                                } else {
                                    trimmed[5..].trim().parse::<usize>().ok()
                                };
                                if let Some(id) = target_id {
                                    if let Some(item) = supervisor.queue.get(id) {
                                        if item.status == ItemStatus::Queued {
                                            cli_editing_id = Some(id);
                                            input_buffer = item.prompt.clone();
                                            cursor_idx = input_buffer.len();
                                            render_cli_prompt(
                                                &mut stdout,
                                                cli_editing_id,
                                                &input_buffer,
                                                cursor_idx,
                                            );
                                            continue;
                                        }
                                    }
                                }
                                input_buffer.clear();
                                cursor_idx = 0;
                                render_cli_prompt(&mut stdout, None, &input_buffer, cursor_idx);
                            } else if trimmed == "status" {
                                let state_str = format!("{:?}", supervisor.state_machine.current());
                                let usage_str = supervisor.get_target_usage_summary();
                                let _ = write!(stdout, "\r\x1b[KStatus: {}", state_str);
                                if let Some(ref u) = usage_str {
                                    let _ = write!(stdout, "  •  Usage: {}", u);
                                }
                                let _ = write!(stdout, "\r\n\r\n");
                                input_buffer.clear();
                                cursor_idx = 0;
                                render_cli_prompt(&mut stdout, None, &input_buffer, cursor_idx);
                            } else if trimmed == "queue" || trimmed == "list" {
                                let items = supervisor.queue.all_items();
                                let _ = write!(stdout, "\r\x1b[KPrompt Queue ({}):\r\n", items.len());
                                if items.is_empty() {
                                    let _ = write!(stdout, "  (No prompts in queue)\r\n");
                                } else {
                                    for it in items {
                                        let status_badge = match it.status {
                                            ItemStatus::Queued => "[⏳ QUEUED]",
                                            ItemStatus::Submitting => "[➤ SENDING]",
                                            ItemStatus::Acknowledged => "[✓ SENT]",
                                            ItemStatus::Uncertain => "[! UNCERTAIN]",
                                        };
                                        let attach = if let Some(ref a) = it.attachments_summary {
                                            format!(" ({})", a)
                                        } else {
                                            "".to_string()
                                        };
                                        let _ = write!(
                                            stdout,
                                            "  {} #{} {}{}\r\n",
                                            status_badge, it.id, it.prompt, attach
                                        );
                                    }
                                }
                                let _ = write!(stdout, "\r\n");
                                input_buffer.clear();
                                cursor_idx = 0;
                                render_cli_prompt(&mut stdout, None, &input_buffer, cursor_idx);
                            } else if !trimmed.is_empty() {
                                let images = extract_image_references(&trimmed);
                                let desc = if !images.is_empty() {
                                    Some(describe_attachments(&images))
                                } else {
                                    None
                                };
                                let formatted =
                                    format_prompt_with_images(&trimmed, &images, &target.harness);
                                let id = supervisor
                                    .enqueue_prompt_with_attachments(formatted, desc.clone());
                                if let Some(d) = desc {
                                    let _ = write!(stdout, "\r\x1b[K✓ Image attached: {}\r\n", d);
                                }
                                let _ = write!(stdout, "\r\x1b[K✓ Prompt #{} queued\r\n\r\n", id);
                                input_buffer.clear();
                                cursor_idx = 0;
                                render_cli_prompt(&mut stdout, None, &input_buffer, cursor_idx);
                            }
                        }
                        KeyCode::Esc => {
                            if cli_editing_id.is_some() {
                                cli_editing_id = None;
                                input_buffer.clear();
                                cursor_idx = 0;
                                let _ = write!(stdout, "\r\x1b[K(Edit cancelled)\r\n");
                                render_cli_prompt(&mut stdout, None, &input_buffer, cursor_idx);
                            } else {
                                break;
                            }
                        }
                        KeyCode::Char(c) => {
                            input_buffer.insert(cursor_idx, c);
                            cursor_idx += 1;
                            render_cli_prompt(
                                &mut stdout,
                                cli_editing_id,
                                &input_buffer,
                                cursor_idx,
                            );
                        }
                        KeyCode::Backspace => {
                            if cursor_idx > 0 {
                                cursor_idx -= 1;
                                input_buffer.remove(cursor_idx);
                                render_cli_prompt(
                                    &mut stdout,
                                    cli_editing_id,
                                    &input_buffer,
                                    cursor_idx,
                                );
                            }
                        }
                        KeyCode::Delete => {
                            if cursor_idx < input_buffer.len() {
                                input_buffer.remove(cursor_idx);
                                render_cli_prompt(
                                    &mut stdout,
                                    cli_editing_id,
                                    &input_buffer,
                                    cursor_idx,
                                );
                            }
                        }
                        KeyCode::Left => {
                            if cursor_idx > 0 {
                                cursor_idx -= 1;
                                render_cli_prompt(
                                    &mut stdout,
                                    cli_editing_id,
                                    &input_buffer,
                                    cursor_idx,
                                );
                            }
                        }
                        KeyCode::Right => {
                            if cursor_idx < input_buffer.len() {
                                cursor_idx += 1;
                                render_cli_prompt(
                                    &mut stdout,
                                    cli_editing_id,
                                    &input_buffer,
                                    cursor_idx,
                                );
                            }
                        }
                        KeyCode::Home => {
                            cursor_idx = 0;
                            render_cli_prompt(
                                &mut stdout,
                                cli_editing_id,
                                &input_buffer,
                                cursor_idx,
                            );
                        }
                        KeyCode::End => {
                            cursor_idx = input_buffer.len();
                            render_cli_prompt(
                                &mut stdout,
                                cli_editing_id,
                                &input_buffer,
                                cursor_idx,
                            );
                        }
                        _ => {}
                    }
                }
            }
        }

        // Supervisor tick every 500ms
        if last_tick.elapsed() >= Duration::from_millis(500) {
            last_tick = std::time::Instant::now();
            match supervisor.tick() {
                Ok(events) => {
                    if !events.is_empty() {
                        let _ = write!(stdout, "\r\x1b[K");
                        for ev in events {
                            let _ = write!(stdout, "{}\r\n", ev);
                        }
                        let _ = write!(stdout, "\r\n");
                        render_cli_prompt(&mut stdout, cli_editing_id, &input_buffer, cursor_idx);
                    }
                }
                Err(e) => {
                    let _ = write!(stdout, "\r\x1b[K! Supervisor halted: {}\r\n", e);
                    break;
                }
            }
        }
    }

    drop(_raw_guard);
    let _ = supervisor
        .state_machine
        .transition(harness_queue::models::State::Detached);
    println!("\n✓ Detached cleanly");
    std::process::exit(0);
}
