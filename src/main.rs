use harness_queue::cli::{
    format_boxed_header, format_mode_prompt, format_not_detected_guide, read_initial_input,
    read_prompt_event, read_prompt_input, select_ambiguous_target, InputEvent,
};
use harness_queue::harness::create_harness_adapter;
use harness_queue::models::{AgentTarget, Mode, DEFAULT_CONTINUATION_PROMPT};
use harness_queue::supervisor::Supervisor;
use harness_queue::terminal::herdr::HerdrAdapter;
use harness_queue::terminal::orca::OrcaAdapter;
use harness_queue::terminal::{NeighborDetection, TerminalAdapter};
use std::env;
use std::io::{self, BufReader, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

static RUNNING: AtomicBool = AtomicBool::new(true);

extern "C" {
    fn signal(sig: i32, handler: extern "C" fn(i32)) -> usize;
}

extern "C" fn sigint_handler(_: i32) {
    RUNNING.store(false, Ordering::SeqCst);
}

fn detect_terminal_environment() -> Option<Arc<dyn TerminalAdapter>> {
    let orca = Arc::new(OrcaAdapter::new());
    let herdr = Arc::new(HerdrAdapter::new());

    let in_orca_env = env::var("TERM_PROGRAM").map(|v| v == "Orca").unwrap_or(false)
        || env::var("ORCA_TERMINAL_HANDLE").map(|v| !v.trim().is_empty()).unwrap_or(false);

    let in_herdr_env = env::var("HERDR_PANE_ID").map(|v| !v.trim().is_empty()).unwrap_or(false)
        || env::var("HERDR_SOCKET").map(|v| !v.trim().is_empty()).unwrap_or(false)
        || env::var("HERDR_ENV").map(|v| v == "1").unwrap_or(false);

    if in_orca_env && orca.is_available() {
        return Some(orca);
    }

    if in_herdr_env && herdr.is_available() {
        return Some(herdr);
    }

    if orca.detect_current_pane().is_ok() {
        return Some(orca);
    }

    if herdr.detect_current_pane().is_ok() {
        return Some(herdr);
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
        println!("  hq [PROMPT]        Launch hq in an adjacent terminal pane");
        println!("  hq --help          Show this help text\n");
        println!("ENVIRONMENTS:");
        println!("  • Orca IDE         Native terminal API & split panes");
        println!("  • Herdr            Terminal pane manager & agent IPC\n");
        println!("SUPPORTED AGENTS:");
        println!("  Codex, Claude Code, OpenCode, OMP, Pi, Google Antigravity (AGY)");
        return;
    }

    let cli_initial_prompt: Option<String> = if args.len() > 1 {
        let joined = args[1..].join(" ");
        let trimmed = joined.trim().to_string();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed)
        }
    } else {
        None
    };

    let stdout = io::stdout();
    let mut out = stdout.lock();
    let stdin = io::stdin();
    let mut reader = BufReader::new(stdin.lock());

    // 1. Detect terminal adapter (Orca IDE or Herdr)
    let terminal = match detect_terminal_environment() {
        Some(t) => t,
        None => {
            let _ = write!(out, "{}", format_not_detected_guide());
            let _ = out.flush();
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
            match select_ambiguous_target(&mut reader, &mut out, &list) {
                Some(selected) => selected,
                None => {
                    eprintln!("No agent target selected.");
                    std::process::exit(1);
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

    // 3. Render clean boxed header
    let header = format_boxed_header(terminal.terminal_name(), &current_pane, &target);
    let _ = writeln!(out, "\n{}", header);
    let _ = out.flush();

    // 4. Mode and initial prompt determination
    let (selected_mode, initial_prompt) = if let Some(prompt) = cli_initial_prompt {
        (Mode::AutoSend, Some(prompt))
    } else {
        let _ = write!(out, "{}", format_mode_prompt());
        let _ = out.flush();

        let (mode, direct_prompt) = read_initial_input(&mut reader);
        if mode == Mode::AutoSend && direct_prompt.is_none() {
            let p = read_prompt_input(&mut reader, &mut out, "\n› Enter prompt:\n");
            (Mode::AutoSend, p)
        } else {
            (mode, direct_prompt)
        }
    };

    let harness = Arc::from(create_harness_adapter(&target.harness));
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
                let id = supervisor.enqueue_prompt(prompt_text);
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
    let _ = out.flush();

    // 5. Continuous queue loop
    let (tx, rx) = std::sync::mpsc::channel::<String>();
    let input_tx = tx.clone();

    // Background thread continuously accepts prompts
    thread::spawn(move || {
        let stdin = io::stdin();
        let mut r = BufReader::new(stdin.lock());
        let mut out = io::stdout();

        while RUNNING.load(Ordering::SeqCst) {
            match read_prompt_event(&mut r, &mut out, "\n› Next prompt:\n") {
                InputEvent::Text(p) => {
                    if input_tx.send(p).is_err() {
                        break;
                    }
                }
                InputEvent::Empty => {
                    thread::sleep(Duration::from_millis(200));
                }
                InputEvent::Eof => {
                    break;
                }
            }
        }
    });

    // Supervisor event loop
    while RUNNING.load(Ordering::SeqCst) {
        while let Ok(prompt) = rx.try_recv() {
            let id = supervisor.enqueue_prompt(prompt);
            let _ = writeln!(out, "✓ Prompt #{} queued", id);
            let _ = out.flush();
        }

        match supervisor.tick() {
            Ok(events) => {
                for ev in events {
                    let _ = writeln!(out, "{}", ev);
                }
                let _ = out.flush();
            }
            Err(e) => {
                let _ = writeln!(out, "\n! Supervisor halted: {}", e);
                let _ = out.flush();
                break;
            }
        }

        thread::sleep(Duration::from_millis(500));
    }

    let _ = supervisor
        .state_machine
        .transition(harness_queue::models::State::Detached);
    let _ = writeln!(out, "\n✓ Detached cleanly");
    let _ = out.flush();
}
