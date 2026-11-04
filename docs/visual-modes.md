# Visual modes

`tgg` draws the graph column in one of two modes. Everything else (table, detail, menus,
keys) is the same in both.

## Text mode

The default everywhere. Lanes and nodes are box-drawing characters, so it works in any
terminal, over SSH, and inside tmux or zellij. Every feature is available.

## Graphics mode

On terminals that support the Kitty graphics protocol (for example ghostty or kitty), the
graph column is drawn as anti-aliased images, one per row: smooth lines, curved
joins and forks, round nodes. Only the graph column changes; text is never drawn as an
image.

Graphics are shown only on the graph screen. While a menu, dialog or the "Terminal too
small" message is open, the images are removed and the column is drawn as text. On exit
(or panic) all images are deleted from the terminal. If writing an image fails, `tgg`
switches to text mode for the rest of the run and shows a notice.

## Configuration

Both keys are optional, in `~/.config/terminal-git-graph/config.toml`:

```toml
# "auto": use graphics when the terminal is detected to support them.
# "text": never use graphics.
# "graphics": like auto, but warn at startup if graphics cannot be used.
renderer = "auto"
# Round pill caps on badges (needs a font with the ◖ ◗ glyphs).
nerd_font = false
```

An invalid value gives a warning and the default is used. There are no command-line flags
for these.

## Detection and limits

Before the event loop starts, `tgg` asks the terminal whether it supports Kitty graphics
and what its cell size is, waiting about 450 ms at worst (300 ms plus a 150 ms grace
window). Environment variables alone never turn graphics on; the terminal has to answer.

- tmux and zellij (detected via `TMUX` / `ZELLIJ`) and `TERM=dumb` or `TERM=linux` use
  text mode without asking the terminal.
- With `renderer = "graphics"`, if detection fails `tgg` starts in text mode and shows a
  notice saying so.
- With `renderer = "text"` nothing is asked of the terminal.

To show how your own setup looks, capture a screenshot yourself and add it under `docs/`.

## Real-terminal checklist

Run through this by hand in **ghostty** and in a **herdr** split pane (`tgg` in a repository
with merges and several branches):

- [ ] Graphics turn on: smooth lanes and round nodes in the graph column.
- [ ] Mouse: click, double click and right click on graph rows still select and open menus.
- [ ] Scroll: wheel and keys scroll without flicker, tearing or leftover images.
- [ ] Menu overlap: opening a context menu or dialog over the graph shows no image
      bleeding through; closing it brings the images back.
- [ ] Resize: dragging the window or the split keeps the images aligned with their rows;
      shrinking below the minimum size and growing back recovers.
- [ ] Exit: after `q`, no graph image is left in the terminal or its scrollback area.
- [ ] `renderer = "text"` in the config gives the text graph, with no images at all.
- [ ] Inside tmux (or with `TERM=dumb`), the text graph is used.
