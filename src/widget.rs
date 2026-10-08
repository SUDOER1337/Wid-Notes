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
    let (rect, response) = ui.allocate_exact_size(available, egui::Sense::click_and_drag());

    if response.clicked() {
        ui.memory_mut(|mem| mem.request_focus(response.id));
    }

    // Route keyboard events to vim unless a text field (chat, search, prompt,
    // settings) holds them. Deliberately NOT gated on this widget's focus:
    // egui surrenders focus on every Escape (EventFilter.escape defaults to
    // false) and our auto-generated id shifts when the central-panel layout
    // changes — either would silently kill all keyboard input while the
    // cursor keeps painting. Text fields consume their events during their
    // own pass (all rendered before the editor), so nothing is double-handled.
    if !ui.ctx().text_edit_focused() {
        ui.ctx().input_mut(|input| {
            let mut keep = Vec::new();
            for ev in input.events.drain(..) {
                match &ev {
                    Event::Key { .. } | Event::Text(_) => {
                        if let Some(kp) = KeyPress::from_egui_event(&ev) {
                            state.handle_key(kp, buffer);
                            continue;
                        }
                    }
                    _ => {}
                }
                keep.push(ev);
            }
            input.events = keep;
        });
    }

    // Mouse: a plain click places the cursor; a drag selects text and enters
    // visual (charwise) mode, matching vim's mouse behavior. `clicked` never
    // fires on a drag, so the two paths are mutually exclusive.
    let inner = rect.shrink2(Vec2::new(es.padding, es.padding));
    let scroll = view_scroll(buffer, inner, es);

    if response.drag_started() {
        // Anchor at the press position — interact_pointer_pos tracks the
        // current drag position, which would collapse the selection.
        if let Some(press) = ui.ctx().input(|i| i.pointer.press_origin()) {
            let at = pos_to_byte(rect, es, buffer, scroll, press);
            buffer.set_cursor(at);
            buffer.start_selection();
            state.mode = Mode::Visual(SelectionMode::Charwise);
        }
    }
    if response.dragged_by(egui::PointerButton::Primary) {
        if let Some(pos) = ui.ctx().input(|i| i.pointer.latest_pos()) {
            let at = pos_to_byte(rect, es, buffer, scroll, pos);
            buffer.set_cursor(at);
            if let Some(ref mut sel) = buffer.selection {
                sel.active = at;
            }
        }
    }
    if response.clicked_by(egui::PointerButton::Primary) {
        if let Some(press) = response.interact_pointer_pos() {
            let at = pos_to_byte(rect, es, buffer, scroll, press);
            buffer.set_cursor(at);
            buffer.clear_selection();
            if matches!(state.mode, Mode::Visual(_)) {
                state.mode = Mode::Normal;
            }
        }
    }

    paint_buffer(ui, buffer, state, rect, es, ec);
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
    let (cursor_line, cursor_col) = buffer.line_col(buffer.cursor);
    let scroll = view_scroll(buffer, inner, es);

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

/** First visible line for the current cursor, clamped to the buffer.
 *  Shared by paint and mouse hit-testing so both use the same grid.
 *  @param buffer - Buffer whose cursor drives the tracking
 *  @param inner - Editor rect after padding shrink
 *  @param es - Editor metrics (line height)
 *  @returns The topmost line index to display
 */
fn view_scroll(buffer: &Buffer, inner: Rect, es: &crate::settings::EditorSettings) -> usize // Cursor-tracked scroll
{
    let total = buffer.line_count();
    let visible = (inner.height() / es.line_height) as usize;
    let (cursor_line, _) = buffer.line_col(buffer.cursor);
    sticky_scroll(cursor_line, total, visible).min(total.saturating_sub(visible))
}

/** Map a pointer position to the nearest buffer byte offset.
 *  Uses the same monospace grid the painter uses (padding + gutter + char
 *  width); out-of-bounds coordinates clamp to the buffer.
 *  @param rect - Full editor rect
 *  @param es - Editor metrics (padding, gutter, char width, line height)
 *  @param buffer - Buffer to hit-test against
 *  @param scroll - First visible line (from `view_scroll`)
 *  @param pos - Pointer position in screen space
 *  @returns Byte offset of the hit character
 */
fn pos_to_byte(
    rect: Rect,
    es: &crate::settings::EditorSettings,
    buffer: &Buffer,
    scroll: usize,
    pos: Pos2,
) -> usize // Pointer → buffer offset
{
    let inner = rect.shrink2(Vec2::new(es.padding, es.padding));
    let text_start_x = inner.left() + es.gutter_width;
    let line = if pos.y <= inner.top() {
        0
    } else {
        scroll + ((pos.y - inner.top()) / es.line_height) as usize
    };
    let line = line.min(buffer.line_count().saturating_sub(1));
    let col = if pos.x <= text_start_x {
        0
    } else {
        ((pos.x - text_start_x) / es.char_width) as usize
    };
    buffer.byte_from_line_col(line, col)
}
