use harness_queue::cli::{
    format_agent_detection, format_mode_selection_menu, read_mode_selection, read_prompt_input,
};
use harness_queue::harness::create_harness_adapter;
use harness_queue::models::Mode;
use harness_queue::supervisor::Supervisor;
use harness_queue::terminal::ghostty::GhosttyAdapter;
use harness_queue::terminal::herdr::HerdrAdapter;
use harness_queue::terminal::TerminalAdapter;
use std::io::{self, BufReader, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

static RUNNING: AtomicBool = AtomicBool::new(true);

fn main() {
    let stdout = io::stdout();
    let mut out = stdout.lock();
    let stdin = io::stdin();
    let mut reader = BufReader::new(stdin.lock());

    // 1. Detect terminal adapter (Herdr first, then Ghostty)
    let terminal: Arc<dyn TerminalAdapter> = {
        let herdr = Arc::new(HerdrAdapter::new());
        if herdr.is_available() {
            herdr
        } else {
            let ghostty = Arc::new(GhosttyAdapter::new());
            if ghostty.is_available() {
                ghostty
            } else {
                eprintln!("Error: neither Herdr nor Ghostty environment was detected.");
                eprintln!("Please run hq inside a Herdr pane or Ghostty tab with an adjacent agent.");
                std::process::exit(1);
            }
        }
    };

    // 2. Discover adjacent agent
    let current_pane = match terminal.detect_current_pane() {
        Ok(cp) => cp,
        Err(e) => {
            eprintln!("Error detecting current pane: {}", e);
            std::process::exit(1);
        }
    };

    let target = match terminal.find_adjacent_agent(&current_pane) {
        Ok(harness_queue::terminal::NeighborDetection::Exact(t)) => t,
        Ok(harness_queue::terminal::NeighborDetection::Ambiguous(list)) => {
            eprintln!("Error: ambiguous adjacent agents found ({}).", list.len());
            for (idx, item) in list.iter().enumerate() {
                eprintln!(
                    "  {}) {} at pane {}",
                    idx + 1,
                    item.harness.display_name(),
                    item.pane_id
                );
            }
            std::process::exit(1);
        }
        Ok(harness_queue::terminal::NeighborDetection::NoneFound(msg)) => {
            eprintln!("Error: {}", msg);
            std::process::exit(1);
        }
        Err(e) => {
            eprintln!("Terminal error: {}", e);
            std::process::exit(1);
        }
    };

    // Print detection banner
    let detection_msg = format_agent_detection(terminal.terminal_name(), &target);
    let _ = writeln!(out, "{}", detection_msg);

    // 3. Print mode selection menu
    let _ = write!(out, "{}", format_mode_selection_menu());
    let _ = out.flush();

    let selected_mode = read_mode_selection(&mut reader);

    let harness: Arc<dyn harness_queue::harness::HarnessAdapter> =
        Arc::from(create_harness_adapter(&target.harness));
    let mut supervisor = Supervisor::new(terminal.clone(), harness.clone(), selected_mode);
    supervisor.target = Some(target.clone());
    let _ = supervisor
        .state_machine
        .transition(harness_queue::models::State::SelectingMode);
    let _ = supervisor
        .state_machine
        .transition(harness_queue::models::State::Watching);

    match selected_mode {
        Mode::AutoResume => {
            let _ = writeln!(out, "✓ Mode selected: Auto-resume");
            let _ = writeln!(out, "✓ Built-in continuation prompt loaded\n");
            let _ = writeln!(
                out,
                "● Watching {} session {}",
                target.harness.display_name(),
                target.short_session_id()
            );
            supervisor.enqueue_prompt(harness_queue::models::DEFAULT_CONTINUATION_PROMPT.to_string());
        }
        Mode::AutoSend => {
            let _ = writeln!(out, "✓ Mode selected: Auto-send\n");
            if let Some(initial_prompt) =
                read_prompt_input(&mut reader, &mut out, "› Enter prompt:\n")
            {
                let id = supervisor.enqueue_prompt(initial_prompt);
                let _ = writeln!(out, "✓ Prompt #{} queued", id);
            }
        }
    }
    let _ = out.flush();

    // Spawn background worker tick thread and prompt receiver channel
    let (tx, rx) = std::sync::mpsc::channel::<String>();

    // Input thread: continuously reads next prompts without exiting
    let input_tx = tx.clone();
    thread::spawn(move || {
        let stdin = io::stdin();
        let mut r = BufReader::new(stdin.lock());
        let mut out = io::stdout();

        loop {
            if let Some(p) = read_prompt_input(&mut r, &mut out, "\n› Next prompt:\n") {
                if input_tx.send(p).is_err() {
                    break;
                }
            } else {
                // Short sleep on empty input
                thread::sleep(Duration::from_millis(200));
            }
        }
    });

    // Main supervisor loop
    while RUNNING.load(Ordering::SeqCst) {
        // Collect any queued user inputs from channel
        while let Ok(prompt) = rx.try_recv() {
            let id = supervisor.enqueue_prompt(prompt);
            let _ = writeln!(out, "✓ Prompt #{} queued", id);
            let _ = out.flush();
        }

        // Run supervisor tick
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
}
