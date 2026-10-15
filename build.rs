use std::env;
use std::fs;
use std::path::{Path, PathBuf};

// File: build.rs
// Functions:
//   main()               - Generate the runtime icon, and (on Windows) embed an .ico
//   render_png()         - Render the icon SVG to an encoded PNG at a given size
//   build_ico()          - Pack PNG frames into an ICO container (Windows only)
//   embed_windows_icon() - Build the multi-size .ico and embed it (Windows only)

/// Square pixel size of the generated runtime window icon.
const ICON_SIZE: u32 = 256;

/// Sizes packed into the Windows `.ico` (16 → 256 covers taskbar to large tiles).
#[cfg(windows)]
const ICO_SIZES: &[u32] = &[16, 32, 48, 64, 128, 256];

/// Render `assets/icons/icon.svg` into `$OUT_DIR/icon.png` so `main.rs` can
/// `include_bytes!` it as the runtime window/taskbar icon. On Windows, also
/// embed a multi-size `.ico` into the executable as the Explorer shell icon.
///
/// The committed SVG is font-free (its glyphs are already outlined), so this
/// only needs resvg's plain shape renderer — no system fonts required.
fn main() {
    let manifest_dir = env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR not set");
    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR not set"));

    let svg_path = Path::new(&manifest_dir).join("assets/icons/icon.svg");
    let png_path = out_dir.join("icon.png");

    println!("cargo:rerun-if-changed={}", svg_path.display());

    let svg_data = fs::read(&svg_path).expect("Failed to read assets/icons/icon.svg");
    let tree = resvg::usvg::Tree::from_data(&svg_data, &resvg::usvg::Options::default())
        .expect("Failed to parse icon SVG");

    fs::write(&png_path, render_png(&tree, ICON_SIZE)).expect("Failed to write icon.png");

    #[cfg(windows)]
    embed_windows_icon(&out_dir, &tree);
}

/// Render `tree` to an encoded PNG of `size`×`size` pixels.
fn render_png(tree: &resvg::usvg::Tree, size: u32) -> Vec<u8> // Render SVG to PNG bytes
{
    // Scale the square source into a square target. Using the larger dimension
    // keeps the artwork fully inside the frame for non-square inputs too.
    let source = tree.size().to_int_size();
    let scale = size as f32 / source.width().max(source.height()).max(1) as f32;

    let mut pixmap = resvg::tiny_skia::Pixmap::new(size, size).expect("Failed to allocate pixmap");
    resvg::render(
        tree,
        resvg::tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );

    pixmap.encode_png().expect("Failed to encode PNG")
}

/// Assemble an ICO file from `(size, png_bytes)` frames. Frames are stored as
/// PNGs (supported by Windows Vista+), so no BMP conversion is needed.
#[cfg(windows)]
fn build_ico(frames: &[(u32, Vec<u8>)]) -> Vec<u8> // Pack PNG frames into an ICO
{
    let mut out = Vec::new();
    out.extend_from_slice(&0u16.to_le_bytes()); // reserved
    out.extend_from_slice(&1u16.to_le_bytes()); // type: 1 = icon
    out.extend_from_slice(&(frames.len() as u16).to_le_bytes());

    // Directory entries precede the image data.
    let mut offset = 6 + 16 * frames.len() as u32;
    for (size, data) in frames {
        // A zero width/height encodes 256 in the ICO directory.
        let dim = if *size >= 256 { 0 } else { *size as u8 };
        out.push(dim); // width
        out.push(dim); // height
        out.push(0); // palette color count
        out.push(0); // reserved
        out.extend_from_slice(&1u16.to_le_bytes()); // color planes
        out.extend_from_slice(&32u16.to_le_bytes()); // bits per pixel
        out.extend_from_slice(&(data.len() as u32).to_le_bytes());
        out.extend_from_slice(&offset.to_le_bytes());
        offset += data.len() as u32;
    }

    for (_, data) in frames {
        out.extend_from_slice(data);
    }
    out
}

/// Render `ICO_SIZES`, pack them into `$OUT_DIR/icon.ico`, and embed it in the
/// binary as the Windows shell icon.
#[cfg(windows)]
fn embed_windows_icon(out_dir: &Path, tree: &resvg::usvg::Tree) // Embed .ico into the .exe
{
    let frames: Vec<(u32, Vec<u8>)> = ICO_SIZES
        .iter()
        .map(|&size| (size, render_png(tree, size)))
        .collect();

    let ico_path = out_dir.join("icon.ico");
    fs::write(&ico_path, build_ico(&frames)).expect("Failed to write icon.ico");

    winresource::WindowsResource::new()
        .set_icon(ico_path.to_str().expect("icon path is valid UTF-8"))
        .compile()
        .expect("Failed to embed Windows icon resource");
}
