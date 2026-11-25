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
| Navigate | `Tab` view, `f`/`F` file, `]`/`[` hunk, `o` jump to file, `/` search, `n`/`N` match |
| Review | `c` comment, `m` edit note, `1` focus notes, `R` resend, `s` send, `S` target |
| Git | `a` stage, `C-a` stage all, `u` unstage, `C-u` unstage all, `A` stage hunk, `gc` commit, `r` refresh |
| View | `v` split, `b` comments, `L` commit log, `J`/`K` scroll log diff |
| Tools | `t` new agent tab, `e` editor tab, `E` editor float, `?` help |

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

Unknown keys are ignored. Duplicate key bindings produce a warning in the status line.

## Local state

diffnote writes `.diffnote/comments.json` (your review notes) and `.diffnote/inbox/*.md` (payload snapshots) inside each repo. It adds `.diffnote/` to that repo's `.git/info/exclude`, which is local and never committed — no `.gitignore` changes needed in your projects.

Comments survive restarts and re-anchor to the matching line when the diff changes; comments that no longer match are marked orphan instead of being dropped.

## Optional Rex integration

`rex/diffnote.lua` binds `cmd+r` to open diffnote in a floating pane beside the focused agent. It is optional — diffnote works without Rex. Load it from your Rex `init.lua`:

```lua
dofile(os.getenv("HOME") .. "/path/to/diffnote/rex/diffnote.lua").setup()
```

## Contributing

Issues and pull requests are welcome at [github.com/galihhhp/diffnote](https://github.com/galihhhp/diffnote).

```sh
cargo test
cargo clippy --all-targets
```

## License

MIT. See [LICENSE](LICENSE).
