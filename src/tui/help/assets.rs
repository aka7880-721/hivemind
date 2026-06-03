// Copyright © 2026 ComfyHome™
// All rights reserved.
//
// Licensed under the ComfyGit SA-PS License
// For details, see the LICENSE file in the repository root.

use std::io::Cursor;
use std::path::PathBuf;

use comfy_txt_engine::markdown::ImageResolver;
use image::imageops::FilterType;
use ratatui_image::picker::{Capability, Picker, ProtocolType};

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

/// Pick the best available terminal graphics protocol for help images.
pub(crate) fn create_help_picker() -> Picker {
    match Picker::from_query_stdio() {
        Ok(mut picker) => {
            let caps = picker.capabilities();
            if caps.contains(&Capability::Kitty) && picker.protocol_type() != ProtocolType::Kitty {
                picker.set_protocol_type(ProtocolType::Kitty);
            }
            picker
        }
        Err(_) => Picker::halfblocks(),
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
fn help_target_rows(path: &str) -> u16 {
    match path {
        "logo-protected.webp" | "logo.webp" => 12,
        "demo.webp" => 24,
        _ => 12,
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
    let target_rows = help_target_rows(path);
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
