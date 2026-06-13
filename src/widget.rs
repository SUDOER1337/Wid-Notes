use crate::editor::{Buffer, SelectionMode};
use crate::settings::Settings;
use crate::theme::{EditorColors, Theme};
use crate::vim::{KeyPress, Mode, VimState};
use egui::{self, Align2, Event, FontId, Pos2, Rect, Vec2};

pub fn editor_ui(
    ui: &mut egui::Ui,
    state: &mut VimState,
    buffer: &mut Buffer,
    settings: &Settings,
    theme: &Theme,
) {
    let es = &settings.editor;
    let ec = &theme.editor;
    let available = ui.available_size();
    let (rect, response) = ui.allocate_exact_size(available, egui::Sense::click());

    if response.clicked() {
        ui.memory_mut(|mem| mem.request_focus(response.id));
        if let Some(pos) = response.interact_pointer_pos() {
            cursor_from_click(pos, rect, buffer, es);
        }
    }

    // Always drain and process events, regardless of focus state.
    // Widgets that consume events (TextEdit, consume_key) do so before
    // editor_ui runs, so there are no conflicts.
    ui.ctx().input_mut(|input| {
        let events: Vec<Event> = input.events.drain(..).collect();
        for ev in events {
            if let Some(kp) = KeyPress::from_egui_event(&ev) {
                log::trace!("Pressed: {}", kp);
                state.handle_key(kp, buffer);
            }
        }
    });

    paint_buffer(ui, buffer, state, rect, es, ec);
}

fn cursor_from_click(
    pos: Pos2,
    rect: Rect,
    buffer: &mut Buffer,
    es: &crate::settings::EditorSettings,
) {
    let inner = rect.shrink2(Vec2::new(es.padding, es.padding));
    let text_start_x = inner.left() + es.gutter_width;

    let click_line = ((pos.y - inner.top()) / es.line_height) as usize;
    let click_col = ((pos.x - text_start_x) / es.char_width).max(0.0) as usize;

    if click_line < buffer.line_count() {
        let line = buffer.line_text(click_line);
        let col = click_col.min(line.chars().count());
        buffer.set_cursor(buffer.byte_from_line_col(click_line, col));
    }
}

fn paint_buffer(
    ui: &egui::Ui,
    buffer: &Buffer,
    state: &VimState,
    rect: Rect,
    es: &crate::settings::EditorSettings,
    ec: &EditorColors,
) {
    let painter = ui.painter_at(rect);
    let font_id = FontId::monospace(16.0);

    let inner = rect.shrink2(Vec2::new(es.padding, es.padding));
    let text_start_x = inner.left() + es.gutter_width;

    painter.rect_filled(rect, 0.0, ec.background);

    if buffer.text.is_empty() {
        let hint = match state.mode {
            Mode::Normal => "~ Press 'i' to start editing",
            Mode::Insert => "~ Type to edit...",
            _ => "~ ",
        };
        painter.text(
            Pos2::new(text_start_x, inner.top() + es.line_height),
            Align2::LEFT_TOP,
            hint,
            font_id.clone(),
            ec.line_numbers,
        );
        if state.mode == Mode::Insert {
            let block = Rect::from_min_size(
                Pos2::new(text_start_x, inner.top()),
                Vec2::new(2.0, es.line_height),
            );
            painter.rect_filled(block, 0.0, ec.cursor_insert);
        }
        return;
    }

    let total_lines = buffer.line_count();
    let visible_lines = (inner.height() / es.line_height) as usize;

    // Determine scroll line from cursor
    let cursor_line = buffer.current_line_idx();
    let cursor_col = buffer.current_col();

    // Simple scroll tracking (stored in cursor_line for now — we'll refine later)
    let scroll_line = sticky_scroll(cursor_line, total_lines, visible_lines);

    let max_scroll = total_lines.saturating_sub(visible_lines);
    let scroll = scroll_line.min(max_scroll);

    let line_num_width = total_lines.max(1).to_string().len().max(3);

    // Selection ranges
    let sel_range = buffer.selection.as_ref().map(|s| {
        if s.mode == SelectionMode::Linewise {
            let start_line = buffer.line_col(s.start()).0;
            let end_line = buffer.line_col(s.end().saturating_sub(1)).0;
            buffer.line_start(start_line)..buffer.line_end(end_line)
        } else {
            s.start()..s.end()
        }
    });

    for i in 0..visible_lines.min(total_lines.saturating_sub(scroll)) {
        let line_idx = scroll + i;
        let line_text = buffer.line_text(line_idx);
        let y = inner.top() + i as f32 * es.line_height;
        let line_start_byte = buffer.line_start(line_idx);
        let line_end_byte = buffer.line_end(line_idx);

        // Selection highlight
        if let Some(ref sr) = sel_range {
            let sel_start = sr.start.max(line_start_byte);
            let sel_end = sr.end.min(line_end_byte);
            if sel_end > sel_start {
                let local_start = buffer.text[line_start_byte..sel_start].chars().count();
                let local_end = buffer.text[line_start_byte..sel_end].chars().count();
                let sx = text_start_x + local_start as f32 * es.char_width;
                let ex = text_start_x + local_end as f32 * es.char_width;
                painter.rect_filled(
                    Rect::from_min_size(Pos2::new(sx, y), Vec2::new(ex - sx, es.line_height)),
                    0.0,
                    ec.selection_bg,
                );
            }
        }

        // Line number
        let line_num = format!("{:>width$}", line_idx + 1, width = line_num_width);
        painter.text(
            Pos2::new(inner.left(), y),
            Align2::LEFT_TOP,
            &line_num,
            font_id.clone(),
            ec.line_numbers,
        );

        // Line text
        painter.text(
            Pos2::new(text_start_x, y),
            Align2::LEFT_TOP,
            line_text,
            font_id.clone(),
            ec.foreground,
        );

        // Cursor (always visible — editor processes events regardless of focus)
        if line_idx == cursor_line {
            let cursor_x = text_start_x + cursor_col as f32 * es.char_width;
            match state.mode {
                Mode::Normal | Mode::Visual(_) => {
                    let ch = line_text.chars().nth(cursor_col);
                    let block_w = ch.map(|_| es.char_width).unwrap_or(es.char_width * 0.5);
                    let block = Rect::from_min_size(
                        Pos2::new(cursor_x, y),
                        Vec2::new(block_w, es.line_height),
                    );
                    let color = match state.mode {
                        Mode::Visual(_) => ec.cursor_visual,
                        _ => ec.cursor,
                    };
                    painter.rect_filled(block, 0.0, color);
                    if let Some(c) = ch {
                        painter.text(
                            Pos2::new(cursor_x, y),
                            Align2::LEFT_TOP,
                            c.to_string(),
                            font_id.clone(),
                            ec.background,
                        );
                    }
                }
                Mode::Insert => {
                    let bar =
                        Rect::from_min_size(Pos2::new(cursor_x, y), Vec2::new(2.0, es.line_height));
                    painter.rect_filled(bar, 0.0, ec.cursor_insert);
                }
                Mode::CommandLine => {
                    // Don't draw cursor in command line mode — the text input is shown in status bar
                }
            }
        }
    }
}

fn sticky_scroll(cursor_line: usize, total_lines: usize, visible_lines: usize) -> usize {
    if visible_lines == 0 {
        return 0;
    }
    let ideal_top = cursor_line.saturating_sub(visible_lines / 2);
    let max_scroll = total_lines.saturating_sub(visible_lines);
    ideal_top.min(max_scroll)
}
