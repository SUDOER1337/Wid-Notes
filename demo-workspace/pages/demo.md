# Wid Demo

A full-featured **Markdown** note to showcase _all_ the ~~hidden~~ formatting ~~gems~~ options.

---

## Text Formatting (Is not rendered correctly yet)

- **Bold** – `**Bold**` (Is not actually bold in preview)
- _Italic_ – `*Italic*` (Is not actually italic in the preview)
- **_Bold + Italic_** – `***Bold + Italic***` (same as above)
- ~~Strikethrough~~ – `~~Strikethrough~~` (Not Strikethrough)
- `Inline code` – backtick-wrapped

---

## Headings

# H1 – Document Title

## H2 – Section

### H3 – Subsection

#### H4 – Sub-subsection

##### H5 – Deep

###### H6 – Deepest

---

## Lists

### Unordered

- Red
- Green
- Blue
  - Cyan
  - Teal
    - Turquoise
- Yellow

### Ordered

1. First
2. Second
3. Third
   1. Third point A
   2. Third point B

### Task List (Need indicator in preview)

- [x] Implement vim bindings
- [x] Add Markdown preview
- [ ] Add undo/redo
- [ ] Add search

---

## Blockquotes

> This is a blockquote.
>
> > Nested blockquote — recursion at its finest.
>
> Back to the outer quote.

---

## Code

### Inline

Use the `std::fs::read_to_string` function to load a file.

### Fenced (Rust)

```rust
fn fibonacci(n: u32) -> u32 {
    match n {
        0 => 0,
        1 => 1,
        _ => fibonacci(n - 1) + fibonacci(n - 2),
    }
}

fn main() {
    for i in 0..10 {
        println!("fib({}) = {}", i, fibonacci(i));
    }
}
```

### Fenced (Python)

```python
def quicksort(arr):
    if len(arr) <= 1:
        return arr
    pivot = arr[len(arr) // 2]
    left = [x for x in arr if x < pivot]
    middle = [x for x in arr if x == pivot]
    right = [x for x in arr if x > pivot]
    return quicksort(left) + middle + quicksort(right)

print(quicksort([3, 6, 8, 10, 1, 2, 1]))
```

### Plain text block

```
$ cargo run --release
   Compiling wid v0.1.0
    Finished release profile [optimized] target(s) in 12.34s
     Running `target/release/wid.exe`
```

---

## Tables (Not Implemented yet)
  
| Key   | Action           | Mode     |
| ----- | ---------------- | -------- |
| `h`   | Move left        | Normal   |
| `j`   | Move down        | Normal   |
| `k`   | Move up          | Normal   |
| `l`   | Move right       | Normal   |
| `i`   | Insert at cursor | → Insert |
| `Esc` | Back to Normal   | ← Normal |
| `dd`  | Delete line      | Normal   |

---

## Images

- **Image of Rust Logo** ![Rust Logo](assets/rust-logo-512x512.png)

## Links (Can't be clicked on)
- **Link:** [egui on crates.io](https://crates.io/crates/egui)

---

## Horizontal Rules

Three ways to write them:

---

---

---

---

## Escaping Special Characters

Use a backslash to escape: \*not italic\*, \_not emphasized\_, \[not a link\].

---

## Hard Line Breaks (Yet to be implemented)

Two spaces at the end of a line  
create a line break.

Or use a backslash at the end of a line\

Like this.

---

## Unicode & Emoji

- Greek: α β γ δ ε ζ η θ
- Math: ∫ Σ √ π ∞ ≠ ≤ ≥
- Arrows: ← ↑ → ↓ ↔
- Emoji: ⚡ 🦀 🔥 💾 ✨

---

> "The best way to predict the future is to implement it."  
> — _Someone who probably used vim_
