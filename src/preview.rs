use egui::{self, Color32, RichText};
use pulldown_cmark::{Event, HeadingLevel, Parser, Tag, TagEnd};

enum Block {
    None,
    Heading(HeadingLevel),
    Paragraph,
    Code,
    ListItem(usize),
}

pub fn render_markdown(ui: &mut egui::Ui, text: &str) {
    let mut block = Block::None;
    let mut buf = String::new();
    let mut list_depth: usize = 0;

    for event in Parser::new(text) {
        match &event {
            Event::Start(Tag::Heading { level, .. }) => {
                flush_block(ui, &mut block, &mut buf);
                block = Block::Heading(*level);
            }
            Event::End(TagEnd::Heading(_)) => {
                flush_block(ui, &mut block, &mut buf);
            }
            Event::Start(Tag::CodeBlock(_)) => {
                flush_block(ui, &mut block, &mut buf);
                block = Block::Code;
            }
            Event::End(TagEnd::CodeBlock) => {
                flush_block(ui, &mut block, &mut buf);
            }
            Event::Start(Tag::Paragraph) => {
                flush_block(ui, &mut block, &mut buf);
                block = Block::Paragraph;
            }
            Event::End(TagEnd::Paragraph) => {
                flush_block(ui, &mut block, &mut buf);
            }
            Event::Start(Tag::List(_)) => {
                flush_block(ui, &mut block, &mut buf);
                list_depth += 1;
            }
            Event::End(TagEnd::List(_)) => {
                list_depth = list_depth.saturating_sub(1);
            }
            Event::Start(Tag::Item) => {
                flush_block(ui, &mut block, &mut buf);
                block = Block::ListItem(list_depth);
            }
            Event::End(TagEnd::Item) => {
                flush_block(ui, &mut block, &mut buf);
            }
            Event::Rule => {
                flush_block(ui, &mut block, &mut buf);
                ui.separator();
                ui.add_space(4.0);
            }
            Event::Text(t) => buf.push_str(t),
            Event::HardBreak | Event::SoftBreak => buf.push('\n'),
            _ => {}
        }
    }
    flush_block(ui, &mut block, &mut buf);
}

fn flush_block(ui: &mut egui::Ui, block: &mut Block, buf: &mut String) {
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
                        .color(heading_color(level)),
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
                    ui.label(RichText::new(bullet).color(Color32::YELLOW));
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
