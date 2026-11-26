# diffnote

A terminal UI for reviewing git diffs, leaving line comments, staging changes, committing, and sending those comments to a coding agent.

Repo: [github.com/galihhhp/diffnote](https://github.com/galihhhp/diffnote)

It combines a focused one-file-at-a-time review (like `reviewr`) with a git panel (like `lazygit`). By default it shows the file list, a split diff, and the comment list. Each pane can be toggled.

Works in any terminal. Agent delivery works through Rex, tmux, a filesystem inbox, or the clipboard. Opening a file in your editor and spawning a new agent tab work in Rex and tmux.

## Install

```sh
git clone git@github.com:galihhhp/diffnote.git
cd diffnote
cargo install --path .
```

Or install straight from GitHub:

```sh
cargo install --git https://github.com/galihhhp/diffnote.git
```

Requires Rust 1.98+ and `git`. Everything else is optional:

| Feature | Needs |
| --- | --- |
| Core review/staging/commit | `git` |
| Send to a running agent | `rex` or `tmux` |
| New agent tab / editor open | `rex` or `tmux` |
| Clipboard target | `pbcopy` (macOS), `wl-copy`, `xclip`, or `xsel` |
| Editor open | `$EDITOR` / `$VISUAL` (default `nvim`) |

## Quick start

```sh
cd your-repo
diffnote
```

diffnote detects staged, unstaged, and untracked files. `s` opens a picker asking where to send your comments — pick the active agent, a new tab, the inbox, or the clipboard. `q` quits.

## Keys

Run `?` inside diffnote for the full list. Defaults:

| Group | Keys |
| --- | --- |
| Navigate | `Tab` view, `f`/`F` file, `]`/`[` hunk, `o` jump to file, `/` search, `n`/`N` match, `h`/`l`/`0` pan split view |
| Review | `c` comment, `m` edit note, `1` focus notes, `R` resend, `s` send, `S` target |
| Git | `g` opens the git command menu, `ga` stage, `gA` stage all, `gu` unstage, `gU` unstage all, `gh` stage hunk, `gc` commit, `gr` refresh, `gs` update branch, `gb` new branch, `gB` choose base, `gn` rename, `gl` pull, `gp` push, `gP` force-with-lease push, `gz` stash push (asks for a title), `gZ` stash pop (pick from a list) |
| View | `v` split, `b` comments, `L` commit log, `J`/`K` scroll log diff |
| Tools | `t` new agent tab, `e` editor tab, `E` editor float, `?` help |
| Select | `V` select lines, then `A` stage selection or `c` range comment |

Multiline input: `C-j` or `Shift+Enter` for a newline, `Enter` to commit, `C-u` to clear. Pasting multiline text works too.

## Config

`~/.config/diffnote/config.toml` (or `$XDG_CONFIG_HOME/diffnote/config.toml`, or `$DIFFNOTE_CONFIG`). Copy `config.example.toml` to start.

```toml
send_mode = "submit"
split = true
show_comments = true
adapters = ["rex", "tmux", "inbox", "clipboard"]
agents = ["pi", "claude", "codex", "opencode", "aider", "goose", "cline"]

new_tab_agent = "pi"
clipboard_command = "pbcopy"
editor_command = "nvim"
base_branch = "main"
update_command = "{update_script} {base}"
branchup_command = "{branchup_script} {branch} --base {base}"

[templates]
pi = 'pi -- @{file} "Please address these review comments"'
claude = 'claude "Please address the review comments in {file}"'
codex = 'codex "Please address the review comments in {file}"'
opencode = 'opencode run "Please address the review comments in {file}"'
aider = 'aider --message-file {file}'
goose = 'goose run --instructions {file}'
cline = 'cline "Please address the review comments in {file}"'
```

`{file}` expands to a shell-quoted absolute path of the saved review payload. `new_tab_agent` selects a template from `[templates]`; the default is `pi`. Set `new_tab_command` directly to override the selected template. Add your own harness by adding it to `agents` and giving it a template.

| Setting | Meaning |
| --- | --- |
| `agents` | Process names diffnote recognizes when detecting running agents |
| `adapters` | Delivery backends: `rex`, `tmux`, `inbox`, `clipboard` |
| `send_mode` | `submit` presses Enter, `queue` only pastes |
| `clipboard_command` | Any program reading stdin; auto-detected across macOS/Wayland/X11 |
| `editor_command` | Overridden by `$DIFFNOTE_NVIM`, then `$VISUAL`, then `$EDITOR` |
| `split`, `show_comments` | Default pane visibility; toggled with `v` and `b` |
| `base_branch` | Branch to compare against; header shows ahead/behind and "up to date"; `p` changes it for the session from a list of local branches |
| `rename_branch`, `push` | Key bindings for `C` (rename current branch) and `P` (`git push -u origin HEAD`), both confirmed before running |
| `update_command`, `branchup_command` | Shell commands run with `gs` / `gb`; `{base}` and `{branch}` expand before running |

All git command keys start with `g` (like `gc` for commit). Press `g` to open the menu, then the second key. `gB` opens a list of local branches to pick the base for this session; the header and `gs`/`gb` update immediately. `gn` prompts for a new name and renames the current local branch. `gl` runs `git pull`, `gp` runs `git push -u origin HEAD`, and `gP` runs `git push -u origin HEAD --force-with-lease`. All of them ask for confirmation first. `gP` uses a red warning because it can rewrite remote history. Every git command shows a loading overlay while it runs, then the command output. `gp` and `gP` refuse to run on a detached HEAD.

`U` runs `update_command` and `B` prompts for a branch name then runs `branchup_command`. Both run through `$SHELL -ic` (falling back to `/bin/sh`), and output appears in a dismissible overlay before the view refreshes. `{update_script}` and `{branchup_script}` expand to bundled POSIX-sh scripts in `scripts/`, which diffnote writes to `$XDG_DATA_HOME/diffnote/scripts/` on first use. Replace those tokens with any command you like — including shell functions from your own `.zshrc`.

The bundled scripts stash uncommitted changes, pull the base branch, and rebase/create a branch. They mutate the worktree; review `scripts/git_update.sh` and `scripts/branchup.sh` before using them. If the base branch does not exist locally, the header says so.

Unknown keys are ignored. Duplicate key bindings produce a warning in the status line.

## Line selection and range comments

`V` starts visual line selection and jumps to the nearest `+`/`-` line, so a single `V` already selects a real change. `j`/`k` extend it, `A` stages just the selected lines via `git apply --cached --recount`, and `c` opens a comment covering the whole range. Range comments show as `start-end` in the notes panel and include the selected code in the payload sent to your agent. `Esc` cancels.

## Bottom panel

The bottom row is split in two: comments take 70% on the left, the stash list 30% on the right (`Stash (n)` shows how many you have). `p` focuses the comments panel; `gz` asks for a title and stashes the working tree including untracked files; `gZ` opens a picker listing every stash so you choose which one to pop.

## Local state

diffnote writes `.diffnote/comments.json` (your review notes) and `.diffnote/inbox/*.md` (payload snapshots) inside each repo. It adds `.diffnote/` to that repo's `.git/info/exclude`, which is local and never committed — no `.gitignore` changes needed in your projects.

Comments survive restarts and re-anchor to the matching line when the diff changes; comments that no longer match are marked orphan instead of being dropped.

## Optional Rex integration

`rex/diffnote.lua` binds `cmd+shift+r` to open diffnote in a floating pane beside the focused agent. It is optional — diffnote works without Rex. The plugin resolves the `diffnote` binary through an interactive shell at setup time, because the Rex server runs with a minimal `PATH`; override `command` with an absolute path if yours is elsewhere. After editing `init.lua`, run `rex api call server.config_reload '{}'` so the binding registers. Load it from your Rex `init.lua`:

```lua
dofile(os.getenv("HOME") .. "/path/to/diffnote/rex/diffnote.lua")
  .setup({ key = "cmd+shift+r" })
```

Override `key` to any free chord, or set `key = false` to keep only the `diffnote_open` action in Rex's action palette. `command`, `ratio`, `float`, and `bounds` are overridable the same way.

## Contributing

Issues and pull requests are welcome at [github.com/galihhhp/diffnote](https://github.com/galihhhp/diffnote).

```sh
cargo test
cargo clippy --all-targets
```

## License

MIT. See [LICENSE](LICENSE).
