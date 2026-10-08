# Theme Reference

Wid uses a cascading theme system. Colors are defined in TOML files and merged at startup.

## Cascade Order

```
1. Built-in Dark Theme (default)
       ↓
2. ~/.config/wid/theme.toml  (global override)
       ↓
3. <workspace>/theme.toml          (workspace override)
```

Each level fills in missing fields. A workspace theme with only `[editor]` changes editor colors; everything else inherits.

## Color Groups

| Section | Controls |
|---------|----------|
| `[editor]` | Editor background, text, cursor, selection, line numbers |
| `[sidebar]` | Sidebar panel, active items, borders |
| `[palette]` | Command palette overlay |
| `[titlebar]` | Custom title bar (when OS chrome is hidden) |
| `[status]` | Status bar |
| `[syntax]` | Syntax highlighting colors (future) |

## Example

```toml
[editor]
background = "#1e1e1e"
foreground = "#d4d4d4"
cursor = "#ffcc00"
selection_bg = "#264f78"
line_numbers = "#858585"
```

## File Locations

| Platform | Global Config Path |
|----------|-------------------|
| Windows | `C:\Users\<user>\AppData\Roaming\wid\theme.toml` |
| Linux | `~/.config/wid/theme.toml` |
| macOS | `~/Library/Application Support/wid/theme.toml` |

Create a `theme.toml` in any of these locations to customize colors. See `THEMING.md` for the full reference.
