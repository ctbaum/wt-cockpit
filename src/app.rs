//! App state + key handling. One Mode enum drives which screen/modal is
//! active; all actions shell out through ext.rs.

use crate::ext;
use crate::sessions::{Agent as SessionAgent, Session};
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::text::Text;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::mpsc;

#[derive(Clone)]
pub enum EntryKind {
    Workspace { id: String, status: String },
    Remote(String), // ssh alias from $HERDR_DECK_REMOTES; opens herdr --remote
    Worktree(PathBuf),
    Cleanable { path: PathBuf, clean: bool },
    Dir(PathBuf),
    Session(Session),
}

#[derive(Clone)]
pub struct Entry {
    pub label: String, // workspace label or ~-collapsed path
    pub kind: EntryKind,
}

impl Entry {
    pub fn cache_key(&self) -> String {
        match &self.kind {
            EntryKind::Workspace { id, .. } => format!("w:{id}"),
            EntryKind::Session(s) => format!("s:{}:{}", s.agent.id(), s.id),
            EntryKind::Cleanable { path, .. } => format!("c:{}", path.to_string_lossy()),
            _ => format!("d:{}", self.label),
        }
    }

    fn matches(&self, filter: &str) -> bool {
        match_indices(&self.label, filter).is_some()
            || match &self.kind {
                EntryKind::Session(s) => {
                    match_indices(s.agent.id(), filter).is_some()
                        || match_indices(&s.cwd.to_string_lossy(), filter).is_some()
                }
                EntryKind::Cleanable { path, .. } => {
                    match_indices(&path.to_string_lossy(), filter).is_some()
                }
                _ => false,
            }
    }
}

#[derive(Clone, Copy, Eq, PartialEq)]
pub enum Source {
    Projects,
    Sessions,
    Cleanup,
}

pub struct LaunchForm {
    pub dir: PathBuf,
    pub agent: usize, // index into App.agents; == agents.len() means "none"
    pub branch: String,
    pub candidates: Vec<ext::WorktreeCandidate>,
    /// Candidates load on a worker thread (`wt list` can take minutes on a
    /// pathological repo); the form opens immediately and fills in later.
    pub candidates_loading: bool,
    /// Index into the filtered candidate list, not `candidates` itself.
    pub candidate_selected: Option<usize>,
    pub dangerous: bool,
    pub field: usize, // 0 agent, 1 branch, 2 dangerous
}

impl LaunchForm {
    fn new(dir: PathBuf, agent: usize) -> Self {
        Self {
            dir,
            agent,
            branch: String::new(),
            candidates: vec![],
            candidates_loading: true,
            candidate_selected: None,
            dangerous: true,
            field: 0,
        }
    }

    pub fn matching_candidates(&self) -> Vec<usize> {
        self.candidates
            .iter()
            .enumerate()
            .filter(|(_, candidate)| match_indices(&candidate.branch, self.branch.trim()).is_some())
            .map(|(index, _)| index)
            .collect()
    }

    fn move_candidate(&mut self, delta: isize) {
        let len = self.matching_candidates().len();
        if len == 0 {
            self.candidate_selected = None;
            return;
        }
        self.candidate_selected = Some(match (self.candidate_selected, delta.is_negative()) {
            (Some(0), true) | (None, true) => len - 1,
            (Some(index), true) => index - 1,
            (Some(index), false) => (index + 1) % len,
            (None, false) => 0,
        });
    }

    /// The checkout name Enter would hand to worktrunk as a new worktree:
    /// non-empty free-form input with no exact worktree match, no highlighted
    /// candidate, and not a worktrunk shortcut. Shown in the form so a typo
    /// is visible before it silently becomes a branch.
    pub fn pending_create(&self) -> Option<&str> {
        let typed = self.branch.trim();
        if typed.is_empty()
            || self.candidates_loading // an exact match may still be on its way
            || self.candidate_selected.is_some()
            || ext::worktrunk_is_shortcut(typed)
            || self.candidates.iter().any(|c| c.branch == typed)
        {
            return None;
        }
        Some(typed)
    }

    fn accept_candidate(&mut self) -> bool {
        let Some(selected) = self.candidate_selected else {
            return false;
        };
        let Some(candidate) = self
            .matching_candidates()
            .get(selected)
            .and_then(|index| self.candidates.get(*index))
        else {
            self.candidate_selected = None;
            return false;
        };
        self.branch.clone_from(&candidate.branch);
        self.candidate_selected = None;
        true
    }
}

pub enum DelAction {
    CloseWs(String),
    RemoveMerged(PathBuf),
    OfferForce(PathBuf, String), // path, branch — 'f' arms the force stage
    ForceArmed(PathBuf, String),
    RemoveAll(Vec<PathBuf>),
}

pub enum Mode {
    List,
    Launch(LaunchForm),
    NewPath { input: String },
    ConfirmDelete { msg: String, action: DelAction },
    Help,
}

/// Footer status line. Errors render red, everything else yellow.
pub struct Status {
    pub msg: String,
    pub error: bool,
}

impl Status {
    pub fn info(msg: impl Into<String>) -> Self {
        Self {
            msg: msg.into(),
            error: false,
        }
    }

    pub fn err(msg: impl Into<String>) -> Self {
        Self {
            msg: msg.into(),
            error: true,
        }
    }
}

/// A blocking operation queued by a key handler and executed by the event
/// loop right after the next draw, so the status message set alongside it is
/// on screen while subprocesses (worktrunk hooks, session scans) run.
pub enum Pending {
    Reload,
    Launch {
        dir: PathBuf,
        agent: Option<String>,
        branch: String,
        dangerous: bool,
    },
    Resume(Session),
    /// ^d on a worktree: `wt list` decides merged vs force-confirm, and can
    /// be slow, so it runs as a pending op instead of in the key handler.
    DeleteWorktree {
        path: PathBuf,
        label: String,
    },
    Delete(DelAction),
}

pub struct App {
    pub entries: Vec<Entry>,
    pub filter: String,
    pub filtered: Vec<usize>,
    pub selected: usize, // index into filtered
    pub mode: Mode,
    pub preview: HashMap<String, Text<'static>>,
    pub agents: Vec<String>, // detected agent binaries; launch form adds "none"
    pub source: Source,
    pub session_agent: Option<SessionAgent>,
    pub status: Option<Status>,
    pub pending: Option<Pending>,
    pub quit: bool,
    // Previews run subprocesses (herdr/wt/eza) on a worker thread so a slow
    // or hung command can never freeze the UI; results drain each tick.
    preview_tx: mpsc::Sender<(String, Entry, u16, u16)>,
    preview_rx: mpsc::Receiver<(String, Text<'static>)>,
    requested: HashSet<String>,
    // Launch-form checkout candidates load the same way (wt list can be slow).
    candidates_rx: Option<mpsc::Receiver<(PathBuf, Vec<ext::WorktreeCandidate>)>>,
}

/// Case-insensitive subsequence match; tier order is preserved among matches
/// (mirrors the tv channel's no_sort), so no scoring needed. Returns the char
/// indices of the matched chars so the UI can highlight them (tv's match_fg).
pub fn match_indices(hay: &str, needle: &str) -> Option<Vec<usize>> {
    let mut idx = Vec::new();
    let mut ni = needle.chars().map(|c| c.to_ascii_lowercase());
    let mut want = ni.next();
    for (i, c) in hay.chars().enumerate() {
        match want {
            None => break,
            Some(w) if c.to_ascii_lowercase() == w => {
                idx.push(i);
                want = ni.next();
            }
            _ => {}
        }
    }
    want.is_none().then_some(idx)
}

impl App {
    pub fn new() -> Self {
        let own_pane = ext::json(&["herdr", "pane", "current"])
            .and_then(|v| v["result"]["pane"]["pane_id"].as_str().map(String::from))
            .unwrap_or_default();
        let (preview_tx, req_rx) = mpsc::channel::<(String, Entry, u16, u16)>();
        let (res_tx, preview_rx) = mpsc::channel();
        std::thread::spawn(move || {
            while let Ok((key, entry, w, h)) = req_rx.recv() {
                let text = crate::preview::compute(&entry, w, h, &own_pane);
                if res_tx.send((key, text)).is_err() {
                    break;
                }
            }
        });
        let mut app = App {
            entries: vec![],
            filter: String::new(),
            filtered: vec![],
            selected: 0,
            mode: Mode::List,
            preview: HashMap::new(),
            agents: ext::detected_agents(),
            source: Source::Projects,
            session_agent: None,
            status: None,
            pending: None,
            quit: false,
            preview_tx,
            preview_rx,
            requested: HashSet::new(),
            candidates_rx: None,
        };
        app.reload();
        // Every ext call swallows subprocess errors, so a missing tool would
        // otherwise just look like an inexplicably empty list.
        let missing: Vec<&str> = ["wt", "zoxide", "fd"]
            .into_iter()
            .filter(|bin| !ext::command_exists(bin))
            .collect();
        if !missing.is_empty() {
            app.status = Some(Status::err(format!(
                "not found: {} · related features disabled",
                missing.join(", ")
            )));
        }
        app
    }

    pub fn reload(&mut self) {
        self.entries.clear();
        match self.source {
            Source::Projects => {
                for w in ext::workspaces() {
                    self.entries.push(Entry {
                        label: w.label,
                        kind: EntryKind::Workspace {
                            id: w.id,
                            status: w.status,
                        },
                    });
                }
                for host in ext::remotes() {
                    self.entries.push(Entry {
                        label: host.clone(),
                        kind: EntryKind::Remote(host),
                    });
                }
                for d in ext::dirs() {
                    let label = ext::collapse_tilde(&d.path.to_string_lossy());
                    let kind = if d.kind == ext::DirKind::Worktree {
                        EntryKind::Worktree(d.path)
                    } else {
                        EntryKind::Dir(d.path)
                    };
                    self.entries.push(Entry { label, kind });
                }
            }
            Source::Sessions => {
                self.entries
                    .extend(crate::sessions::list().into_iter().map(|session| Entry {
                        label: session.title.clone(),
                        kind: EntryKind::Session(session),
                    }));
            }
            Source::Cleanup => {
                self.entries
                    .extend(
                        ext::integrated_worktrees()
                            .into_iter()
                            .map(|worktree| Entry {
                                label: format!("{}/{}", worktree.project, worktree.branch),
                                kind: EntryKind::Cleanable {
                                    path: worktree.path,
                                    clean: worktree.clean,
                                },
                            }),
                    );
            }
        }
        self.invalidate_previews();
        self.apply_filter();
    }

    pub fn invalidate_previews(&mut self) {
        self.preview.clear();
        self.requested.clear();
    }

    pub fn apply_filter(&mut self) {
        self.filtered = (0..self.entries.len())
            .filter(|&i| {
                let entry = &self.entries[i];
                let agent_matches = match (&entry.kind, self.session_agent) {
                    (EntryKind::Session(s), Some(agent)) => s.agent == agent,
                    _ => true,
                };
                agent_matches && entry.matches(&self.filter)
            })
            .collect();
        self.selected = self.selected.min(self.filtered.len().saturating_sub(1));
    }

    pub fn selected_entry(&self) -> Option<&Entry> {
        self.filtered.get(self.selected).map(|&i| &self.entries[i])
    }

    /// Default launch-form agent: claude when installed, else the first
    /// detected agent (or "none" when nothing is detected).
    fn default_agent(&self) -> usize {
        self.agents.iter().position(|a| a == "claude").unwrap_or(0)
    }

    /// Queue the selected entry's preview for the worker thread. Called from
    /// the event loop only when input is idle (debounce for free).
    pub fn request_preview(&mut self, w: u16, h: u16) {
        let Some(entry) = self.selected_entry() else {
            return;
        };
        let key = entry.cache_key();
        if self.preview.contains_key(&key) || self.requested.contains(&key) {
            return;
        }
        let entry = entry.clone();
        self.requested.insert(key.clone());
        let _ = self.preview_tx.send((key, entry, w, h));
    }

    /// Pull any finished previews into the cache (non-blocking).
    pub fn drain_previews(&mut self) {
        while let Ok((key, text)) = self.preview_rx.try_recv() {
            self.preview.insert(key, text);
        }
    }

    /// Open the launch form immediately and fetch its checkout candidates on
    /// a one-off worker thread; `drain_candidates` fills them in.
    fn open_launch_form(&mut self, dir: PathBuf) {
        let (tx, rx) = mpsc::channel();
        self.candidates_rx = Some(rx);
        let d = dir.clone();
        std::thread::spawn(move || {
            let _ = tx.send((d.clone(), ext::worktree_candidates(&d)));
        });
        self.mode = Mode::Launch(LaunchForm::new(dir, self.default_agent()));
    }

    /// Deliver finished candidate loads to the launch form (non-blocking).
    /// Results for a form that was closed or reopened on another dir are
    /// dropped.
    pub fn drain_candidates(&mut self) {
        let Some(rx) = &self.candidates_rx else {
            return;
        };
        while let Ok((dir, candidates)) = rx.try_recv() {
            if let Mode::Launch(form) = &mut self.mode
                && form.dir == dir
                && form.candidates_loading
            {
                form.candidates = candidates;
                form.candidates_loading = false;
            }
        }
    }

    /// Execute the queued blocking operation (see [`Pending`]).
    pub fn run_pending(&mut self) {
        let Some(pending) = self.pending.take() else {
            return;
        };
        match pending {
            Pending::Reload => {
                self.reload();
                self.status = None;
            }
            Pending::Launch {
                dir,
                agent,
                branch,
                dangerous,
            } => match ext::launch_deck(&dir, agent.as_deref(), branch.trim(), dangerous) {
                Ok(()) => self.quit = true,
                Err(e) => {
                    self.status = Some(Status::err(e));
                    self.mode = Mode::List;
                }
            },
            Pending::Resume(session) => match ext::launch_session_deck(&session) {
                Ok(()) => self.quit = true,
                Err(e) => self.status = Some(Status::err(e)),
            },
            Pending::DeleteWorktree { path, label } => {
                let Some(info) = ext::wt_info(&path) else {
                    self.status =
                        Some(Status::info(format!("{label} is not a worktrunk worktree")));
                    return;
                };
                self.status = None;
                let branch = if info.branch.is_empty() {
                    label
                } else {
                    info.branch.clone()
                };
                self.mode = if info.integrated() {
                    Mode::ConfirmDelete {
                        msg: format!("remove merged worktree {branch}?"),
                        action: DelAction::RemoveMerged(path),
                    }
                } else {
                    Mode::ConfirmDelete {
                        msg: format!("{branch} is not merged (state: {}).", info.main_state),
                        action: DelAction::OfferForce(path, branch),
                    }
                };
            }
            Pending::Delete(action) => {
                let status = match action {
                    DelAction::CloseWs(id) => {
                        ext::close_workspace(&id);
                        Status::info("workspace closed")
                    }
                    DelAction::RemoveMerged(p) => {
                        if ext::wt_remove(&p, false) {
                            Status::info("removed merged worktree")
                        } else {
                            Status::err("wt remove failed (dirty tree?)")
                        }
                    }
                    DelAction::ForceArmed(p, branch) => {
                        if ext::wt_remove(&p, true) {
                            Status::info(format!("force-removed worktree {branch}"))
                        } else {
                            Status::err("wt remove -f -D failed")
                        }
                    }
                    DelAction::RemoveAll(paths) => {
                        let r = ext::wt_remove_clean(&paths);
                        let msg = format!(
                            "removed {} · skipped {} · failed {}",
                            r.removed, r.skipped, r.failed
                        );
                        if r.failed > 0 {
                            Status::err(msg)
                        } else {
                            Status::info(msg)
                        }
                    }
                    // Never queued: 'f' re-arms the modal instead.
                    DelAction::OfferForce(..) => return,
                };
                self.reload();
                self.status = Some(status);
            }
        }
    }

    pub fn handle_key(&mut self, key: KeyEvent) {
        self.status = None;
        if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
            self.quit = true;
            return;
        }
        match self.mode {
            Mode::List => self.key_list(key),
            Mode::Launch(_) => self.key_launch(key),
            Mode::NewPath { .. } => self.key_new_path(key),
            Mode::ConfirmDelete { .. } => self.key_confirm_delete(key),
            Mode::Help => self.mode = Mode::List,
        }
    }

    fn key_list(&mut self, key: KeyEvent) {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        match key.code {
            KeyCode::Esc => {
                if self.filter.is_empty() {
                    self.quit = true;
                } else {
                    self.filter.clear();
                    self.apply_filter();
                }
            }
            KeyCode::Up => self.selected = self.selected.saturating_sub(1),
            KeyCode::Down => {
                self.selected = (self.selected + 1).min(self.filtered.len().saturating_sub(1))
            }
            KeyCode::Char('k') if ctrl => self.selected = self.selected.saturating_sub(1),
            KeyCode::Char('j') if ctrl => {
                self.selected = (self.selected + 1).min(self.filtered.len().saturating_sub(1))
            }
            KeyCode::Enter => self.open_selected(),
            KeyCode::Char('s') if ctrl => {
                self.source = match self.source {
                    Source::Sessions => Source::Projects,
                    Source::Projects | Source::Cleanup => Source::Sessions,
                };
                self.session_agent = None;
                self.filter.clear();
                self.selected = 0;
                self.status = Some(Status::info(match self.source {
                    Source::Sessions => "loading sessions…",
                    _ => "loading projects…",
                }));
                self.pending = Some(Pending::Reload);
            }
            KeyCode::Char('g') if ctrl => {
                self.source = if self.source == Source::Cleanup {
                    Source::Projects
                } else {
                    Source::Cleanup
                };
                self.session_agent = None;
                self.filter.clear();
                self.selected = 0;
                self.status = Some(Status::info(match self.source {
                    Source::Cleanup => "scanning repositories for cleanable worktrees…",
                    _ => "loading projects…",
                }));
                self.pending = Some(Pending::Reload);
            }
            KeyCode::Tab if self.source == Source::Sessions => self.cycle_session_agent(1),
            KeyCode::BackTab if self.source == Source::Sessions => self.cycle_session_agent(-1),
            KeyCode::Char('n') if ctrl => self.mode = Mode::NewPath { input: "~/".into() },
            KeyCode::Char('d') if ctrl => self.delete_selected(),
            KeyCode::Char('x') if ctrl && self.source == Source::Cleanup => {
                self.confirm_remove_all()
            }
            KeyCode::Char('r') if ctrl => {
                self.status = Some(Status::info("reloading…"));
                self.pending = Some(Pending::Reload);
            }
            KeyCode::Char('?') if self.filter.is_empty() => self.mode = Mode::Help,
            KeyCode::Backspace => {
                self.filter.pop();
                self.apply_filter();
            }
            KeyCode::Char(c) if !ctrl => {
                self.filter.push(c);
                self.apply_filter();
            }
            _ => {}
        }
    }

    fn open_selected(&mut self) {
        let Some(entry) = self.selected_entry() else {
            return;
        };
        match &entry.kind {
            EntryKind::Workspace { id, .. } => {
                ext::focus_workspace(id);
                self.quit = true;
            }
            EntryKind::Remote(host) => {
                ext::open_remote(host);
                self.quit = true;
            }
            EntryKind::Worktree(p) | EntryKind::Cleanable { path: p, .. } | EntryKind::Dir(p) => {
                self.open_launch_form(p.clone());
            }
            EntryKind::Session(session) => {
                let session = session.clone();
                self.pending = Some(Pending::Resume(session));
                self.status = Some(Status::info("resuming session…"));
            }
        }
    }

    fn cycle_session_agent(&mut self, delta: isize) {
        let order = [
            SessionAgent::Claude,
            SessionAgent::Codex,
            SessionAgent::Cursor,
            SessionAgent::Pi,
        ];
        let available: Vec<Option<SessionAgent>> = std::iter::once(None)
            .chain(
                order
                    .into_iter()
                    .filter(|agent| {
                        self.entries
                            .iter()
                            .any(|e| matches!(&e.kind, EntryKind::Session(s) if s.agent == *agent))
                    })
                    .map(Some),
            )
            .collect();
        let current = available
            .iter()
            .position(|agent| *agent == self.session_agent)
            .unwrap_or(0) as isize;
        let next = (current + delta).rem_euclid(available.len() as isize) as usize;
        self.session_agent = available[next];
        self.selected = 0;
        self.apply_filter();
    }

    fn delete_selected(&mut self) {
        let Some(entry) = self.selected_entry() else {
            return;
        };
        match &entry.kind {
            EntryKind::Workspace { id, .. } => {
                self.mode = Mode::ConfirmDelete {
                    msg: format!("close workspace {}?", entry.label),
                    action: DelAction::CloseWs(id.clone()),
                };
            }
            EntryKind::Worktree(p) => {
                let (path, label) = (p.clone(), entry.label.clone());
                self.status = Some(Status::info("checking worktree state…"));
                self.pending = Some(Pending::DeleteWorktree { path, label });
            }
            EntryKind::Cleanable { path, clean } => {
                if *clean {
                    self.mode = Mode::ConfirmDelete {
                        msg: format!("remove clean integrated worktree {}?", entry.label),
                        action: DelAction::RemoveMerged(path.clone()),
                    };
                } else {
                    self.status = Some(Status::info(
                        "integrated, but dirty; clean the worktree before removal",
                    ));
                }
            }
            EntryKind::Dir(_) | EntryKind::Remote(_) | EntryKind::Session(_) => {
                self.status = Some(Status::info(
                    "not a workspace or worktree; nothing to delete",
                ));
            }
        }
    }

    fn confirm_remove_all(&mut self) {
        let mut paths = Vec::new();
        let mut projects = HashSet::new();
        let mut dirty = 0;
        for &index in &self.filtered {
            let entry = &self.entries[index];
            if let EntryKind::Cleanable { path, clean } = &entry.kind {
                if *clean {
                    paths.push(path.clone());
                    projects.insert(entry.label.split('/').next().unwrap_or(""));
                } else {
                    dirty += 1;
                }
            }
        }
        if paths.is_empty() {
            self.status = Some(Status::info(if dirty == 0 {
                "no clean integrated worktrees in this view".into()
            } else {
                format!("no removable worktrees; {dirty} integrated worktrees are dirty")
            }));
            return;
        }
        let skipped = if dirty == 0 {
            String::new()
        } else {
            format!("; {dirty} dirty will be skipped")
        };
        self.mode = Mode::ConfirmDelete {
            msg: format!(
                "remove {} clean integrated worktrees across {} projects{skipped}?",
                paths.len(),
                projects.len()
            ),
            action: DelAction::RemoveAll(paths),
        };
    }

    fn key_confirm_delete(&mut self, key: KeyEvent) {
        // Take the mode by value: arms either consume the action or restore it.
        let Mode::ConfirmDelete { msg, action } = std::mem::replace(&mut self.mode, Mode::List)
        else {
            return;
        };
        match (key.code, action) {
            (KeyCode::Esc, _) => {}
            (KeyCode::Char('f'), DelAction::OfferForce(p, branch)) => {
                self.mode = Mode::ConfirmDelete {
                    msg: format!("force-remove {branch}? deletes uncommitted work AND the branch"),
                    action: DelAction::ForceArmed(p, branch),
                };
            }
            // Confirmed removals run as pending ops so the status below is
            // visible while wt/herdr subprocesses do the work.
            (
                KeyCode::Char('y'),
                action @ (DelAction::CloseWs(_)
                | DelAction::RemoveMerged(_)
                | DelAction::RemoveAll(_)
                | DelAction::ForceArmed(..)),
            ) => {
                self.status = Some(Status::info(match &action {
                    DelAction::CloseWs(_) => "closing workspace…",
                    DelAction::RemoveAll(_) => "removing clean worktrees…",
                    _ => "removing worktree…",
                }));
                self.pending = Some(Pending::Delete(action));
            }
            (_, action) => self.mode = Mode::ConfirmDelete { msg, action },
        }
    }

    fn key_launch(&mut self, key: KeyEvent) {
        let n_agents = self.agents.len() + 1; // + "none"
        let agents = &self.agents;
        let Mode::Launch(form) = &mut self.mode else {
            return;
        };
        match key.code {
            KeyCode::Esc => self.mode = Mode::List,
            KeyCode::Tab => {
                form.field = (form.field + 1) % 3;
                form.candidate_selected = None;
            }
            KeyCode::BackTab => {
                form.field = (form.field + 2) % 3;
                form.candidate_selected = None;
            }
            KeyCode::Down if form.field == 1 => form.move_candidate(1),
            KeyCode::Up if form.field == 1 => form.move_candidate(-1),
            KeyCode::Down => form.field = (form.field + 1) % 3,
            KeyCode::Up => form.field = (form.field + 2) % 3,
            KeyCode::Enter => {
                if form.field == 1 && form.accept_candidate() {
                    return;
                }
                let msg = if form.branch.trim().is_empty() {
                    "building deck…"
                } else {
                    "resolving checkout, running hooks…"
                };
                self.pending = Some(Pending::Launch {
                    dir: form.dir.clone(),
                    agent: self.agents.get(form.agent).cloned(),
                    branch: form.branch.clone(),
                    dangerous: form.dangerous,
                });
                self.status = Some(Status::info(msg));
            }
            KeyCode::Left if form.field == 0 => form.agent = (form.agent + n_agents - 1) % n_agents,
            KeyCode::Right | KeyCode::Char(' ') if form.field == 0 => {
                form.agent = (form.agent + 1) % n_agents
            }
            KeyCode::Char(' ')
                if form.field == 2
                    && agents
                        .get(form.agent)
                        .is_some_and(|a| ext::dangerous_toggleable(a)) =>
            {
                form.dangerous = !form.dangerous
            }
            KeyCode::Backspace if form.field == 1 => {
                form.branch.pop();
                form.candidate_selected = None;
            }
            KeyCode::Char(c) if form.field == 1 => {
                form.branch.push(c);
                form.candidate_selected = None;
            }
            _ => {}
        }
    }

    fn key_new_path(&mut self, key: KeyEvent) {
        let Mode::NewPath { input } = &mut self.mode else {
            return;
        };
        match key.code {
            KeyCode::Esc => self.mode = Mode::List,
            KeyCode::Backspace => {
                input.pop();
            }
            KeyCode::Char(c) => input.push(c),
            KeyCode::Enter => {
                let raw = input.trim().to_string();
                if raw.is_empty() {
                    self.mode = Mode::List;
                    return;
                }
                let mut p = ext::expand_tilde(&raw);
                if !p.starts_with('/') {
                    p = format!("{}/{p}", ext::home());
                }
                match std::fs::create_dir_all(&p) {
                    Ok(()) => self.open_launch_form(PathBuf::from(p)),
                    Err(e) => {
                        self.status = Some(Status::err(format!("mkdir failed: {e}")));
                        self.mode = Mode::List;
                    }
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn launch_form(branches: &[&str]) -> LaunchForm {
        LaunchForm {
            dir: PathBuf::from("/repo"),
            agent: 0,
            branch: String::new(),
            candidates: branches
                .iter()
                .map(|branch| ext::WorktreeCandidate {
                    branch: (*branch).into(),
                    path: PathBuf::from(format!("/repo.wt/{branch}")),
                    current: false,
                })
                .collect(),
            candidates_loading: false,
            candidate_selected: None,
            dangerous: true,
            field: 1,
        }
    }

    #[test]
    fn checkout_candidates_filter_and_accept_without_blocking_free_form_input() {
        let mut form = launch_form(&["main", "feature/picker", "fix-preview"]);
        form.branch = "pick".into();
        assert_eq!(form.matching_candidates(), vec![1]);
        assert!(!form.accept_candidate());

        form.move_candidate(1);
        assert!(form.accept_candidate());
        assert_eq!(form.branch, "feature/picker");
        assert_eq!(form.candidate_selected, None);
    }

    #[test]
    fn create_hint_only_for_free_form_input_without_exact_match() {
        let mut form = launch_form(&["main", "feature/picker"]);
        assert_eq!(form.pending_create(), None); // empty input

        form.branch = "pick".into(); // fuzzy match exists, but Enter creates
        assert_eq!(form.pending_create(), Some("pick"));

        form.move_candidate(1); // highlighted candidate: Enter accepts it
        assert_eq!(form.pending_create(), None);
        form.candidate_selected = None;

        form.branch = "feature/picker".into(); // exact worktree: opens it
        assert_eq!(form.pending_create(), None);

        for shortcut in ["^", "-", "@", "pr:12"] {
            form.branch = shortcut.into(); // worktrunk resolves these
            assert_eq!(form.pending_create(), None, "{shortcut}");
        }

        form.branch = "  spaced  ".into();
        assert_eq!(form.pending_create(), Some("spaced"));

        // While candidates are still loading, an exact match may be on its
        // way, so no create hint.
        form.candidates_loading = true;
        assert_eq!(form.pending_create(), None);
    }

    #[test]
    fn checkout_candidate_navigation_wraps() {
        let mut form = launch_form(&["main", "feature/picker"]);
        form.move_candidate(-1);
        assert_eq!(form.candidate_selected, Some(1));
        form.move_candidate(1);
        assert_eq!(form.candidate_selected, Some(0));
    }
}
