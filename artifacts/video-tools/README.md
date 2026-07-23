# herdr-deck showcase tooling

The Cap projects and privacy-safe fixture data live under
`/Users/Shared/herdr-deck-demo`. Product source files are not changed.

- `capture-scene.sh` records any one of the seven scenes so each Cap process
  remains inside a short controller invocation. It resolves the current picker
  workspace by label and raises the exact `review-demo` window before capture.
- `showcase.filter` contains the 4K caption and assembly edit.
- `assemble.sh` renders the seven Cap exports into the final deliverable.

The live Herdr demo uses the named session `herdr-deck-demo`. Its home,
configuration, repositories, Git identity, shell history, and agent-session
history are isolated from the operator's normal environment.

`demo-env` starts commands from a strict environment allowlist. In particular,
host credentials, history, account names, and `NO_COLOR` do not enter the demo
session. `ghostty-demo.conf` fixes the terminal to a true-color Catppuccin
palette so the captured rendering matches the application theme.

`codex-demo` is the privacy-safe agent surface used for the recording. It is
clearly labeled as an isolated demo session, performs no network request, and
contains no account state.
