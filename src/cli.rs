use crate::models::{AgentTarget, Mode};
use std::io::{BufRead, Write};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InputEvent {
    Text(String),
    Empty,
    Eof,
}

pub fn shorten_handle(handle: &str) -> &str {
    if handle.starts_with("term_") {
        if let Some(pos) = handle.find('-') {
            return &handle[..pos];
        }
    }
    handle
}

pub fn format_boxed_header(
    terminal_name: &str,
    current_pane: &str,
    target: &AgentTarget,
) -> String {
    let title = "Harness Queue (hq) v0.1";
    let host_line = format!(
        "Host: {} · Location: {}",
        terminal_name,
        shorten_handle(current_pane)
    );
    let target_detail = if target.session_id.is_empty() || target.session_id == target.pane_id {
        format!(
            "Target: {} ({})",
            target.harness.display_name(),
            shorten_handle(&target.pane_id)
        )
    } else {
        format!(
            "Target: {} ({} · session {})",
            target.harness.display_name(),
            shorten_handle(&target.pane_id),
            target.short_session_id()
        )
    };

    let content_lines = [title, &host_line, &target_detail];
    let max_len = content_lines
        .iter()
        .map(|l| l.chars().count())
        .max()
        .unwrap_or(40);
    let inner_width = max_len.max(54) + 4;

    let mut out = String::new();
    out.push('┌');
    out.push_str(&"─".repeat(inner_width));
    out.push_str("┐\n");

    for line in &content_lines {
        let line_len = line.chars().count();
        let padding = inner_width.saturating_sub(line_len + 2);
        out.push_str("│  ");
        out.push_str(line);
        out.push_str(&" ".repeat(padding));
        out.push_str("│\n");
    }

    out.push('└');
    out.push_str(&"─".repeat(inner_width));
    out.push('┘');

    out
}

pub fn format_not_detected_guide() -> String {
    r#"┌────────────────────────────────────────────────────────┐
│  Harness Queue (hq) — Terminal Not Detected            │
└────────────────────────────────────────────────────────┘

hq operates alongside coding agents in a split terminal pane.

To use hq:
  • Orca IDE:
    Open a terminal split pane (⌘D) next to your agent in the same
    folder and run `just` (or `hq`).

  • Herdr:
    Open an adjacent pane next to your agent in the same folder
    and run `just` (or `hq`).

  • Ghostty:
    Open a split pane (⌘D) next to your agent in the same folder
    and run `just` (or `hq`).

Supported agents:
  Codex, Claude Code, OpenCode, OMP, Pi, Google Antigravity (AGY)
"#
    .to_string()
}

pub fn format_agent_detection(terminal_name: &str, target: &AgentTarget) -> String {
    format!(
        "✓ {} detected\n✓ Adjacent agent: {}\n  Session {} · pane {}",
        terminal_name,
        target.harness.display_name(),
        target.short_session_id(),
        target.pane_id
    )
}

pub fn format_mode_selection_menu() -> String {
    r#"
Harness Queue v0.1

? Select mode:

  1) Auto-resume
     Resume with the built-in continuation prompt.

  2) Auto-send
     Queue your own prompts for automatic delivery.

› Select [1]: "#
        .to_string()
}

pub fn format_mode_prompt() -> String {
    r#"
? Select mode:
  1) Auto-resume  (default — press [Enter])
     Resume with the built-in continuation prompt.

  2) Auto-send
     Queue your own prompts for automatic delivery.

› Choice [1] or enter prompt directly: "#
        .to_string()
}

pub fn read_mode_selection<R: BufRead>(reader: &mut R) -> Mode {
    let mut line = String::new();
    if reader.read_line(&mut line).is_ok() {
        let trimmed = line.trim();
        if trimmed == "2" {
            return Mode::AutoSend;
        }
    }
    Mode::AutoResume
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InitialChoice {
    AutoResume,
    AutoSend(Option<String>),
    Cancelled,
}

pub fn read_initial_input<R: BufRead>(reader: &mut R) -> InitialChoice {
    let mut line = String::new();
    match reader.read_line(&mut line) {
        Ok(0) => InitialChoice::Cancelled, // EOF (Ctrl+D)
        Ok(_) => {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed == "1" {
                InitialChoice::AutoResume
            } else if trimmed == "2" {
                InitialChoice::AutoSend(None)
            } else {
                InitialChoice::AutoSend(Some(trimmed.to_string()))
            }
        }
        Err(_) => InitialChoice::Cancelled,
    }
}

pub fn select_ambiguous_target<R: BufRead, W: Write>(
    reader: &mut R,
    writer: &mut W,
    targets: &[AgentTarget],
) -> Option<AgentTarget> {
    if targets.is_empty() {
        return None;
    }
    if targets.len() == 1 {
        return Some(targets[0].clone());
    }

    let _ = writeln!(writer, "\n? Multiple adjacent agents detected:");
    for (idx, target) in targets.iter().enumerate() {
        let _ = writeln!(
            writer,
            "  {}) {} ({})",
            idx + 1,
            target.harness.display_name(),
            shorten_handle(&target.pane_id)
        );
    }
    let _ = write!(writer, "› Select [1]: ");
    let _ = writer.flush();

    let mut line = String::new();
    match reader.read_line(&mut line) {
        Ok(0) => None, // EOF / Cancelled
        Ok(_) => {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                return Some(targets[0].clone());
            }
            if let Ok(num) = trimmed.parse::<usize>() {
                if num >= 1 && num <= targets.len() {
                    return Some(targets[num - 1].clone());
                }
            }
            Some(targets[0].clone())
        }
        Err(_) => None,
    }
}

pub fn read_prompt_event<R: BufRead, W: Write>(
    reader: &mut R,
    writer: &mut W,
    prompt_label: &str,
) -> InputEvent {
    let _ = write!(writer, "{}", prompt_label);
    let _ = writer.flush();

    let mut first_line = String::new();
    match reader.read_line(&mut first_line) {
        Ok(0) => InputEvent::Eof,
        Ok(_) => {
            let trimmed = first_line.trim_end_matches(&['\r', '\n'][..]);
            if trimmed.is_empty() {
                return InputEvent::Empty;
            }

            let mut full_prompt = trimmed.to_string();

            // Read subsequent lines if user ended with continuation backslash '\'
            while full_prompt.ends_with('\\') {
                full_prompt.pop(); // remove trailing backslash
                let mut next_line = String::new();
                match reader.read_line(&mut next_line) {
                    Ok(0) => break,
                    Ok(_) => {
                        let next_trimmed = next_line.trim_end_matches(&['\r', '\n'][..]);
                        full_prompt.push('\n');
                        full_prompt.push_str(next_trimmed);
                    }
                    Err(_) => break,
                }
            }

            InputEvent::Text(full_prompt)
        }
        Err(_) => InputEvent::Eof,
    }
}

pub fn read_prompt_input<R: BufRead, W: Write>(
    reader: &mut R,
    writer: &mut W,
    prompt_label: &str,
) -> Option<String> {
    match read_prompt_event(reader, writer, prompt_label) {
        InputEvent::Text(t) => Some(t),
        InputEvent::Empty | InputEvent::Eof => None,
    }
}
