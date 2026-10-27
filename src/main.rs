use harness_queue::cli::{
    format_boxed_header, format_mode_prompt, format_not_detected_guide, read_initial_input,
    read_prompt_input, select_ambiguous_target, InitialChoice,
};
use harness_queue::harness::create_harness_adapter;
use harness_queue::image::{
    describe_attachments, extract_image_references, format_prompt_with_images,
};
use harness_queue::models::{AgentTarget, Mode, DEFAULT_CONTINUATION_PROMPT};
use harness_queue::supervisor::Supervisor;
use harness_queue::terminal::ghostty::GhosttyAdapter;
use harness_queue::terminal::herdr::HerdrAdapter;
use harness_queue::terminal::orca::OrcaAdapter;
use harness_queue::terminal::{NeighborDetection, TerminalAdapter};
use harness_queue::tui::run_tui;
use std::env;
use std::io::{self, BufRead, BufReader, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

static RUNNING: AtomicBool = AtomicBool::new(true);
static ATTACHED: AtomicBool = AtomicBool::new(false);

extern "C" {
    fn signal(sig: i32, handler: extern "C" fn(i32)) -> usize;
    fn _exit(status: i32) -> !;
    fn write(fd: i32, buf: *const u8, count: usize) -> isize;
}

extern "C" fn sigint_handler(_: i32) {
    if !ATTACHED.load(Ordering::SeqCst) {
        unsafe {
            let msg = b"\n\n\xe2\x9c\x93 Cancelled\n";
            let _ = write(1, msg.as_ptr(), msg.len());
            _exit(0);
        }
    } else {
        RUNNING.store(false, Ordering::SeqCst);
    }
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
        signal(2, sigint_handler); // SIGINT (Ctrl+C)
    }

    let args: Vec<String> = env::args().collect();
    if args.iter().any(|a| a == "-h" || a == "--help") {
        println!("Harness Queue (hq) — Terminal-based coding agent queue\n");
        println!("USAGE:");
        println!("  hq [PROMPT]        Launch hq in default CLI mode");
        println!("  hq --cli [PROMPT]  Launch hq explicitly in CLI mode");
        println!("  hq --tui [PROMPT]  Launch hq in interactive TUI mode");
        println!("  hq --help          Show this help text\n");
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
            if is_tui {
                list[0].clone()
            } else {
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

    // Render clean boxed header
    let header = format_boxed_header(terminal.terminal_name(), &current_pane, &target);
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
        }
    }
    let _ = write!(out, "\n› Next prompt (or image path): ");
    let _ = out.flush();

    // Drop stdout lock so subsequent output doesn't contend
    drop(out);

    // Continuous queue loop: accept prompts infinitely
    ATTACHED.store(true, Ordering::SeqCst);

    let (tx, rx) = std::sync::mpsc::channel::<String>();
    let input_tx = tx.clone();

    // Background thread continuously accepts prompts from stdin
    thread::spawn(move || {
        let stdin = io::stdin();
        let mut r = BufReader::new(stdin.lock());

        while RUNNING.load(Ordering::SeqCst) {
            let mut line = String::new();
            match r.read_line(&mut line) {
                Ok(0) => break, // EOF
                Ok(_) => {
                    let trimmed = line.trim();
                    if !trimmed.is_empty() && input_tx.send(trimmed.to_string()).is_err() {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
    });

    // Supervisor event loop
    while RUNNING.load(Ordering::SeqCst) {
        let mut received_any = false;
        while let Ok(prompt) = rx.try_recv() {
            received_any = true;
            let images = extract_image_references(&prompt);
            let desc = if !images.is_empty() {
                let d = describe_attachments(&images);
                println!("✓ Image attached: {}", d);
                Some(d)
            } else {
                None
            };
            let formatted = format_prompt_with_images(&prompt, &images, &target.harness);
            let id = supervisor.enqueue_prompt_with_attachments(formatted, desc);
            println!("✓ Prompt #{} queued", id);
        }

        if received_any {
            print!("\n› Next prompt (or image path): ");
            io::stdout().flush().ok();
        }

        match supervisor.tick() {
            Ok(events) => {
                if !events.is_empty() {
                    for ev in events {
                        println!("{}", ev);
                    }
                    print!("\n› Next prompt (or image path): ");
                    io::stdout().flush().ok();
                }
            }
            Err(e) => {
                println!("\n! Supervisor halted: {}", e);
                break;
            }
        }

        thread::sleep(Duration::from_millis(500));
    }

    let _ = supervisor
        .state_machine
        .transition(harness_queue::models::State::Detached);
    println!("\n✓ Detached cleanly");
    std::process::exit(0);
}
