# terminal-git-graph

A terminal git graph viewer. `tgg` draws a repository's commit graph with its branches,
remotes and tags, shows each commit's files and diffs, and runs common git actions from
menus. Pointed at a folder instead of a repository, it lists the repositories inside
(Manygit) and opens their graphs from there.

It runs in any terminal with keyboard and mouse. On terminals that support the Kitty
graphics protocol the graph is drawn as smooth images; everywhere else it is text. See
[docs/visual-modes.md](docs/visual-modes.md).

## Install

Needs a Rust toolchain and `git` on `PATH`. From a clone of this repository:

```sh
cargo install --path .
```

This installs the `tgg` binary. macOS comes first; Linux should work. Windows is not
supported.

## Usage

```sh
tgg [PATH]
```

- `PATH` is a repository (or a folder inside one): its graph is shown.
- `PATH` is a plain folder: the repositories found under it are listed (Manygit).
- Without `PATH`, the current folder is used.

Press `?` for the keys, `q` to quit. All bindings are in [docs/keys.md](docs/keys.md).

## Features

- Commit graph of all branches or only the current one, with remote branches shown or
  hidden. Branch, remote and tag labels sit on their commits.
- Commits load 300 at a time; scrolling to the end loads the next 300.
- Commit detail: message, author, changed files and each file's diff. Uncommitted changes
  get their own row and detail.
- Menus on branches, remote branches, tags, commits and uncommitted changes: checkout,
  create/rename/delete branches, merge, rebase, cherry-pick, revert, reset
  (soft/mixed/hard), add/delete/push tags, push, delete remote branches, commit, stash,
  copy names and hashes.
- Fetch from the toolbar or with `f`.
- Every change asks for confirmation first, except checking out a local branch and
  Fetch. If git refuses, its own message is shown. Resetting uncommitted changes keeps
  untracked files.
- Refreshes by itself when refs or files in the repository change.
- Manygit: a folder's repositories with their branch, whether they have uncommitted
  changes and how far ahead/behind they are, refreshed every 15 seconds; pull, push,
  fetch, fetch all, commit, stash, reset and open the remote's web page.
- Keyboard and mouse: click, double click, right click menus, wheel.
- Graphics mode: anti-aliased graph lanes and nodes on Kitty-graphics terminals (such as
  ghostty), detected at startup; text mode everywhere else, including tmux and zellij
  (detected via `TMUX` / `ZELLIJ`).
- Falls back to 256 colors without true color, and to ASCII graph lines on terminals
  without box drawing.
- No network access except the fetch, pull, push and remote-branch delete you run. No
  telemetry or update checks.

## Configuration

One optional file, `~/.config/terminal-git-graph/config.toml`. Every key is optional;
these are the defaults:

```toml
# Folder levels Manygit searches for repositories (1 to 5).
scan_depth = 3
# Show each repository's latest tag in the Manygit list.
show_tags = false
# Color theme.
theme = "default"
# "auto" uses graphics when the terminal supports them; "text" never does;
# "graphics" is like auto but warns if they cannot be used.
renderer = "auto"
# Round pill caps on badges (needs a font with the ◖ ◗ glyphs).
nerd_font = false
```

There is no settings screen. An invalid file is reported at startup and the defaults are
used.

## Development

```sh
cargo test
cargo test --release -- --ignored   # performance checks on a 50,000-commit repository
cargo clippy --all-targets -- -D warnings
cargo deny check licenses
```

## License

MIT. See [LICENSE](LICENSE).

Inspired by herdr-git-graph and VS Code Git Graph. No code is shared with either.
