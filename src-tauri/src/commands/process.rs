//! Process detection commands for Claude Code

use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

#[cfg(windows)]
use std::os::windows::process::CommandExt;

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x08000000;

#[derive(Debug, Clone)]
pub struct RunningClaudeProcess {
    pub pid: u32,
    pub command: String,
    pub is_background: bool,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ClaudeProcessInfo {
    pub count: usize,
    pub background_count: usize,
    pub can_switch: bool,
    pub pids: Vec<u32>,
}

#[tauri::command]
pub async fn check_claude_processes() -> Result<ClaudeProcessInfo, String> {
    let processes = collect_running_claude_processes().map_err(|e| e.to_string())?;
    let pids: Vec<u32> = processes
        .iter()
        .filter(|process| !process.is_background)
        .map(|process| process.pid)
        .collect();
    let background_count = processes
        .iter()
        .filter(|process| process.is_background)
        .count();

    Ok(ClaudeProcessInfo {
        count: pids.len(),
        background_count,
        can_switch: pids.is_empty() && background_count == 0,
        pids,
    })
}

pub fn collect_running_claude_processes() -> anyhow::Result<Vec<RunningClaudeProcess>> {
    #[cfg(unix)]
    {
        collect_running_claude_processes_unix()
    }

    #[cfg(windows)]
    {
        collect_running_claude_processes_windows()
    }
}

pub fn gracefully_stop_claude_processes(processes: &[RunningClaudeProcess]) -> anyhow::Result<()> {
    if processes.is_empty() {
        return Ok(());
    }

    #[cfg(unix)]
    {
        for process in processes {
            let _ = Command::new("kill")
                .args(["-TERM", &process.pid.to_string()])
                .output();
        }

        let start = Instant::now();
        while start.elapsed() < Duration::from_secs(5) {
            let any_running = processes.iter().any(|process| {
                Command::new("kill")
                    .args(["-0", &process.pid.to_string()])
                    .status()
                    .map(|status| status.success())
                    .unwrap_or(false)
            });

            if !any_running {
                return Ok(());
            }

            thread::sleep(Duration::from_millis(100));
        }
    }

    #[cfg(windows)]
    {
        for process in processes {
            let _ = Command::new("taskkill")
                .creation_flags(CREATE_NO_WINDOW)
                .args(["/PID", &process.pid.to_string()])
                .output();
        }
        thread::sleep(Duration::from_secs(2));
        return Ok(());
    }

    anyhow::bail!("Timed out waiting for Claude Code processes to close gracefully");
}

pub fn restart_claude_processes(processes: &[RunningClaudeProcess]) -> anyhow::Result<()> {
    #[cfg(unix)]
    {
        for process in processes {
            if process.command.trim().is_empty() {
                continue;
            }

            Command::new("sh")
                .arg("-c")
                .arg("nohup sh -lc \"$1\" >/dev/null 2>&1 &")
                .arg("sh")
                .arg(&process.command)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()?;
        }
    }

    #[cfg(windows)]
    {
        for process in processes {
            if process.command.trim().is_empty() {
                continue;
            }

            Command::new("cmd")
                .creation_flags(CREATE_NO_WINDOW)
                .args(["/C", "start", "", "/B", &process.command])
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()?;
        }
    }

    Ok(())
}

#[cfg(unix)]
fn collect_running_claude_processes_unix() -> anyhow::Result<Vec<RunningClaudeProcess>> {
    let mut processes = Vec::new();
    let output = Command::new("ps").args(["-eo", "pid=,command="]).output()?;
    let stdout = String::from_utf8_lossy(&output.stdout);

    for line in stdout.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        if let Some((pid_str, command)) = line.split_once(' ') {
            let command = command.trim();
            let executable = command.split_whitespace().next().unwrap_or("");
            let is_claude = executable == "claude" || executable.ends_with("/claude");
            let is_background = command.contains(".claude")
                || command.contains("claude-vscode")
                || command.contains(".vscode");
            let is_switcher = command.contains("claude-switcher")
                || command.contains("claude-switcher-gpt")
                || command.contains("Claude Switcher");

            if is_claude && !is_switcher {
                if let Ok(pid) = pid_str.trim().parse::<u32>() {
                    if pid != std::process::id()
                        && !processes.iter().any(|p: &RunningClaudeProcess| p.pid == pid)
                    {
                        processes.push(RunningClaudeProcess {
                            pid,
                            command: command.to_string(),
                            is_background,
                        });
                    }
                }
            }
        }
    }

    Ok(processes)
}

#[cfg(windows)]
fn collect_running_claude_processes_windows() -> anyhow::Result<Vec<RunningClaudeProcess>> {
    let mut processes = Vec::new();
    let output = Command::new("tasklist")
        .creation_flags(CREATE_NO_WINDOW)
        .args(["/FI", "IMAGENAME eq claude.exe", "/FO", "CSV", "/NH"])
        .output()?;
    let stdout = String::from_utf8_lossy(&output.stdout);

    for line in stdout.lines() {
        let parts: Vec<&str> = line.split(',').collect();
        if parts.len() > 1 {
            let name = parts[0].trim_matches('"').to_lowercase();
            if name == "claude.exe" {
                let pid_str = parts[1].trim_matches('"');
                if let Ok(pid) = pid_str.parse::<u32>() {
                    if pid != std::process::id() {
                        processes.push(RunningClaudeProcess {
                            pid,
                            command: String::from("claude"),
                            is_background: false,
                        });
                    }
                }
            }
        }
    }

    Ok(processes)
}
