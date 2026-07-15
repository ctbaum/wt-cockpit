<p align="center">
  <img src="assets/wt-cockpit-logo.png" alt="wt-cockpit retro-punk logo" width="420">
</p>

# wt-cockpit

An opinionated cockpit manager for [herdr](https://herdr.dev): one picker to browse,
launch, and tear down agent workspaces bound to git worktrees.

wt-cockpit runs inside a herdr pane and drives everything by shelling out to the
`herdr` and [`wt` (worktrunk)](https://github.com/max-sixty/worktrunk) CLIs.
No daemon, no async runtime, one small binary.

> [!IMPORTANT]
> This is my personal workflow extracted into a public binary, not a generic
> Herdr workspace manager. The picker is reusable; the cockpit it builds is
> deliberately coupled to my nvim, agent, and terminal setup. Read
> [Requirements and compatibility](#requirements-and-compatibility) before
> installing.

```
╭─ wt-cockpit [projects] ─╮╭──────────────────── dotfiles/main ────────────────────╮
│> dot▌             3 / 42││ nvim                         │ codex                  │
╰─────────────────────────╯│                              │                        │
╭──────── Results ────────╮│                              │                        │
│> ● dotfiles/main        ││──────────────────────────────┴────────────────────────│
│  ● dotfiles/ratatui-ux  ││ zsh                                                   │
│  ▸ ~/dotfiles.wt/...    ││                                                       │
╰─────────────────────────╯╰───────────────────────────────────────────────────────╯
```

## What it does

- **Browse**: live herdr workspaces first (agents blocked on you sort to the
  top), then worktrunk worktrees, then your zoxide directories. Type to
  filter. The preview shows a live 2D thumbnail of each workspace's actual
  pane layout, or worktree status (branch, merge state, dirty flags), or a
  directory listing.
- **Remotes**: set `WT_COCKPIT_REMOTES` to a comma/space-separated list of ssh
  aliases and each remote herdr server becomes an entry (`⇄`); Enter opens a
  `herdr --remote` thin client in its own terminal window, leaving the local
  session alone (running it inside a pane would nest herdr in herdr).
- **Open**: Enter on a live workspace focuses it. Enter on a worktree or
  directory opens a launch form: pick an agent (detected from your PATH,
  using herdr's known-agent list), optionally name a branch (the worktree is
  created via `wt switch` if needed), and go. wt-cockpit builds a cockpit
  workspace: editor + agent pane + full-width terminal + a lazygit tab.
- **Resume**: `ctrl-s` switches to a separate session-history source, so past
  conversations never pollute workspace/path search. Type searches the first
  prompt and project path; Tab filters by agent. Claude, Codex, and Pi sessions
  resume in a recreated cockpit rooted at the session's original directory.
  Cursor opens its native session picker in that cockpit because its CLI does
  not expose an enumerable local history store.
- **Create**: `ctrl-n` prompts for a new directory. A new worktree in an
  existing repo is just Enter on the repo plus the branch field.
- **Destroy**: `ctrl-d` closes a workspace, or removes a worktree — but only
  when its branch is merged (worktrunk's `integrated`/`empty` state); an
  unmerged worktree gets an explicit force-remove confirmation instead.
  Removed paths are purged from zoxide.

## Looking for something less opinionated?

Try [Herdr Navigator](https://github.com/thanhdat77/herdr-navigator). It is a
configurable Herdr plugin that fuzzy-searches workspaces, agents, projects,
sessions, remotes, directories, and actions. Sources can be disabled, custom
command/JSON integrations can be added without changing Rust, and missing
optional tools degrade quietly.

The distinction is mostly intent: Herdr Navigator helps you **jump to
anything**; wt-cockpit recreates **my particular working cockpit** around the
thing you selected.

## Requirements and compatibility

### Minimum

- [Herdr](https://herdr.dev), with `wt-cockpit` launched from inside a Herdr
  session. It talks directly to Herdr's socket CLI and exits otherwise.
- A Unix-like environment. The core local workflow is intended for macOS or
  Linux; shell command construction and agent discovery assume Unix paths and
  process behavior.
- `git` for repository detection, branch labels, and worktree-aware behavior.
- `nvim` for every cockpit launch. It is not optional: the editor pane always
  runs `nvim`.
- The CLI for whichever agent you select, available on `PATH`. Herdr's direct
  agent integrations are strongly recommended so status indicators work.

Rust and Cargo are needed only to build from source. Installing the binary
does not install or configure any of the tools below.

### Feature dependencies

| feature | dependency | behavior when missing |
|---|---|---|
| linked-worktree create/remove | [worktrunk](https://github.com/max-sixty/worktrunk) (`wt`) | ordinary directory cockpits still work; worktree actions and status do not |
| directory discovery | `zoxide` and/or `fd` | that source becomes sparse or empty |
| directory preview | `eza`, with `ls` fallback | falls back to plain `ls -la` |
| git tab | `lazygit` | the tab is still created, but its command fails |
| Claude cockpit | Claude Code + [claudecode.nvim](https://github.com/coder/claudecode.nvim) + compatible nvim glue | nvim opens, but Claude does not auto-start |
| saved sessions | agent-owned local history files | only histories found at the supported hardcoded locations appear |
| remote entries | macOS `open` + Ghostty | remote launch is unavailable on other terminals/platforms |

Saved-session discovery currently reads `~/.claude/projects`,
`~/.codex/sessions`, and `~/.pi/agent/sessions`. Cursor exposes discovery only
through its own picker, so wt-cockpit opens `cursor-agent ls`. These are
agent-owned storage formats and may change underneath wt-cockpit.

### Claude and nvim contract

Claude is the most personal integration. Unlike other agents, wt-cockpit does
not create its agent pane itself. It creates the workspace with these variables
and starts nvim:

| variable | value set by wt-cockpit |
|---|---|
| `NIC_AI` | `claude` |
| `NIC_CLAUDE_ARGS` | `--dangerously-skip-permissions`, plus `--resume ID` for a saved session |

The `NIC_` names are inherited from the original shell `nic` function. Your
nvim configuration must consume them. This is the minimum auto-start glue for
a lazy.nvim claudecode spec:

```lua
{
  "coder/claudecode.nvim",
  opts = {},
  config = function(_, opts)
    require("claudecode").setup(opts)
    if vim.env.NIC_AI == "claude" then
      vim.schedule(function()
        vim.cmd("ClaudeCode " .. (vim.env.NIC_CLAUDE_ARGS or ""))
      end)
    end
  end,
}
```

That snippet starts Claude using whichever terminal provider you configured in
claudecode.nvim. The cockpit shown here goes further: my nvim config supplies a
custom Herdr external-pane provider, waits for its shell prompt, prevents a
duplicate Claude after an nvim restart, and uses Herdr pane commands for
selection sending. That provider is not bundled with wt-cockpit. Reproducing
the exact screenshot therefore requires equivalent personal nvim glue (and, in
my implementation, `folke/snacks.nvim`, `jq`, `pgrep`, `ps`, `sed`, and `sh`).

### Environment variables

| variable | direction | purpose |
|---|---|---|
| `NIC_AI`, `NIC_CLAUDE_ARGS` | wt-cockpit → workspace | Claude/nvim startup contract described above |
| `WT_COCKPIT_REMOTES` | user → wt-cockpit | comma/space-separated SSH aliases shown as remote entries |
| `HERDR_NAV_PASSTHROUGH_RE` | user → navigation plugin | lets `ctrl-j/k` reach wt-cockpit when using seamless pane navigation |
| `HERDR_*` | Herdr → processes | inherited session/socket identity; scrubbed only when opening a remote Ghostty window |

### Safety defaults

The launch form starts with **dangerous mode enabled**. Codex, Cursor, Devin,
Droid, Kimi, OpenCode, Kilo, Hermes, Qoder CLI, and other known agents receive
their built-in bypass/yolo flag or environment override when one is known.
Claude always receives `--dangerously-skip-permissions`; its toggle is disabled
because there is no safer branch in this personal workflow. Review
`src/ext.rs` before using this on a machine or repository where that default is
not acceptable.

## Install

Straight from GitHub (no clone needed):

```sh
cargo install --git https://github.com/ctbaum/wt-cockpit
```

Or from a clone:

```sh
git clone https://github.com/ctbaum/wt-cockpit
cd wt-cockpit
cargo install --path .
```

Either way the binary lands in `~/.cargo/bin` (make sure that's on your
PATH). Then bind it in `~/.config/herdr/config.toml`:

```toml
[[keys.command]]
key = "prefix+o"
type = "pane"
command = "wt-cockpit"
```

wt-cockpit exits when its pane loses focus, so the temporary pane never sticks
around.

## Keys

| key | action |
|-----|--------|
| type | filter (esc clears) |
| `↵` | focus workspace / open remote window / launch form / resume session |
| `ctrl-s` | switch projects / past sessions source |
| `tab` / `shift-tab` | sessions: cycle agent filter |
| `ctrl-n` | new directory, then launch form |
| `ctrl-d` | close workspace / merge-gated worktree remove |
| `ctrl-r` | reload |
| `ctrl-j/k` | move selection (needs passthrough, see below) |
| `?` | help |
| `esc` | back / quit |

If you use a herdr Ctrl-hjkl pane-navigation plugin (e.g.
vim-herdr-navigation), add `wt-cockpit` to its passthrough list so `ctrl-j/k`
reach the picker:

```sh
export HERDR_NAV_PASSTHROUGH_RE='^(tv|lazygit|wt-cockpit)$'
```

## Opinionated setup and compatibility

wt-cockpit mirrors my own personal workflow and layout:

- the cockpit layout is fixed: editor top-left, agent top-right, terminal
  bottom, lazygit on a new unfocused tab;
- `claude` is special-cased to start through nvim and claudecode.nvim, using
  the environment contract above;
- the "dangerous" toggle knows each agent's own yolo mechanism (flag or env)
  from a built-in table, and is **on by default**; Claude always receives
  `--dangerously-skip-permissions`, and unknown agents get no toggle;
- remote entries spawn their window via macOS `open` + Ghostty, hardcoded.

In practical terms: without nvim, cockpit launch is broken; without lazygit,
the git tab is empty; without the Claude/nvim glue, a Claude cockpit contains
only nvim; and selecting an agent normally launches it with its unsafe/yolo
mode enabled. These are current design choices, not graceful optional paths.

Worktrees are recognised structurally — any directory whose `.git` is a
file (a linked git worktree) — so any worktrunk `worktree-path` layout
works.

If any of these bite you, open an issue — making them configurable is the
obvious next step.
