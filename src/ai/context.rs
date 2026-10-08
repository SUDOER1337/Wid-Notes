use std::fs;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

pub struct NoteChunk {
    pub title: String,
    pub content: String,
}

/// Walk a workspace directory (pages/ and journals/), parse `.md` files,
/// and return the `max_notes` most recently modified files as a context string.
pub fn build_context(workspace_root: &Path, max_notes: usize) -> String {
    let mut notes: Vec<(PathBuf, u64)> = Vec::new();

    let pages_dir = workspace_root.join("pages");
    let journals_dir = workspace_root.join("journals");

    if pages_dir.is_dir() {
        collect_md_files(&pages_dir, &mut notes);
    }
    if journals_dir.is_dir() {
        collect_md_files(&journals_dir, &mut notes);
    }

    notes.sort_by(|a, b| b.1.cmp(&a.1));

    let mut chunks: Vec<NoteChunk> = Vec::new();
    for (path, _) in notes.iter().take(max_notes) {
        if let Some(note) = parse_file(path) {
            chunks.push(note);
        }
    }

    format_context(&chunks)
}

fn collect_md_files(dir: &Path, out: &mut Vec<(PathBuf, u64)>) {
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                collect_md_files(&path, out);
            } else if path.extension().and_then(|s| s.to_str()) == Some("md") {
                let mtime = fs::metadata(&path)
                    .ok()
                    .and_then(|m| m.modified().ok())
                    .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                    .map(|d| d.as_secs())
                    .unwrap_or(0);
                out.push((path, mtime));
            }
        }
    }
}

fn parse_file(path: &Path) -> Option<NoteChunk> {
    let text = fs::read_to_string(path).ok()?;
    let title = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("untitled")
        .replace('_', " ")
        .replace('-', " ");

    let display_name = if let Some(parent) = path.parent() {
        let folder = parent.file_name().and_then(|s| s.to_str()).unwrap_or("");
        match folder {
            "journals" => format!("Journal: {}", title),
            "pages" => format!("Page: {}", title),
            _ => title.clone(),
        }
    } else {
        title.clone()
    };

    let clean = clean_md(&text);
    if clean.trim().is_empty() {
        return None;
    }

    let truncated = if clean.len() > 2000 {
        format!("{}...", &clean[..2000])
    } else {
        clean
    };

    Some(NoteChunk {
        title: display_name,
        content: truncated,
    })
}

fn clean_md(text: &str) -> String {
    let mut out = Vec::new();
    let mut in_fence = false;
    let mut in_drawer = false;
    let mut in_props = true;

    for line in text.lines() {
        let trimmed = line.trim();

        if trimmed.starts_with("```") {
            in_fence = !in_fence;
            if in_fence {
                out.push("[code block]".to_string());
            }
            continue;
        }
        if in_fence {
            continue;
        }

        if trimmed.starts_with(":LOGBOOK:") {
            in_drawer = true;
            continue;
        }
        if trimmed.starts_with(":END:") {
            in_drawer = false;
            continue;
        }
        if in_drawer {
            continue;
        }

        if in_props {
            if trimmed.is_empty() {
                in_props = false;
                continue;
            }
            if trimmed.contains("::") && !trimmed.starts_with("```") {
                continue;
            }
            in_props = false;
        }

        if trimmed.starts_with("property::") || (trimmed.contains("::") && !trimmed.contains(' ')) {
            continue;
        }

        if trimmed == "---" || trimmed == "___" || trimmed == "***" {
            continue;
        }

        let cleaned = clean_line(line);
        if !cleaned.is_empty() {
            out.push(cleaned);
        }
    }

    out.join("\n")
}

fn clean_line(line: &str) -> String {
    let mut s = line.to_string();

    s = strip_pattern(r"\(\([^)]+\)\)", &s);
    s = strip_pattern(r"\^\^([^^]+)\^\^", &s);
    s = strip_pattern(r"\[\[([^\]]+)\]\]", &s);
    s = strip_pattern(
        r"^\s*(TODO|DOING|DONE|LATER|NOW|WAITING|CANCELED|CANCELLED)\s+",
        &s,
    );
    s = strip_pattern(r"^(\s*)[-*]\s+", &s);
    s = strip_pattern(r"^#+\s+", &s);

    s.trim().to_string()
}

fn strip_pattern(pattern: &str, text: &str) -> String {
    match pattern {
        r"\(\([^)]+\)\)" => strip_double_parens(text),
        r"\^\^([^^]+)\^\^" => strip_caret_highlight(text),
        r"\[\[([^\]]+)\]\]" => strip_wiki_link(text),
        r"^\s*(TODO|DOING|DONE|LATER|NOW|WAITING|CANCELED|CANCELLED)\s+" => strip_task_prefix(text),
        r"^(\s*)[-*]\s+" => strip_bullet(text),
        r"^#+\s+" => strip_heading_marker(text),
        _ => text.to_string(),
    }
}

fn strip_double_parens(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let mut i = 0;
    let bytes = text.as_bytes();
    while i < text.len() {
        if bytes[i] == b'(' && i + 1 < text.len() && bytes[i + 1] == b'(' {
            if let Some(end) = text[i + 2..].find("))") {
                i += end + 4;
                continue;
            }
        }
        result.push(text[i..].chars().next().unwrap());
        i += text[i..].chars().next().unwrap().len_utf8();
    }
    result
}

fn strip_caret_highlight(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let mut i = 0;
    let bytes = text.as_bytes();
    while i < text.len() {
        if bytes[i] == b'^' && i + 1 < text.len() && bytes[i + 1] == b'^' {
            let start = i + 2;
            if let Some(end) = text[start..].find("^^") {
                result.push_str(&text[start..start + end]);
                i = start + end + 2;
                continue;
            }
        }
        result.push(text[i..].chars().next().unwrap());
        i += text[i..].chars().next().unwrap().len_utf8();
    }
    result
}

fn strip_wiki_link(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let mut i = 0;
    while i < text.len() {
        if text[i..].starts_with("[[") {
            if let Some(end) = text[i + 2..].find("]]") {
                result.push_str(&text[i + 2..i + 2 + end]);
                i += end + 4;
                continue;
            }
        }
        result.push(text[i..].chars().next().unwrap());
        i += text[i..].chars().next().unwrap().len_utf8();
    }
    result
}

fn strip_task_prefix(text: &str) -> String {
    let trimmed = text.trim_start();
    for prefix in &[
        "TODO ",
        "DOING ",
        "DONE ",
        "LATER ",
        "NOW ",
        "WAITING ",
        "CANCELED ",
        "CANCELLED ",
    ] {
        if trimmed.starts_with(prefix) {
            let rest = &trimmed[prefix.len()..];
            let indent = &text[..text.len() - text.trim_start().len()];
            return format!("{}{}", indent, rest);
        }
    }
    text.to_string()
}

fn strip_bullet(text: &str) -> String {
    let trimmed = text.trim_start();
    if trimmed.starts_with("- ") || trimmed.starts_with("* ") {
        let indent = &text[..text.len() - text.trim_start().len()];
        let content = &trimmed[2..];
        return format!("{}{}", indent, content);
    }
    text.to_string()
}

fn strip_heading_marker(text: &str) -> String {
    let trimmed = text.trim_start();
    let mut i = 0;
    for c in trimmed.chars() {
        if c == '#' {
            i += 1;
        } else {
            break;
        }
    }
    if i > 0 && trimmed[i..].starts_with(' ') {
        return format!(
            "{}{}",
            &text[..text.len() - trimmed.len()],
            &trimmed[i + 1..]
        );
    }
    text.to_string()
}

fn format_context(chunks: &[NoteChunk]) -> String {
    if chunks.is_empty() {
        return String::new();
    }

    let mut parts = vec!["The user has these recent notes from their workspace:".to_string()];
    for chunk in chunks {
        parts.push(format!("---\n### {}\n{}", chunk.title, chunk.content));
    }
    parts.push(String::new());

    parts.join("\n\n")
}
