# Terminal browser

The terminal browser presents local archive data through the same read-only engine queries used by
the CLI. It never contacts GitHub, reads `--config` or `FORGESYNC_CONFIG`, loads model settings, or
changes the archive.

Start it with an initialized archive from an interactive terminal:

```sh
forgesync tui --archive PATH
```

The normal CLI build enables the `tui` feature. Builds made with
`--no-default-features` omit the command. Interactive output is not compatible with `--json`.

## Keyboard controls

| Key | Action |
| --- | --- |
| `Tab` / `Shift+Tab` | Move focus between repositories, discussions, and detail |
| `↑` / `k`, `↓` / `j` | Move the selected row or scroll detail |
| `Enter` | Apply a repository, or open the selected discussion detail |
| `/` | Enter a local keyword search; `Enter` submits it and `Esc` cancels it |
| `n` / `p` | Move to the next or previous page of discussions |
| `r` | Reload the current discussion page |
| `c` | Show archive coverage and local health |
| `f` | Show recent failed and unfinished runs |
| `Esc` | Return to browsing, clear search, or leave a detail pane |
| `q` / `Ctrl+C` | Exit |

The browser uses background engine queries, so keyboard input and redraws continue while local
queries are running. Older results are ignored after a newer query replaces them. The layout stacks
its panes on narrow terminals and uses side-by-side panes on wider terminals.
