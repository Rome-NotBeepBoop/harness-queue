use crate::models::{AgentTarget, HarnessKind, Mode};
use crate::session_discovery::{discover_sessions_for_harness, format_session_label, DiscoveredSession};
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
    usage_summary: Option<&str>,
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
    let worktree_line = format!("Worktree: {}", target.cwd);

    let mut content_lines = vec![title.to_string(), host_line, target_detail, worktree_line];
    if let Some(usage) = usage_summary {
        content_lines.push(format!("Usage: {}", usage));
    }

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

pub fn is_cancel_input(s: &str) -> bool {
    let trimmed = s.trim();
    trimmed.contains('\x03') // Ctrl+C (ETX)
        || trimmed.contains('\x04') // Ctrl+D (EOT)
        || trimmed.eq_ignore_ascii_case("q")
        || trimmed.eq_ignore_ascii_case("exit")
        || trimmed.eq_ignore_ascii_case("cancel")
}

pub fn read_mode_selection<R: BufRead>(reader: &mut R) -> Mode {
    let mut line = String::new();
    if reader.read_line(&mut line).is_ok() {
        if is_cancel_input(&line) {
            return Mode::AutoResume;
        }
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
            if is_cancel_input(&line) {
                return InitialChoice::Cancelled;
            }
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

    let _ = writeln!(
        writer,
        "\n? Multiple agents detected in this terminal tab ({} found):",
        targets.len()
    );
    for (idx, target) in targets.iter().enumerate() {
        let worktree_info = if !target.cwd.is_empty() {
            format!(" · worktree: {}", target.cwd)
        } else {
            "".to_string()
        };
        let _ = writeln!(
            writer,
            "  {}) {} ({}){}",
            idx + 1,
            target.harness.display_name(),
            shorten_handle(&target.pane_id),
            worktree_info
        );
    }
    let _ = write!(writer, "› Select agent to target [1]: ");
    let _ = writer.flush();

    let mut line = String::new();
    match reader.read_line(&mut line) {
        Ok(0) => None, // EOF / Cancelled
        Ok(_) => {
            if is_cancel_input(&line) {
                return None;
            }
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
            if is_cancel_input(&first_line) {
                return InputEvent::Eof;
            }
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
                        if is_cancel_input(&next_line) {
                            return InputEvent::Eof;
                        }
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

pub fn format_fallback_notification() -> &'static str {
    "! No Herdr, Orca, or Ghostty terminal panes detected.\n› Falling back to Terminal Agent Selection...\n"
}

pub fn select_harness_menu<R: BufRead, W: Write>(
    reader: &mut R,
    writer: &mut W,
) -> Option<HarnessKind> {
    let _ = writeln!(writer, "\n? Select agent harness:");
    let _ = writeln!(writer, "  1) Google Antigravity (AGY)");
    let _ = writeln!(writer, "  2) OpenAI Codex");
    let _ = writeln!(writer, "  3) Claude Code");
    let _ = writeln!(writer, "  4) OpenCode");
    let _ = writeln!(writer, "  5) OMP");
    let _ = writeln!(writer, "  6) Pi");
    let _ = write!(writer, "› Choice [1]: ");
    let _ = writer.flush();

    let mut line = String::new();
    match reader.read_line(&mut line) {
        Ok(0) => None, // EOF / Cancelled
        Ok(_) => {
            if is_cancel_input(&line) {
                return None;
            }
            let trimmed = line.trim();
            match trimmed {
                "" | "1" => Some(HarnessKind::Agy),
                "2" => Some(HarnessKind::Codex),
                "3" => Some(HarnessKind::ClaudeCode),
                "4" => Some(HarnessKind::OpenCode),
                "5" => Some(HarnessKind::Omp),
                "6" => Some(HarnessKind::Pi),
                _ => Some(HarnessKind::Agy),
            }
        }
        Err(_) => None,
    }
}

pub fn select_session_menu<R: BufRead, W: Write>(
    reader: &mut R,
    writer: &mut W,
    harness: &HarnessKind,
    sessions: &[DiscoveredSession],
) -> Option<AgentTarget> {
    let _ = writeln!(writer, "\n? Select session for {}:", harness.display_name());

    let mut custom_idx = 1;
    if !sessions.is_empty() {
        for (idx, sess) in sessions.iter().enumerate() {
            let label = format_session_label(sess);
            let _ = writeln!(writer, "  {}) {}", idx + 1, label);
        }
        custom_idx = sessions.len() + 1;
    }

    let _ = writeln!(writer, "  {}) [Enter custom session ID or start new]", custom_idx);
    let _ = write!(writer, "› Select session [1]: ");
    let _ = writer.flush();

    let mut line = String::new();
    let choice_idx = match reader.read_line(&mut line) {
        Ok(0) => return None,
        Ok(_) => {
            if is_cancel_input(&line) {
                return None;
            }
            let trimmed = line.trim();
            if trimmed.is_empty() {
                1
            } else {
                match trimmed.parse::<usize>() {
                    Ok(n) => n,
                    Err(_) => 1,
                }
            }
        }
        Err(_) => return None,
    };

    let default_cwd = std::env::current_dir()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| ".".to_string());

    if choice_idx >= 1 && choice_idx <= sessions.len() {
        let chosen = &sessions[choice_idx - 1];
        let cwd = if !chosen.workspace_path.is_empty() && chosen.workspace_path != "." {
            chosen.workspace_path.clone()
        } else if chosen.workspace != "default" && !chosen.workspace.is_empty() {
            chosen.workspace.clone()
        } else {
            default_cwd
        };

        Some(AgentTarget {
            terminal_id: "standalone".to_string(),
            pane_id: "tab_agent".to_string(),
            pid: 0,
            harness: harness.clone(),
            session_id: chosen.session_id.clone(),
            cwd,
        })
    } else {
        // Custom session entry
        let _ = write!(writer, "› Enter session ID: ");
        let _ = writer.flush();
        let mut sess_line = String::new();
        let custom_id = match reader.read_line(&mut sess_line) {
            Ok(0) => return None,
            Ok(_) => {
                if is_cancel_input(&sess_line) {
                    return None;
                }
                let t = sess_line.trim().to_string();
                if t.is_empty() {
                    "new-session".to_string()
                } else {
                    t
                }
            }
            Err(_) => return None,
        };

        let _ = write!(writer, "› Enter worktree / folder path [{}]: ", default_cwd);
        let _ = writer.flush();
        let mut cwd_line = String::new();
        let custom_cwd = match reader.read_line(&mut cwd_line) {
            Ok(0) => return None,
            Ok(_) => {
                if is_cancel_input(&cwd_line) {
                    return None;
                }
                let t = cwd_line.trim().to_string();
                if t.is_empty() {
                    default_cwd
                } else {
                    t
                }
            }
            Err(_) => return None,
        };

        Some(AgentTarget {
            terminal_id: "standalone".to_string(),
            pane_id: "tab_agent".to_string(),
            pid: 0,
            harness: harness.clone(),
            session_id: custom_id,
            cwd: custom_cwd,
        })
    }
}

pub fn select_fallback_agent_target<R: BufRead, W: Write>(
    reader: &mut R,
    writer: &mut W,
) -> Option<AgentTarget> {
    let harness = select_harness_menu(reader, writer)?;
    let sessions = discover_sessions_for_harness(&harness);
    select_session_menu(reader, writer, &harness, &sessions)
}

