# Key bindings

Taken from `src/input.rs`. Press `?` in `tgg` for a short version. `Ctrl+C` quits from
anywhere.

## Graph (commit table)

| Key | Action |
| --- | --- |
| `j` / `k`, `Down` / `Up` | Move the selection |
| `PgDn` / `PgUp` | Move one page |
| `g` / `G`, `Home` / `End` | First / last row (`End` loads the next page of commits) |
| `Enter` | Open the commit detail; with the detail open, focus its file list |
| `Tab` | Switch focus between the table and the detail's file list |
| `h` / `l`, `Left` / `Right` | Focus the previous / next ref label of the selected row |
| `m` | Menu of the focused label, else of the selected row |
| `a` | All branches ↔ current branch |
| `r` | Show / hide remote branches |
| `f` | Fetch |
| `s` | Switch to another repository (graph opened from the Manygit list) |
| `R`, `F5` | Refresh |
| `?` | Key help |
| `Esc` | Clear the label focus, else close the detail, else back to the Manygit list, else quit |
| `q` | Quit |

## Detail file list

| Key | Action |
| --- | --- |
| `j` / `k`, `Down` / `Up` | Next / previous file |
| `g`, `Home`, `PgUp` | First file |
| `G`, `End`, `PgDn` | Last file |
| `Enter` | Show the file's diff |
| `Tab` | Back to the table |
| `Esc` | Close the detail |

`m`, `a`, `r`, `f`, `R`/`F5`, `?` and `q` work as in the graph.

## Diff

| Key | Action |
| --- | --- |
| `j` / `k`, `Down` / `Up` | Scroll one line |
| `PgDn` / `PgUp` | Scroll one page |
| `g` / `G`, `Home` / `End` | Top / bottom |
| `Esc` | Close the diff |

`m`, `a`, `r`, `f`, `R`/`F5`, `?` and `q` work as in the graph.

## Menus

| Key | Action |
| --- | --- |
| `j` / `k`, `Down` / `Up` | Move |
| `Enter` | Run the selected item |
| `Esc`, `m`, `q` | Close |

## Dialogs

| Key | Action |
| --- | --- |
| `Enter` | Accept |
| `Esc` | Cancel |
| `Backspace` | Delete a character (input dialogs) |
| Any character | Type (input dialogs) |

## Manygit list

| Key | Action |
| --- | --- |
| `j` / `k`, `Down` / `Up` | Move |
| `PgDn` / `PgUp` | Move one page |
| `g` / `G`, `Home` / `End` | First / last repository |
| `Enter` | Open the repository's graph |
| `m` | Repository menu |
| `F` | Fetch all repositories |
| `R`, `F5` | Refresh |
| `?` | Key help |
| `q`, `Esc` | Quit |

## Mouse

| Action | Effect |
| --- | --- |
| Click | Select a row, ref label or file |
| Double click a row | Same as `Enter` |
| Double click a branch label | Check out that branch |
| Double click a file | Show its diff |
| Right click a row or label | Open its menu |
| Wheel | Move up / down (scrolls the diff when it is shown) |
| Click a toolbar button | Scope, remotes, Fetch, `‹ Manygit`, repository name (switch) |
| Click outside an open menu | Close it |

In the Manygit list, click selects, double click opens the graph, right click opens the
repository menu and the wheel moves.
