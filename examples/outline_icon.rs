// File: examples/outline_icon.rs
// Functions:
//   main()       - Outline every <text> in an SVG, writing a font-free SVG
//   pick_serif() - Pick the first preferred serif family present in the font db
//
// Dev tool (run via `just outline-icon`). The shipped icon must not depend on a
// system font, so `assets/icons/icon.src.svg` (editable, uses a <text> "W") is
// converted to `assets/icons/icon.svg` with the glyph baked into a <path>.

use resvg::usvg;
use resvg::usvg::fontdb;

/// Serif families to try when the SVG asks for one that isn't installed
/// (the source icon uses "Times", which many systems lack).
const SERIF_CANDIDATES: &[&str] = &[
    "DejaVu Serif",
    "Liberation Serif",
    "Times New Roman",
    "FreeSerif",
    "Noto Serif",
];

/// Convert every `<text>` in `args[1]` to outlines and write the result to
/// `args[2]`.
fn main() {
    let mut args = std::env::args().skip(1);
    let (Some(input), Some(output)) = (args.next(), args.next()) else {
        eprintln!("usage: outline_icon <in.svg> <out.svg>");
        std::process::exit(2);
    };

    let mut opt = usvg::Options::default();
    opt.fontdb_mut().load_system_fonts();
    if let Ok(dir) = std::env::var("WID_ICON_FONT_DIR") {
        opt.fontdb_mut().load_fonts_dir(dir);
    }
    if let Some(serif) = pick_serif(opt.fontdb_mut()) {
        opt.fontdb_mut().set_serif_family(serif);
    }

    let data = std::fs::read(&input).expect("failed to read input SVG");
    let tree = usvg::Tree::from_data(&data, &opt).expect("failed to parse SVG");
    std::fs::write(&output, tree.to_string(&usvg::WriteOptions::default()))
        .expect("failed to write output SVG");

    eprintln!("outlined {input} -> {output}");
}

/// Return the first family from `SERIF_CANDIDATES` that exists in `db`.
fn pick_serif(db: &fontdb::Database) -> Option<&'static str> // Pick an available serif family
{
    SERIF_CANDIDATES.iter().copied().find(|name| {
        let query = fontdb::Query {
            families: &[fontdb::Family::Name(name)],
            weight: fontdb::Weight::NORMAL,
            stretch: fontdb::Stretch::Normal,
            style: fontdb::Style::Normal,
        };
        db.query(&query).is_some()
    })
}
