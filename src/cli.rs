use crate::models::{AgentTarget, Mode};
use std::io::{BufRead, Write};

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

pub fn read_prompt_input<R: BufRead, W: Write>(
    reader: &mut R,
    writer: &mut W,
    prompt_label: &str,
) -> Option<String> {
    let _ = write!(writer, "{}", prompt_label);
    let _ = writer.flush();

    let mut line = String::new();
    match reader.read_line(&mut line) {
        Ok(0) => None, // EOF
        Ok(_) => {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        }
        Err(_) => None,
    }
}
