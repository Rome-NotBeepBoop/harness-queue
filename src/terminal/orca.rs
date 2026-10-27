use super::{AgentStatus, DeliveryAck, NeighborDetection, TerminalAdapter, TerminalError};
use crate::models::{AgentTarget, HarnessKind};
use crate::terminal::herdr::HerdrAdapter;
use serde_json::Value;
use std::process::Command;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrcaTerminalEntry {
    pub handle: String,
    pub tab_id: String,
    pub worktree_id: String,
    pub worktree_path: String,
    pub title: String,
    pub connected: bool,
    pub writable: bool,
    pub orphaned: bool,
    pub preview: String,
    pub agent_identity: Option<String>,
}

pub fn sanitize_json_surrogates(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = String::with_capacity(input.len());
    let mut i = 0;

    while i < bytes.len() {
        if i + 5 < bytes.len() && bytes[i] == b'\\' && bytes[i + 1] == b'u' {
            if let Ok(code) = u16::from_str_radix(&input[i + 2..i + 6], 16) {
                if (0xD800..=0xDBFF).contains(&code) {
                    // High surrogate: check if followed by valid low surrogate (\uDC00..\uDFFF)
                    let has_low =
                        if i + 11 < bytes.len() && bytes[i + 6] == b'\\' && bytes[i + 7] == b'u' {
                            u16::from_str_radix(&input[i + 8..i + 12], 16)
                                .map(|low| (0xDC00..=0xDFFF).contains(&low))
                                .unwrap_or(false)
                        } else {
                            false
                        };

                    if has_low {
                        // Valid surrogate pair
                        out.push_str(&input[i..i + 12]);
                        i += 12;
                        continue;
                    } else {
                        // Unpaired high surrogate! Replace with Unicode replacement char \uFFFD
                        out.push_str("\\uFFFD");
                        i += 6;
                        continue;
                    }
                } else if (0xDC00..=0xDFFF).contains(&code) {
                    // Unpaired low surrogate! Replace with Unicode replacement char \uFFFD
                    out.push_str("\\uFFFD");
                    i += 6;
                    continue;
                }
            }
        }
        out.push(bytes[i] as char);
        i += 1;
    }

    out
}

pub fn parse_orca_terminals_json(json_str: &str) -> Result<Vec<OrcaTerminalEntry>, TerminalError> {
    let clean_str = if let Some(idx) = json_str.find('{') {
        &json_str[idx..]
    } else {
        json_str
    };

    let sanitized = sanitize_json_surrogates(clean_str);
    let v: Value = serde_json::from_str(&sanitized)
        .map_err(|e| TerminalError::JsonParseError(e.to_string()))?;

    let terminals_arr = v
        .pointer("/result/terminals")
        .and_then(|t| t.as_array())
        .ok_or_else(|| {
            TerminalError::JsonParseError("Missing result.terminals array".to_string())
        })?;

    let mut entries = Vec::new();
    for item in terminals_arr {
        let handle = item
            .get("handle")
            .and_then(|h| h.as_str())
            .unwrap_or("")
            .to_string();
        let tab_id = item
            .get("tabId")
            .and_then(|t| t.as_str())
            .unwrap_or("")
            .to_string();
        let worktree_id = item
            .get("worktreeId")
            .and_then(|w| w.as_str())
            .unwrap_or("")
            .to_string();
        let worktree_path = item
            .get("worktreePath")
            .and_then(|p| p.as_str())
            .unwrap_or("")
            .to_string();
        let title = item
            .get("title")
            .and_then(|t| t.as_str())
            .unwrap_or("")
            .to_string();
        let connected = item
            .get("connected")
            .and_then(|c| c.as_bool())
            .unwrap_or(true);
        let writable = item
            .get("writable")
            .and_then(|w| w.as_bool())
            .unwrap_or(true);
        let orphaned = item
            .get("orphaned")
            .and_then(|o| o.as_bool())
            .unwrap_or(false);
        let preview = item
            .get("preview")
            .and_then(|p| p.as_str())
            .unwrap_or("")
            .to_string();
        let agent_identity = item
            .get("agentIdentity")
            .and_then(|a| a.as_str())
            .map(|s| s.to_string());

        if !handle.is_empty() {
            entries.push(OrcaTerminalEntry {
                handle,
                tab_id,
                worktree_id,
                worktree_path,
                title,
                connected,
                writable,
                orphaned,
                preview,
                agent_identity,
            });
        }
    }

    Ok(entries)
}

pub fn infer_orca_harness(entry: &OrcaTerminalEntry) -> HarnessKind {
    if let Some(ref id) = entry.agent_identity {
        match id.to_lowercase().as_str() {
            "codex" => return HarnessKind::Codex,
            "claude" => return HarnessKind::ClaudeCode,
            "opencode" => return HarnessKind::OpenCode,
            "omp" => return HarnessKind::Omp,
            "pi" => return HarnessKind::Pi,
            "antigravity" | "agy" => return HarnessKind::Agy,
            other => return HarnessKind::Unknown(other.to_string()),
        }
    }

    // Fall back to title and preview inspection
    let combined = format!("{} {}", entry.title, entry.preview);
    HerdrAdapter::infer_harness(&combined)
}

pub fn is_same_or_subfolder(path_a: &std::path::Path, path_b: &std::path::Path) -> bool {
    let canon_a = path_a
        .canonicalize()
        .unwrap_or_else(|_| path_a.to_path_buf());
    let canon_b = path_b
        .canonicalize()
        .unwrap_or_else(|_| path_b.to_path_buf());
    canon_a == canon_b || canon_a.starts_with(&canon_b) || canon_b.starts_with(&canon_a)
}

fn is_agent_title(title: &str) -> bool {
    let lower = title.to_lowercase();
    lower.contains("codex")
        || lower.contains("claude")
        || lower.contains("opencode")
        || lower.contains("omp")
        || lower.contains("pi")
        || lower.contains("antigravity")
        || lower.contains("agy")
}

pub fn find_orca_neighbor(
    caller_handle: &str,
    caller_tab: Option<&str>,
    caller_cwd: &std::path::Path,
    terminals: &[OrcaTerminalEntry],
) -> NeighborDetection {
    // 1. Identify the caller's tab
    let resolved_tab = caller_tab.map(|s| s.to_string()).or_else(|| {
        terminals
            .iter()
            .find(|t| t.handle == caller_handle)
            .map(|t| t.tab_id.clone())
    });

    let tab = match resolved_tab {
        Some(t) if !t.is_empty() => t,
        _ => {
            return NeighborDetection::NoneFound(
                "Caller terminal tab could not be identified".to_string(),
            );
        }
    };

    // 2. Filter strictly:
    //    - NOT the caller itself
    //    - connected and not orphaned
    //    - MUST be in the EXACT SAME tab/split-pane (t.tab_id == tab)
    //    - MUST be in the EXACT SAME folder (worktree_path matches caller_cwd)
    let candidates: Vec<&OrcaTerminalEntry> = terminals
        .iter()
        .filter(|t| {
            if t.handle == caller_handle || !t.connected || t.orphaned {
                return false;
            }
            if t.tab_id != tab {
                return false;
            }
            if t.worktree_path.is_empty() {
                return false;
            }
            is_same_or_subfolder(std::path::Path::new(&t.worktree_path), caller_cwd)
        })
        .collect();

    if candidates.is_empty() {
        return NeighborDetection::NoneFound(format!(
            "No adjacent agent found in the current split pane and folder ({}). Open a split pane next to your agent to attach.",
            caller_cwd.display()
        ));
    }

    if candidates.len() == 1 {
        let entry = candidates[0];
        return NeighborDetection::Exact(AgentTarget {
            terminal_id: entry.handle.clone(),
            pane_id: entry.handle.clone(),
            pid: 0,
            harness: infer_orca_harness(entry),
            session_id: entry.handle.clone(),
            cwd: entry.worktree_path.clone(),
        });
    }

    // If multiple split panes exist in the same tab and same folder:
    let agent_entries: Vec<&OrcaTerminalEntry> = candidates
        .iter()
        .filter(|t| t.agent_identity.is_some() || is_agent_title(&t.title))
        .copied()
        .collect();

    if agent_entries.len() == 1 {
        let entry = agent_entries[0];
        return NeighborDetection::Exact(AgentTarget {
            terminal_id: entry.handle.clone(),
            pane_id: entry.handle.clone(),
            pid: 0,
            harness: infer_orca_harness(entry),
            session_id: entry.handle.clone(),
            cwd: entry.worktree_path.clone(),
        });
    }

    let target_list = if !agent_entries.is_empty() {
        agent_entries
    } else {
        candidates
    };

    let targets = target_list
        .into_iter()
        .map(|e| AgentTarget {
            terminal_id: e.handle.clone(),
            pane_id: e.handle.clone(),
            pid: 0,
            harness: infer_orca_harness(e),
            session_id: e.handle.clone(),
            cwd: e.worktree_path.clone(),
        })
        .collect();

    NeighborDetection::Ambiguous(targets)
}

pub struct OrcaAdapter {
    pub orca_bin: String,
}

impl Default for OrcaAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl OrcaAdapter {
    pub fn new() -> Self {
        let bin = std::env::var("ORCA_CLI_COMMAND").unwrap_or_else(|_| {
            if std::path::Path::new("/opt/homebrew/bin/orca").exists() {
                "/opt/homebrew/bin/orca".to_string()
            } else if std::path::Path::new("/usr/local/bin/orca").exists() {
                "/usr/local/bin/orca".to_string()
            } else {
                "orca".to_string()
            }
        });
        Self { orca_bin: bin }
    }

    fn run_orca(&self, args: &[&str]) -> Result<String, TerminalError> {
        let output = Command::new(&self.orca_bin)
            .args(args)
            .output()
            .map_err(|e| TerminalError::CommandFailed(e.to_string()))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(TerminalError::CommandFailed(stderr.to_string()));
        }

        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }

    fn list_terminals(&self) -> Result<Vec<OrcaTerminalEntry>, TerminalError> {
        let out = self.run_orca(&["terminal", "list", "--json"])?;
        parse_orca_terminals_json(&out)
    }
}

impl TerminalAdapter for OrcaAdapter {
    fn terminal_name(&self) -> &'static str {
        "Orca IDE"
    }

    fn is_available(&self) -> bool {
        if let Ok(val) = std::env::var("TERM_PROGRAM") {
            if val == "Orca" {
                return true;
            }
        }
        if let Ok(val) = std::env::var("ORCA_TERMINAL_HANDLE") {
            if !val.trim().is_empty() {
                return true;
            }
        }

        Command::new(&self.orca_bin)
            .arg("status")
            .arg("--json")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    fn detect_current_pane(&self) -> Result<String, TerminalError> {
        if let Ok(handle) = std::env::var("ORCA_TERMINAL_HANDLE") {
            if !handle.is_empty() {
                return Ok(handle);
            }
        }

        // Fall back to listing terminals
        let terminals = self.list_terminals()?;
        if let Some(first) = terminals.first() {
            Ok(first.handle.clone())
        } else {
            Err(TerminalError::NotDetected)
        }
    }

    fn find_adjacent_agent(&self, current_pane: &str) -> Result<NeighborDetection, TerminalError> {
        let terminals = self.list_terminals()?;
        let caller_tab = std::env::var("ORCA_TAB_ID").ok();
        let caller_cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
        Ok(find_orca_neighbor(
            current_pane,
            caller_tab.as_deref(),
            &caller_cwd,
            &terminals,
        ))
    }

    fn get_agent_status(&self, pane_id: &str) -> Result<AgentStatus, TerminalError> {
        let terminals = match self.list_terminals() {
            Ok(t) => t,
            Err(_) => return Ok(AgentStatus::Unknown),
        };
        if let Some(target) = terminals.iter().find(|t| t.handle == pane_id) {
            let preview_lower = target.preview.to_lowercase();
            if preview_lower.contains("usage limit") || preview_lower.contains("rate limit") {
                return Ok(AgentStatus::RateLimited);
            }
            if preview_lower.contains("do you want to proceed")
                || preview_lower.contains("approve?")
            {
                return Ok(AgentStatus::BlockedApproval);
            }
            if target.connected && target.writable {
                return Ok(AgentStatus::Ready);
            }
        }
        Ok(AgentStatus::Unknown)
    }

    fn send_prompt(
        &self,
        target: &AgentTarget,
        prompt: &str,
    ) -> Result<DeliveryAck, TerminalError> {
        let out = self.run_orca(&[
            "terminal",
            "send",
            "--terminal",
            &target.terminal_id,
            "--text",
            prompt,
            "--enter",
            "--json",
        ])?;

        let sanitized = sanitize_json_surrogates(&out);
        let v: Value = serde_json::from_str(&sanitized)
            .map_err(|e| TerminalError::JsonParseError(e.to_string()))?;

        let ok = v.get("ok").and_then(|o| o.as_bool()).unwrap_or(true);
        if ok {
            Ok(DeliveryAck {
                acknowledged: true,
                details: format!("Delivered to Orca terminal {}", target.terminal_id),
            })
        } else {
            Err(TerminalError::CommandFailed(
                "Orca rejected terminal input".to_string(),
            ))
        }
    }

    fn verify_target_alive(&self, target: &AgentTarget) -> Result<bool, TerminalError> {
        let terminals = match self.list_terminals() {
            Ok(t) => t,
            Err(_) => return Ok(true), // Don't falsely halt on a transient listing error
        };
        Ok(terminals
            .iter()
            .any(|t| t.handle == target.terminal_id && t.connected))
    }
}
