// Copyright © 2026 ComfyHome™
// All rights reserved.
//
// Licensed under the ComfyGit SA-PS License
// For details, see the LICENSE file in the repository root.

use std::path::PathBuf;

use comfy_txt_engine::markdown::ImageResolver;

#[derive(Clone)]
pub(crate) struct HelpImageResolver {
    base_dir: PathBuf,
}

impl HelpImageResolver {
    pub(crate) fn new(relative_page_dir: &str) -> Self {
        let base_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("src/tui/help")
            .join(relative_page_dir);
        Self { base_dir }
    }
}

impl ImageResolver for HelpImageResolver {
    fn resolve(&mut self, path: &str) -> Option<image::DynamicImage> {
        let full = self.base_dir.join(path);
        image::ImageReader::open(&full).ok()?.decode().ok()
    }
}
