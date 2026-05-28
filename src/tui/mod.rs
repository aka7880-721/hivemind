// Copyright © 2026 ComfyHome™
// All rights reserved.
//
// Licensed under the ComfyGit License v1.2

mod branding;
mod overview_tabs;
mod project_edit;
mod project_wizard;
mod snif_modal;
mod tiles;
mod ui;

pub(crate) use branding::{PixelLogo, choose_header_content};
pub(crate) use overview_tabs::{
    OverviewTab, overview_tab_rects, overview_tabs, render_overview_tabs,
};
pub(crate) use project_edit::{ProjectEditDialog, ProjectEditFocus};
pub(crate) use project_wizard::{ProjectWizard, WizardField};
pub(crate) use snif_modal::SnifModal;
pub(crate) use tiles::{OverviewTileData, TILE_WIDTH, render_overview_tile, tile_height};
pub(crate) use ui::{center_vertically, centered_rect};
