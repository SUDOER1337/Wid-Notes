# Vim Reference

## Modes

| Mode | Description | Enter |
|------|-------------|-------|
| Normal | Default — navigate, manipulate text | `Esc` |
| Insert | Type text | `i`, `I`, `a`, `A`, `o`, `O` |
| Visual | Select text | `v` (char), `V` (line) |
| Command Line | Run ex commands | `:`, `/`, `?` |

## Motions

### Cursor

| Key | Action |
|-----|--------|
| `h` / `j` / `k` / `l` | Left / Down / Up / Right |
| `w` / `b` / `e` | Word forward / back / end |
| `W` / `B` / `E` | WORD forward / back / end |
| `0` | Line start |
| `$` | Line end |
| `^` | First non-blank character |
| `gg` | File start |
| `G` | File end |
| `{` / `}` | Paragraph back / forward |
| `%` | Matching bracket |
| `Ctrl+f` / `Ctrl+b` | Page down / up |
| `f{char}` / `F{char}` | Find char forward / back |
| `t{char}` / `T{char}` | Till char forward / back |
| `;` / `,` | Repeat last find / reverse |

### Count prefix

Most motions accept a count: `3j`, `5w`, `2Ctrl+f`

## Operators

Operators compose with motions: `d{motion}`, `y{motion}`, `c{motion}`

| Key | Action |
|-----|--------|
| `d` | Delete |
| `y` | Yank (copy) |
| `c` | Change (delete + insert) |
| `~` | Toggle case |
| `gu` | Lowercase |
| `gU` | Uppercase |
| `>` | Indent |
| `<` | Outdent |

### Linewise operators

| Key | Action |
|-----|--------|
| `dd` | Delete line |
| `yy` | Yank line |
| `cc` | Change line |
| `D` | Delete to end of line |
| `C` | Change to end of line |
| `Y` | Yank line (same as yy) |
| `J` | Join lines |
| `>>` / `<<` | Indent / outdent line |

### Text objects

Used as `d{i,a}{object}` — `i` = inner, `a` = outer (includes surrounding)

| Object | Description |
|--------|-------------|
| `w` | Word |
| `W` | WORD |
| `p` | Paragraph |
| `(`, `)`, `b` | Parentheses / block |
| `[`, `]` | Brackets |
| `{`, `}`, `B` | Curly braces |
| `"` | Double quotes |
| `'` | Single quotes |

Examples: `di(` (delete inside parens), `ca"` (change around quotes), `yip` (yank inner paragraph)

## Visual Mode

| Key | Action |
|-----|--------|
| `v` | Start visual selection (charwise) |
| `V` | Start visual selection (linewise) |
| `d` / `x` | Delete selection |
| `y` | Yank selection |
| `c` | Change selection |
| `~` | Toggle case on selection |
| `>` / `<` | Indent / outdent selection |

Motions work in visual mode to extend the selection.

## Registers

| Key | Action |
|-----|--------|
| `"{name}` | Select register for next operation |
| `"a`-`"z` | Named registers |
| `""` | Unnamed register |
| `"-` | Small delete register |
| `p` / `P` | Paste after / before cursor |

## Undo / Redo

| Key | Action |
|-----|--------|
| `u` | Undo |
| `Ctrl+r` | Redo |
| `.` | Repeat last change |

## Search

| Key | Action |
|-----|--------|
| `/pattern` | Search forward |
| `?pattern` | Search backward |
| `n` / `N` | Next / previous match |
| `:noh` | Clear search highlights |

## Ex Commands

| Command | Action |
|---------|--------|
| `:w` | Save |
| `:q` | Quit |
| `:wq` / `:x` | Save and quit |
| `:q!` | Force quit |
| `:e <name>` | Open page |
| `:help` | Display help buffer |
| `:noh` | Clear search highlights |

## Tips

- Use count + operator + motion: `3dw` deletes 3 words
- Use count + motion: `5j` moves down 5 lines
- Use register + operator: `"adw` deletes word into register `a`
- Compose freely: `"b3dd` deletes 3 lines into register `b`
