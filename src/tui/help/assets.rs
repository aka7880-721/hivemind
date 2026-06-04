// Copyright © 2026 ComfyHome™
// All rights reserved.
//
// Licensed under the ComfyGit SA-PS License
// For details, see the LICENSE file in the repository root.

use std::io::Cursor;
use std::path::PathBuf;
use std::sync::OnceLock;

use comfy_txt_engine::markdown::ImageResolver;
use image::imageops::FilterType;
use ratatui::layout::Rect;
use ratatui::style::Color;
use ratatui_image::picker::{Capability, Picker, ProtocolType, cap_parser::QueryStdioOptions};

static HELP_PICKER: OnceLock<Picker> = OnceLock::new();

/// Markdown column width frozen when the help modal first opens (not terminal resize).
pub const HELP_LAYOUT_WIDTH: u16 = 76;
/// Maximum image width in cells (within the frozen layout).
pub const HELP_IMAGE_MAX_WIDTH: u16 = 72;

#[derive(Clone)]
pub(crate) struct HelpImageResolver {
    base_dir: PathBuf,
    font_w: u16,
    font_h: u16,
    protocol_type: ProtocolType,
    layout_width: u16,
}

impl HelpImageResolver {
    pub(crate) fn new(relative_page_dir: &str, picker: &Picker) -> Self {
        let (font_w, font_h) = safe_font_size(picker);
        let base_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("src/tui/help")
            .join(relative_page_dir);
        Self {
            base_dir,
            font_w,
            font_h,
            protocol_type: picker.protocol_type(),
            layout_width: HELP_LAYOUT_WIDTH,
        }
    }
}

/// Query terminal capabilities once after entering the alternate screen (see `init_help_picker`).
pub(crate) fn init_help_picker() {
    let _ = HELP_PICKER.get_or_init(build_help_picker);
}

/// Cached picker from startup; falls back to a fresh query in unit tests.
pub(crate) fn help_picker() -> Picker {
    HELP_PICKER.get().cloned().unwrap_or_else(build_help_picker)
}

fn is_konsole() -> bool {
    std::env::var("KONSOLE_VERSION").is_ok_and(|s| !s.is_empty())
}

fn is_ptyxis_or_vte() -> bool {
    std::env::var("VTE_VERSION").is_ok_and(|s| !s.is_empty())
        || std::env::var("PTYXIS_VERSION").is_ok_and(|s| !s.is_empty())
        || std::env::var("TERM_PROGRAM").is_ok_and(|p| {
            let lower = p.to_ascii_lowercase();
            lower.contains("ptyxis")
                || lower == "vte"
                || lower.contains("gnome-terminal")
                || lower == "tilix"
        })
}

fn is_kitty_terminal() -> bool {
    std::env::var("KITTY_WINDOW_ID").is_ok_and(|s| !s.is_empty())
        || std::env::var("TERM").is_ok_and(|t| t.to_ascii_lowercase().contains("kitty"))
}

/// `ratatui-image` blacklists Kitty+Sixel IO probes for Konsole/WezTerm, which forces halfblocks
/// and heavy pixelation. KDE/VTE terminals need Sixel detection instead.
fn help_query_options() -> QueryStdioOptions {
    let blacklist_protocols = if is_konsole() {
        // Konsole kitty placeholders are incomplete — prefer sixel when both are reported.
        vec![ProtocolType::Kitty]
    } else if is_ptyxis_or_vte() {
        // Allow Sixel detection (Ptyxis/VTE ≥0.62).
        Vec::new()
    } else {
        Vec::new()
    };

    QueryStdioOptions {
        terminal_background_color_osc: true,
        blacklist_protocols,
        ..QueryStdioOptions::default()
    }
}

fn apply_help_protocol_preference(picker: &mut Picker) {
    let caps = picker.capabilities().clone();

    if is_kitty_terminal() && caps.contains(&Capability::Kitty) {
        picker.set_protocol_type(ProtocolType::Kitty);
        return;
    }

    if is_konsole() {
        // Konsole Sixel does not clear correctly in a scrolling TUI (ghost bands, wrong rows,
        // multi-second cleanup). Halfblocks render in the cell buffer and scroll cleanly.
        picker.set_protocol_type(ProtocolType::Halfblocks);
        return;
    }

    if is_ptyxis_or_vte() {
        if caps.contains(&Capability::Sixel) {
            picker.set_protocol_type(ProtocolType::Sixel);
        } else {
            // Forced sixel without support draws black boxes on VTE/Ptyxis.
            picker.set_protocol_type(ProtocolType::Halfblocks);
        }
        return;
    }

    if caps.contains(&Capability::Kitty) {
        picker.set_protocol_type(ProtocolType::Kitty);
    } else if caps.contains(&Capability::Sixel) {
        picker.set_protocol_type(ProtocolType::Sixel);
    }

    if let Some((r, g, b)) = caps.iter().find_map(|c| match c {
        Capability::Background(r, g, b) => Some((*r, *g, *b)),
        _ => None,
    }) {
        picker.set_background_color(Some(image::Rgba([r, g, b, 255])));
    }
}

fn build_help_picker() -> Picker {
    let mut picker = Picker::from_query_stdio_with_options(help_query_options())
        .unwrap_or_else(|_| Picker::halfblocks());
    apply_help_protocol_preference(&mut picker);
    picker
}

/// Pick the best available terminal graphics protocol for help images.
pub(crate) fn create_help_picker() -> Picker {
    build_help_picker()
}

pub(crate) fn help_uses_sixel(picker: &Picker) -> bool {
    picker.protocol_type() == ProtocolType::Sixel
}

/// Terminal background for occluding Sixel bleed-through in unpainted cells.
pub(crate) fn help_terminal_bg(picker: &Picker) -> Color {
    picker
        .capabilities()
        .iter()
        .find_map(|c| match c {
            Capability::Background(r, g, b) => Some(Color::Rgb(*r, *g, *b)),
            _ => None,
        })
        .unwrap_or(Color::Black)
}

/// Sixel is drawn below the cell grid; fill every non-image cell with an opaque background so
/// graphics do not show through line gaps, indents, or blank lines (Konsole/VTE).
pub(crate) fn paint_sixel_backdrop(
    frame: &mut ratatui::Frame,
    text_area: ratatui::layout::Rect,
    scroll: u16,
    placements: &[(usize, usize, u16, u16)],
    bg: Color,
) {
    let buf = frame.buffer_mut();
    for y in text_area.y..text_area.y.saturating_add(text_area.height) {
        let doc_row = scroll as usize + (y - text_area.y) as usize;
        for x in text_area.x..text_area.x.saturating_add(text_area.width) {
            let col = (x - text_area.x) as usize;
            if placements
                .iter()
                .any(|&(row_start, row_end, start_col, width)| {
                    doc_row >= row_start
                        && doc_row < row_end
                        && col >= start_col as usize
                        && col < start_col as usize + width as usize
                })
            {
                continue;
            }
            if let Some(cell) = buf.cell_mut((x, y)) {
                cell.set_bg(bg);
                if cell.symbol().is_empty() {
                    cell.set_symbol(" ");
                }
            }
        }
    }
}

/// Opaque row + ECH erase for the terminal line directly under a Sixel image (band spill).
pub(crate) fn seal_sixel_spill_row(
    frame: &mut ratatui::Frame,
    text_area: ratatui::layout::Rect,
    screen_y: u16,
    bg: Color,
) {
    if screen_y < text_area.y || screen_y >= text_area.y.saturating_add(text_area.height) {
        return;
    }
    let row = Rect::new(text_area.x, screen_y, text_area.width, 1);
    erase_terminal_cells(frame, row);
    let buf = frame.buffer_mut();
    for x in text_area.x..text_area.x.saturating_add(text_area.width) {
        if let Some(cell) = buf.cell_mut((x, screen_y)) {
            cell.set_symbol(" ");
            cell.set_bg(bg);
        }
    }
}

/// Screen Y of the terminal row immediately below an image's last reserved line.
pub(crate) fn screen_row_below_image(
    text_area: ratatui::layout::Rect,
    scroll: u16,
    document_row: usize,
    image_height: u16,
) -> Option<u16> {
    let rel = document_row as i32 + image_height as i32 - scroll as i32;
    let y = text_area.y as i32 + rel;
    if y < text_area.y as i32 || y >= text_area.y as i32 + text_area.height as i32 {
        None
    } else {
        Some(y as u16)
    }
}

/// Erase terminal cells before redraw (clears stale Sixel bands after scroll).
pub(crate) fn erase_terminal_cells(frame: &mut ratatui::Frame, area: ratatui::layout::Rect) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    const ESC: &str = "\x1b";
    let mut data = String::new();
    if area.height == 1 {
        use std::fmt::Write;
        let _ = write!(data, "{ESC}[{}X", area.width);
    } else {
        use std::fmt::Write;
        for _ in 0..area.height {
            let _ = write!(data, "{ESC}[{}X{ESC}[1B", area.width);
        }
        let _ = write!(data, "{ESC}[{}A", area.height);
    }
    let pos = ratatui::layout::Position::new(area.x, area.y);
    if let Some(cell) = frame.buffer_mut().cell_mut(pos) {
        cell.set_symbol(&data);
    }
    let mut skip_first = true;
    for y in area.y..area.y.saturating_add(area.height) {
        for x in area.x..area.x.saturating_add(area.width) {
            if skip_first {
                skip_first = false;
                continue;
            }
            if let Some(cell) = frame.buffer_mut().cell_mut((x, y)) {
                cell.set_skip(true);
            }
        }
        skip_first = false;
    }
}

/// Clamp reported terminal font metrics — some emulators return huge values that
/// collapse images to a tiny cell grid (heavy pixelation).
pub(crate) fn safe_font_size(picker: &Picker) -> (u16, u16) {
    let font = picker.font_size();
    let fw = if font.width == 0 {
        8
    } else {
        font.width.clamp(6, 12)
    };
    let fh = if font.height == 0 {
        16
    } else {
        font.height.clamp(10, 20)
    };
    (fw, fh)
}

pub(crate) fn row_pixel_height(rows: u16, font_h: u16, proto: ProtocolType) -> u32 {
    match proto {
        ProtocolType::Halfblocks => (rows as u32) * (font_h as u32) * 2,
        _ => (rows as u32) * (font_h as u32),
    }
}

fn height_divisor(font_h: u16, proto: ProtocolType) -> f64 {
    match proto {
        ProtocolType::Halfblocks => font_h as f64 * 2.0,
        _ => font_h as f64,
    }
}

fn pixel_to_cell(pw: u32, ph: u32, font_w: u16, font_h: u16, proto: ProtocolType) -> (u16, u16) {
    if pw == 0 || ph == 0 || font_w == 0 {
        return (0, 0);
    }
    let cw = (pw as f64 / font_w as f64).ceil() as u16;
    let ch = (ph as f64 / height_divisor(font_h, proto)).ceil() as u16;
    (cw.max(1), ch.max(1))
}

/// Fixed row targets per help asset (independent of terminal width after layout freeze).
fn help_target_rows(path: &str, proto: ProtocolType) -> u16 {
    let base = match path {
        "logo-protected.webp" | "logo.webp" => 12,
        "demo.webp" => 24,
        _ => 12,
    };
    match proto {
        // Sixel: one pixel row per terminal row — need extra rows for sharpness.
        ProtocolType::Sixel => ((f32::from(base) * 1.75).ceil() as u16).min(48),
        // Konsole halfblocks: extra rows for sharper ▀▄ rendering (2 px per cell row).
        ProtocolType::Halfblocks if is_konsole() => ((f32::from(base) * 1.5).ceil() as u16).min(40),
        _ => base,
    }
}

fn help_target_cell_dimensions(
    path: &str,
    img: &image::DynamicImage,
    font_w: u16,
    font_h: u16,
    proto: ProtocolType,
    layout_width: u16,
) -> (u16, u16) {
    let max_w = layout_width.min(HELP_IMAGE_MAX_WIDTH);
    let target_rows = help_target_rows(path, proto);
    let target_px_h = row_pixel_height(target_rows, font_h, proto);
    let scale_h = target_px_h as f64 / img.height().max(1) as f64;
    let nat_w_cells = (img.width() as f64 / font_w as f64).ceil() as u16;
    let max_w_for_img = match path {
        "logo-protected.webp" | "logo.webp" => 24,
        _ => max_w,
    };
    let scale_w = if nat_w_cells > max_w_for_img {
        max_w_for_img as f64 * font_w as f64 / img.width().max(1) as f64
    } else {
        1.0
    };
    let scale = scale_h.min(scale_w);
    let sw = ((img.width() as f64 * scale).ceil() as u32).max(1);
    let sh = ((img.height() as f64 * scale).ceil() as u32).max(1);
    pixel_to_cell(sw, sh, font_w, font_h, proto)
}

fn decode_image_bytes(bytes: &[u8]) -> Option<image::DynamicImage> {
    image::ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .ok()?
        .decode()
        .ok()
}

fn embedded_image_bytes(path: &str) -> Option<&'static [u8]> {
    match path {
        "logo-protected.webp" | "logo.webp" => {
            Some(include_bytes!("pages/dashboard/logo-protected.webp"))
        }
        "demo.webp" => Some(include_bytes!("pages/dashboard/demo.webp")),
        _ => None,
    }
}

impl ImageResolver for HelpImageResolver {
    fn resolve(&mut self, path: &str) -> Option<image::DynamicImage> {
        let full = self.base_dir.join(path);
        if let Ok(reader) = image::ImageReader::open(&full)
            && let Ok(decoded) = reader.with_guessed_format().ok()?.decode()
        {
            return Some(decoded);
        }
        embedded_image_bytes(path).and_then(decode_image_bytes)
    }

    fn cell_dimensions_for_path(
        &mut self,
        path: &str,
        img: &image::DynamicImage,
        _max_width: u16,
        max_height: u16,
    ) -> (u16, u16) {
        let (w, h) = help_target_cell_dimensions(
            path,
            img,
            self.font_w,
            self.font_h,
            self.protocol_type,
            self.layout_width,
        );
        (w, h.min(max_height))
    }

    fn cell_dimensions(
        &mut self,
        img: &image::DynamicImage,
        max_width: u16,
        max_height: u16,
    ) -> (u16, u16) {
        self.cell_dimensions_for_path("", img, max_width, max_height)
    }
}

/// Scale image pixels to match a prepared cell grid (for ratatui-image protocol encoding).
pub(crate) fn scale_image_to_cells(
    img: &image::DynamicImage,
    width_cells: u16,
    height_cells: u16,
    font_w: u16,
    font_h: u16,
    proto: ProtocolType,
) -> image::DynamicImage {
    let target_w = (width_cells as u32).saturating_mul(font_w as u32).max(1);
    let target_h = row_pixel_height(height_cells, font_h, proto).max(1);
    img.resize_exact(target_w, target_h, FilterType::Lanczos3)
}
