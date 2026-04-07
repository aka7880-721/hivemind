// Copyright © 2026 ComfyHome™
// All rights reserved.
//
// Licensed under the ComfyVersionBumper License v1.2
//
// For details, see the LICENSE file in the repository root.

use std::io::Cursor;

use image::{ImageReader, RgbaImage, imageops::FilterType};
use ratatui::{
    style::{Color, Style},
    text::{Line, Span},
};

pub const LOGO_TRANSPARENT_CELL: &str = " ";
pub const LOGO_HALF_BLOCK_TOP: &str = "▀";
pub const LOGO_HALF_BLOCK_BOTTOM: &str = "▄";
pub const LOGO_FULL_BLOCK: &str = "█";
pub const TERMINAL_IMAGE_ASPECT_ADJUSTMENT: f32 = 2.0;

pub const ASCII_HEADER: [&str; 4] = [
 r"▄█████  ▄▄▄  ▄▄   ▄▄ ▄▄▄▄▄ ▄▄ ▄▄ ██  ██ ▄▄▄▄▄ ▄▄▄▄   ▄▄▄▄ ▄▄  ▄▄▄  ▄▄  ▄▄ █████▄ ▄▄ ▄▄ ▄▄   ▄▄ ▄▄▄▄  ▄▄▄▄▄ ▄▄▄▄",
 r"██     ██▀██ ██▀▄▀██ ██▄▄  ▀███▀ ██▄▄██ ██▄▄  ██▄█▄ ███▄▄ ██ ██▀██ ███▄██ ██▄▄██ ██ ██ ██▀▄▀██ ██▄█▀ ██▄▄  ██▄█▄",
 r"▀█████ ▀███▀ ██   ██ ██      █    ▀██▀  ██▄▄▄ ██ ██ ▄▄██▀ ██ ▀███▀ ██ ▀██ ██▄▄█▀ ▀███▀ ██   ██ ██    ██▄▄▄ ██ ██",
 r"                                                                                                            {APP_VERSION}",
];

pub const NARROW_ASCII_HEADER: [&str; 8] = [
 r"                        ",
 r" ██████╗██╗   ██╗██████╗ ",
 r"██╔════╝██║   ██║██╔══██╗",
 r"██║     ██║   ██║██████╔╝",
 r"██║     ╚██╗ ██╔╝██╔══██╗",
 r"╚██████╗ ╚████╔╝ ██████╔╝",
 r" ╚═════╝  ╚═══╝  ╚═════╝ ",
 r"                      {APP_VERSION}",
 ];

const HEADER_LOGO_MARGIN: u16 = 4;
const HEADER_LOGO_GAP: u16 = 6;

#[derive(Clone)]
pub struct HeaderBanner {
    lines: Vec<Line<'static>>,
    width: u16,
}

#[derive(Clone)]
pub struct HeaderContent {
    banner: HeaderBanner,
    show_logo: bool,
    logo_margin: u16,
    logo_gap: u16,
}

#[derive(Clone)]
pub struct PixelLogo {
    source: Option<RgbaImage>,
}

#[derive(Clone)]
pub struct PixelLogoRender {
    lines: Vec<Line<'static>>,
    width: u16,
}

impl PixelLogo {
    pub fn load() -> Self {
        let primary = load_image(include_bytes!("../assets/logo-pix.webp"));
        let source = primary.or_else(|| load_image(include_bytes!("../assets/ico.png")));
        Self { source }
    }

    pub fn render(&self, max_height: u16) -> PixelLogoRender {
        let target_height = max_height.max(1) as u32;

        let Some(source) = &self.source else {
            return Self::fallback_render(target_height as u16);
        };

        let aspect_ratio = source.width() as f32 / source.height() as f32;
        let pixel_height = target_height * 2;
        let target_width = (aspect_ratio * target_height as f32 * TERMINAL_IMAGE_ASPECT_ADJUSTMENT)
            .round()
            .max(1.0) as u32;
        let resized = image::DynamicImage::ImageRgba8(source.clone())
            .resize_exact(target_width, pixel_height, FilterType::Nearest)
            .to_rgba8();
        render_image(&resized)
    }

    fn fallback_render(max_height: u16) -> PixelLogoRender {
        let base = [
            "      /\\      ",
            "     /  \\     ",
            "    / /\\ \\    ",
            "   / /  \\ \\   ",
            "  /_/____\\_\\  ",
            "  || [][] ||    ",
            "  ||      ||    ",
            "  ||______||    ",
        ];
        let line_count = max_height.max(1) as usize;
        let lines = base
            .into_iter()
            .take(line_count)
            .map(Line::from)
            .collect::<Vec<_>>();
        let width = lines.iter().map(|line| line.width() as u16).max().unwrap_or(0);
        PixelLogoRender { lines, width }
    }
}

impl PixelLogoRender {
    pub fn lines(&self) -> &[Line<'static>] {
        &self.lines
    }

    pub fn width(&self) -> u16 {
        self.width
    }
}

impl HeaderBanner {
    pub fn lines(&self) -> &[Line<'static>] {
        &self.lines
    }

    pub fn width(&self) -> u16 {
        self.width
    }
}

impl HeaderContent {
    pub fn banner(&self) -> &HeaderBanner {
        &self.banner
    }

    pub fn show_logo(&self) -> bool {
        self.show_logo
    }

    pub fn logo_margin(&self) -> u16 {
        self.logo_margin
    }

    pub fn logo_gap(&self) -> u16 {
        self.logo_gap
    }
}

pub fn choose_header_content(inner_width: u16, logo_width: u16, version_label: &str) -> HeaderContent {
    let wide_banner = build_header_banner(&ASCII_HEADER, version_label);
    let narrow_banner = build_header_banner(&NARROW_ASCII_HEADER, version_label);
    let wide_with_logo_width = HEADER_LOGO_MARGIN + logo_width + HEADER_LOGO_GAP + wide_banner.width();
    let narrow_with_logo_width = HEADER_LOGO_MARGIN + logo_width + HEADER_LOGO_GAP + narrow_banner.width();

    if inner_width >= wide_with_logo_width {
        HeaderContent {
            banner: wide_banner,
            show_logo: true,
            logo_margin: HEADER_LOGO_MARGIN,
            logo_gap: HEADER_LOGO_GAP,
        }
    } else if inner_width >= wide_banner.width() {
        HeaderContent {
            banner: wide_banner,
            show_logo: false,
            logo_margin: 0,
            logo_gap: 0,
        }
    } else if inner_width >= narrow_with_logo_width {
        HeaderContent {
            banner: narrow_banner,
            show_logo: true,
            logo_margin: HEADER_LOGO_MARGIN,
            logo_gap: HEADER_LOGO_GAP,
        }
    } else {
        HeaderContent {
            banner: narrow_banner,
            show_logo: false,
            logo_margin: 0,
            logo_gap: 0,
        }
    }
}

fn build_header_banner(lines: &[&str], version_label: &str) -> HeaderBanner {
    let rendered = lines
        .iter()
        .map(|line| {
            if let Some(index) = line.find("{APP_VERSION}") {
                let prefix = &line[..index];
                let suffix = &line[index + "{APP_VERSION}".len()..];
                let spans = vec![
                    Span::styled(prefix.to_string(), Style::default().fg(Color::White).bold()),
                    Span::styled(version_label.to_string(), Style::default().fg(Color::Cyan).bold()),
                    Span::styled(suffix.to_string(), Style::default().fg(Color::White).bold()),
                ];
                Line::from(spans)
            } else {
                Line::from(Span::styled(
                    (*line).to_string(),
                    Style::default().fg(Color::White).bold(),
                ))
            }
        })
        .collect::<Vec<_>>();
    let width = rendered.iter().map(|line| line.width() as u16).max().unwrap_or(0);

    HeaderBanner {
        lines: rendered,
        width,
    }
}

fn render_image(image: &RgbaImage) -> PixelLogoRender {
    let mut lines = Vec::new();
    let mut width = 0_u16;

    let mut y = 0;
    while y < image.height() {
        let mut spans = Vec::new();
        let mut current: Option<(Style, String)> = None;

        for x in 0..image.width() {
            let top = image.get_pixel(x, y).0;
            let bottom = if y + 1 < image.height() {
                image.get_pixel(x, y + 1).0
            } else {
                [0, 0, 0, 0]
            };
            let (style, cell) = render_cell(top, bottom);

            match &mut current {
                Some((existing_style, text)) if *existing_style == style => text.push_str(cell),
                Some((existing_style, text)) => {
                    let span = Span::styled(std::mem::take(text), *existing_style);
                    spans.push(span);
                    *existing_style = style;
                    text.push_str(cell);
                }
                None => current = Some((style, cell.to_string())),
            }
        }

        if let Some((style, text)) = current.take() {
            spans.push(Span::styled(text, style));
        }

        let line = Line::from(spans);
        width = width.max(line.width() as u16);
        lines.push(line);
        y += 2;
    }

    PixelLogoRender { lines, width }
}

fn load_image(bytes: &[u8]) -> Option<RgbaImage> {
    let decoded = ImageReader::new(Cursor::new(bytes)).with_guessed_format().ok()?;
    let image = decoded.decode().ok()?;
    Some(image.to_rgba8())
}

fn render_cell(top: [u8; 4], bottom: [u8; 4]) -> (Style, &'static str) {
    let top_color = rgba_to_color(top);
    let bottom_color = rgba_to_color(bottom);

    match (top_color, bottom_color) {
        (None, None) => (Style::default(), LOGO_TRANSPARENT_CELL),
        (Some(top), None) => (Style::default().fg(top), LOGO_HALF_BLOCK_TOP),
        (None, Some(bottom)) => (Style::default().fg(bottom), LOGO_HALF_BLOCK_BOTTOM),
        (Some(top), Some(bottom)) if top == bottom => (Style::default().fg(top), LOGO_FULL_BLOCK),
        (Some(top), Some(bottom)) => (Style::default().fg(top).bg(bottom), LOGO_HALF_BLOCK_TOP),
    }
}

fn rgba_to_color(pixel: [u8; 4]) -> Option<Color> {
    if pixel[3] < 30 {
        None
    } else {
        Some(Color::Rgb(pixel[0], pixel[1], pixel[2]))
    }
}