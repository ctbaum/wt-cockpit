//! Persistent workspace-level focus history for the quick project toggle.

use serde_json::Value;
use std::fs::{self, OpenOptions};
use std::hash::{Hash, Hasher};
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[derive(Default, Debug, PartialEq, Eq)]
struct RecentWorkspaces {
    updated_at: u128,
    current: Option<String>,
    previous: Option<String>,
}

impl RecentWorkspaces {
    fn parse(text: &str) -> Self {
        let mut lines = text.lines();
        Self {
            updated_at: lines.next().and_then(|line| line.parse().ok()).unwrap_or(0),
            current: non_empty(lines.next()),
            previous: non_empty(lines.next()),
        }
    }

    fn serialize(&self) -> String {
        format!(
            "{}\n{}\n{}\n",
            self.updated_at,
            self.current.as_deref().unwrap_or(""),
            self.previous.as_deref().unwrap_or("")
        )
    }

    fn record(&mut self, workspace: &str, observed_at: u128) {
        if observed_at < self.updated_at {
            return;
        }
        if self.current.as_deref() != Some(workspace) {
            self.previous = self.current.replace(workspace.to_string());
        }
        self.updated_at = observed_at;
    }

    fn toggle<F>(&mut self, current: &str, observed_at: u128, focus: F) -> Result<String, String>
    where
        F: FnOnce(&str) -> bool,
    {
        self.record(current, observed_at);
        let target = self
            .previous
            .clone()
            .ok_or_else(|| "no previous project recorded yet".to_string())?;
        if !focus(&target) {
            return Err(format!("previous project {target} is no longer available"));
        }
        self.current = Some(target.clone());
        self.previous = Some(current.to_string());
        self.updated_at = observed_at;
        Ok(target)
    }
}

fn non_empty(value: Option<&str>) -> Option<String> {
    value.filter(|value| !value.is_empty()).map(String::from)
}

fn observed_at() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos()
}

fn workspace_from_env() -> Option<String> {
    std::env::var("HERDR_WORKSPACE_ID")
        .ok()
        .filter(|id| !id.is_empty())
        .or_else(|| {
            let context: Value =
                serde_json::from_str(&std::env::var("HERDR_PLUGIN_CONTEXT_JSON").ok()?).ok()?;
            context["workspace_id"]
                .as_str()
                .filter(|id| !id.is_empty())
                .map(String::from)
        })
}

fn herdr_command() -> Command {
    Command::new(std::env::var_os("HERDR_BIN_PATH").unwrap_or_else(|| "herdr".into()))
}

fn plugin_config_dir() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("HERDR_DECK_STATE_DIR") {
        return Some(PathBuf::from(path));
    }
    let plugin = std::env::var("HERDR_PLUGIN_ID").unwrap_or_else(|_| "herdr-deck".into());
    let output = herdr_command()
        .args(["plugin", "config-dir", &plugin])
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| PathBuf::from(String::from_utf8_lossy(&output.stdout).trim().to_string()))
}

fn state_path() -> Result<PathBuf, String> {
    let dir =
        plugin_config_dir().ok_or_else(|| "cannot locate plugin config directory".to_string())?;
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    std::env::var_os("HERDR_SOCKET_PATH").hash(&mut hasher);
    Ok(dir.join(format!("recent-workspaces-{:016x}", hasher.finish())))
}

struct StateLock(PathBuf);

impl Drop for StateLock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

fn acquire_lock(path: &Path) -> Result<StateLock, String> {
    let lock = path.with_extension("lock");
    for _ in 0..100 {
        match OpenOptions::new().write(true).create_new(true).open(&lock) {
            Ok(_) => return Ok(StateLock(lock)),
            Err(error) if error.kind() == ErrorKind::AlreadyExists => {
                let stale = fs::metadata(&lock)
                    .and_then(|metadata| metadata.modified())
                    .and_then(|modified| modified.elapsed().map_err(std::io::Error::other))
                    .is_ok_and(|age| age > Duration::from_secs(5));
                if stale {
                    let _ = fs::remove_file(&lock);
                } else {
                    std::thread::sleep(Duration::from_millis(5));
                }
            }
            Err(error) => return Err(format!("cannot lock project history: {error}")),
        }
    }
    Err("timed out locking project history".into())
}

fn update_state<T>(
    update: impl FnOnce(&mut RecentWorkspaces) -> Result<T, String>,
) -> Result<T, String> {
    let path = state_path()?;
    let parent = path
        .parent()
        .ok_or_else(|| "invalid state path".to_string())?;
    fs::create_dir_all(parent)
        .map_err(|error| format!("cannot create state directory: {error}"))?;
    let _lock = acquire_lock(&path)?;
    let mut state = fs::read_to_string(&path)
        .map(|text| RecentWorkspaces::parse(&text))
        .unwrap_or_default();
    let result = update(&mut state);
    let temporary = path.with_extension(format!("tmp-{}", std::process::id()));
    fs::write(&temporary, state.serialize())
        .and_then(|()| fs::rename(&temporary, &path))
        .map_err(|error| format!("cannot save project history: {error}"))?;
    result
}

pub fn record_focus() -> Result<(), String> {
    let workspace =
        workspace_from_env().ok_or_else(|| "workspace context is missing".to_string())?;
    let observed_at = observed_at();
    update_state(|state| {
        state.record(&workspace, observed_at);
        Ok(())
    })
}

pub fn toggle_project() -> Result<(), String> {
    let current = workspace_from_env().ok_or_else(|| "workspace context is missing".to_string())?;
    let observed_at = observed_at();
    update_state(|state| {
        state.toggle(&current, observed_at, |target| {
            herdr_command()
                .args(["workspace", "focus", target])
                .output()
                .is_ok_and(|output| output.status.success())
        })?;
        Ok(())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_distinct_workspaces_and_toggles_between_them() {
        let mut state = RecentWorkspaces::default();
        state.record("one", 1);
        state.record("two", 2);
        assert_eq!(state.toggle("two", 3, |_| true).as_deref(), Ok("one"));
        assert_eq!(state.current.as_deref(), Some("one"));
        assert_eq!(state.previous.as_deref(), Some("two"));
        assert_eq!(state.toggle("one", 4, |_| true).as_deref(), Ok("two"));
    }

    #[test]
    fn ignores_focus_events_older_than_a_completed_toggle() {
        let mut state = RecentWorkspaces::default();
        state.record("one", 10);
        state.record("two", 20);
        state.toggle("two", 30, |_| true).unwrap();
        state.record("two", 25);
        assert_eq!(state.current.as_deref(), Some("one"));
        assert_eq!(state.previous.as_deref(), Some("two"));
    }

    #[test]
    fn failed_focus_keeps_the_previous_target() {
        let mut state = RecentWorkspaces::default();
        state.record("one", 1);
        state.record("two", 2);
        assert!(state.toggle("two", 3, |_| false).is_err());
        assert_eq!(state.current.as_deref(), Some("two"));
        assert_eq!(state.previous.as_deref(), Some("one"));
    }

    #[test]
    fn state_round_trips() {
        let state = RecentWorkspaces {
            updated_at: 42,
            current: Some("w2".into()),
            previous: Some("w1".into()),
        };
        assert_eq!(RecentWorkspaces::parse(&state.serialize()), state);
    }
}
