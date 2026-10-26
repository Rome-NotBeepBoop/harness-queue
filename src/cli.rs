use crate::models::{AgentTarget, Mode};
use std::io::{BufRead, Write};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InputEvent {
    Text(String),
    Empty,
    Eof,
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

            // Read subsequent lines if available immediately (e.g. pasted block)
            // or if the line ended with an explicit continuation backslash
            loop {
                let ends_with_slash = full_prompt.ends_with('\\');
                if ends_with_slash {
                    full_prompt.pop(); // remove trailing backslash
                }

                // Check if more input is available
                let mut next_line = String::new();
                match reader.read_line(&mut next_line) {
                    Ok(0) => break,
                    Ok(_) => {
                        let next_trimmed = next_line.trim_end_matches(&['\r', '\n'][..]);
                        if next_trimmed.is_empty() {
                            break;
                        }
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
