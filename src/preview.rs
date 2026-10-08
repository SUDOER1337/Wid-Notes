use egui::{self, Color32, RichText};
use pulldown_cmark::{Event, HeadingLevel, Parser, Tag, TagEnd};

enum Block {
    None,
    Heading(HeadingLevel),
    Paragraph,
    Code,
    ListItem(usize),
}

pub fn render_markdown(ui: &mut egui::Ui, text: &str, dark: bool, excluded: &[Color32]) {
    let mut block = Block::None;
    let mut buf = String::new();
    let mut list_depth: usize = 0;

    for event in Parser::new(text) {
        match &event {
            Event::Start(Tag::Heading { level, .. }) => {
                flush_block(ui, &mut block, &mut buf, dark, excluded);
                block = Block::Heading(*level);
            }
            Event::End(TagEnd::Heading(_)) => {
                flush_block(ui, &mut block, &mut buf, dark, excluded);
            }
            Event::Start(Tag::CodeBlock(_)) => {
                flush_block(ui, &mut block, &mut buf, dark, excluded);
                block = Block::Code;
            }
            Event::End(TagEnd::CodeBlock) => {
                flush_block(ui, &mut block, &mut buf, dark, excluded);
            }
            Event::Start(Tag::Paragraph) => {
                flush_block(ui, &mut block, &mut buf, dark, excluded);
                block = Block::Paragraph;
            }
            Event::End(TagEnd::Paragraph) => {
                flush_block(ui, &mut block, &mut buf, dark, excluded);
            }
            Event::Start(Tag::List(_)) => {
                flush_block(ui, &mut block, &mut buf, dark, excluded);
                list_depth += 1;
            }
            Event::End(TagEnd::List(_)) => {
                list_depth = list_depth.saturating_sub(1);
            }
            Event::Start(Tag::Item) => {
                flush_block(ui, &mut block, &mut buf, dark, excluded);
                block = Block::ListItem(list_depth);
            }
            Event::End(TagEnd::Item) => {
                flush_block(ui, &mut block, &mut buf, dark, excluded);
            }
            Event::Rule => {
                flush_block(ui, &mut block, &mut buf, dark, excluded);
                ui.separator();
                ui.add_space(4.0);
            }
            Event::Text(t) => buf.push_str(t),
            Event::HardBreak | Event::SoftBreak => buf.push('\n'),
            _ => {}
        }
    }
    flush_block(ui, &mut block, &mut buf, dark, excluded);
}

fn flush_block(
    ui: &mut egui::Ui,
    block: &mut Block,
    buf: &mut String,
    dark: bool,
    excluded: &[Color32],
) {
    match std::mem::replace(block, Block::None) {
        Block::Heading(level) => {
            if !buf.is_empty() {
                let size = match level {
                    HeadingLevel::H1 => 24.0,
                    HeadingLevel::H2 => 20.0,
                    HeadingLevel::H3 => 18.0,
                    _ => 16.0,
                };
                ui.add_space(6.0);
                ui.label(
                    RichText::new(buf.as_str())
                        .size(size)
                        .strong()
                        .color(block_color(dark, heading_color(level), excluded)),
                );
                ui.separator();
                ui.add_space(2.0);
            }
        }
        Block::Paragraph => {
            if !buf.is_empty() {
                ui.label(buf.as_str());
                ui.add_space(4.0);
            }
        }
        Block::Code => {
            if !buf.is_empty() {
                egui::Frame::dark_canvas(ui.style()).show(ui, |ui| {
                    ui.label(
                        RichText::new(buf.as_str())
                            .monospace()
                            .color(Color32::from_rgb(180, 220, 180)),
                    );
                });
                ui.add_space(4.0);
            }
        }
        Block::ListItem(depth) => {
            if !buf.is_empty() {
                let indent = depth as f32 * 20.0;
                let bullet = if depth % 2 == 0 {
                    "\u{2022}"
                } else {
                    "\u{2013}"
                };
                ui.horizontal(|ui| {
                    ui.add_space(indent);
                    ui.label(RichText::new(bullet).color(block_color(
                        dark,
                        Color32::YELLOW,
                        excluded,
                    )));
                    ui.label(buf.as_str());
                });
                ui.add_space(2.0);
            }
        }
        Block::None => {}
    }
    buf.clear();
}

fn heading_color(level: HeadingLevel) -> Color32 {
    match level {
        HeadingLevel::H1 => Color32::from_rgb(255, 180, 100),
        HeadingLevel::H2 => Color32::from_rgb(200, 200, 255),
        HeadingLevel::H3 => Color32::from_rgb(180, 220, 180),
        _ => Color32::from_rgb(200, 200, 200),
    }
}

/** Pick an accent color for the current polarity.
 *  Dark palettes are authored for dark backgrounds; light mode inverts so the
 *  text stays readable, unless `excluded` spares the color. Code blocks keep
 *  their dark canvas, so code text is left alone.
 *  @param dark - Whether the app is running the dark palette
 *  @param c - Color authored for the dark palette
 *  @param excluded - Colors to leave as-authored during inversion
 */
fn block_color(dark: bool, c: Color32, excluded: &[Color32]) -> Color32 {
    if dark {
        c
    } else {
        crate::theme::invert_color_if(c, excluded)
    }
}
