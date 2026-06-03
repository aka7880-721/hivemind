// Copyright © 2026 ComfyHome™
// All rights reserved.
//
// Licensed under the ComfyGit SA-PS License
// For details, see the LICENSE file in the repository root.

use std::path::PathBuf;

use comfy_txt_engine::markdown::ImageResolver;
use ratatui_image::picker::{Capability, Picker, ProtocolType};

#[derive(Clone)]
pub(crate) struct HelpImageResolver {
    base_dir: PathBuf,
    font_w: u16,
    font_h: u16,
    protocol_type: ProtocolType,
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

fn safe_font_size(picker: &Picker) -> (u16, u16) {
    let font = picker.font_size();
    if font.width == 0 || font.height == 0 {
        (8, 16)
    } else {
        (font.width, font.height)
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

impl ImageResolver for HelpImageResolver {
    fn resolve(&mut self, path: &str) -> Option<image::DynamicImage> {
        let full = self.base_dir.join(path);
        image::ImageReader::open(&full).ok()?.decode().ok()
    }

    fn cell_dimensions(
        &mut self,
        img: &image::DynamicImage,
        max_width: u16,
        max_height: u16,
    ) -> (u16, u16) {
        let (mut cw, mut ch) = pixel_to_cell(
            img.width(),
            img.height(),
            self.font_w,
            self.font_h,
            self.protocol_type,
        );
        let w = cw.min(max_width);
        if w < cw {
            let ratio = img.height() as f64 * w as f64 / (img.width() as f64).max(1.0);
            ch = (ratio / height_divisor(self.font_h, self.protocol_type)).ceil() as u16;
            cw = w;
        }
        let ch = ch.min(max_height);
        (cw.max(1), ch.max(1))
    }
}
