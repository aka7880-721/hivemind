# Comfy Version Bumper Developer Notes

This document describes the current architecture, active limitations, and the next recommended implementation steps for CVB.

## Current Architecture

The app is a standalone Rust binary crate that uses Ratatui and Crossterm for the terminal UI. Configuration is stored per-user outside the repository. ~~The current implementation is organized around a single application state struct with modal dialogs layered on top of the main screen.~~ The current implementation still uses a central application state struct, but UI helpers, dialog state, git helpers, and target parsing/writing are now split into dedicated modules to reduce `src/app.rs` sprawl.

Key modules:

- ~~`src/app.rs`: UI state machine, rendering, input handling, git actions, file updates~~
- `src/app.rs`: main app orchestration, screen rendering, event routing, and project editing flows
- `src/ui.rs`: shared layout helpers such as centered rectangles and vertical centering
- `src/dialogs.rs`: modal state and shared text input primitives
- `src/git.rs`: git and `gh` command helpers plus repo-related utility functions
- `src/targets.rs`: JSON/TOML probe, read, and write logic for version targets
- `src/config.rs`: config persistence and project model
- `src/versioning.rs`: version scheme validation and bump behavior
- `src/branding.rs`: logo loading, terminal rendering, and wide/narrow ASCII header fallback selection

## Implemented Features

- onboarding wizard for new projects
- saved project configuration
- dashboard project list and project detail
- version bump preview and apply for JSON and TOML targets
- grouped branched bump preview with project-wide vs per-scope semantics and mixed-version mismatch surfacing
- recent git changes modal
- scope-aware recent changes and tag/release dialogs for branched projects, including per-scope repo selection and scoped tag suggestions
- tabbed view-changes modal with recent/history ranges and colored git graph output
- dashboard overview page with `Overview` and `Project Detail` tabs
- per-scope overview tiles for branched projects with inline version adjustments, click targets for view/bump/tag, and drag reorder
- embedded recent-changes panel on the overview page with scope switching that stays available for unified branched projects
- overflow scrolling for tall wizard/edit forms and multi-row overview tiles
- local tag creation
- optional multi-line tag annotations through a dedicated annotation editor
- tag push and `gh`-based release creation for GitHub-enabled projects
- post-bump tag flow that defaults to push-capable actions for git-backed projects
- settings project amendment for repo roots, remotes, and target paths
- settings project amendment for the same core fields exposed by the new-project flow
- branched scope editing with add, remove, rename, reorder, and per-scope target path/key fields
- target-key presets for JSON and TOML targets with explicit custom-mode fallback in both New Project and Edit Project
- scroll-aware New Project and Edit Project forms with PgUp/PgDn, mouse-wheel support, and clipped-content indicators in short terminals
- project removal from the Edit Project dialog
- in-app file browser for target-path and repo-root selection
- tabbed main navigation plus a dedicated UI Settings screen for tab-hint visibility
- wrapped toast notifications with shared default placement and sticky error interactions
- responsive header fallbacks: `logo + wide ASCII`, `wide ASCII only`, `logo + narrow ASCII`, `narrow ASCII only`
- local install and Windows release packaging documentation
- release build script that can emit a zip and MSI

## Current Interaction Model

- keyboard-first interaction
- mouse click support for most modal buttons and project lists
- mouse wheel support for project scrolling, browser navigation, recent-changes scrolling, tall form dialogs, and the dashboard overview viewports
- drag-and-drop tile reordering inside the dashboard overview page
- Enter-driven folder navigation in the browser, with `U` to select a folder explicitly for repo-root flows

## Known Gaps

- No in-app changelog file generation yet.
- Branched projects still only expose one target per scope, with no UI yet for multiple targets inside a single scope.
- No UI yet for per-scope repo overrides or custom display labels even though the git flows are now scope-aware.
- Branched bumping still lacks a dedicated strategy toggle in the editor UI; the preview and git flows now respect `unified_versioning`, but config editing for that strategy is still pending.
- No dedicated release-notes editor before `gh release create`.
- No persistent app theme settings beyond tab-hint visibility and the basic config scaffold.

## Known Bugs Or Risk Areas

- Terminal rendering of the logo still depends on terminal glyph behavior and cell aspect ratio. Different terminals may render the half-block logo slightly differently.
- The email placed on the top border is drawn as overlay text; very narrow terminals may not have enough room for it.
- Git action error reporting is command-output based and still fairly raw.
- JSON write-back currently normalizes formatting through `serde_json::to_string_pretty`, which may not preserve the original formatting style.
- TOML write-back uses `toml_edit`, which preserves structure better than a full parse/rewrite, but edge-case formatting may still shift.
- The MSI packaging script has not been exercised against every Windows environment and currently depends on WiX Toolset v4 being installed locally.

## Recommendations For Next Work

1. ~~Add project editing for target key paths and per-branch scheme configuration, not just repo roots and target file paths.~~ Core edit coverage now matches the current New Project flow for the first target/branch, but full per-branch and multi-target editing is still missing.
2. Add keyboard shortcuts for overview tile-row scrolling so overflowed branched dashboards are navigable without the mouse.
3. Add explicit changelog generation and preview before GitHub release creation.
4. Add release guards that check `gh auth status` before showing release actions.
5. Add support for multiple targets with intentionally different version values inside a branched project flow.
6. ~~Extract git operations into a separate module so `app.rs` stops growing.~~ `src/git.rs` now owns git and GitHub CLI helpers; `app.rs` still needs more reduction on the rendering side.
7. Extract modal state and field editing patterns into reusable components.
8. Add integration tests around git workflows using temporary repositories.
9. Add focused UI tests around overview tab switching, tile scrolling, and branched scope selection.

## Branched Project Type Implementation Plan

Goal: support projects with multiple modules or services, each with its own version targets and optional branch-specific release flow, without regressing the current all-in-one path.

1. ~~Expand the config model.~~
	Add explicit branch-or-module collections that can hold multiple targets, per-branch labels, and future per-branch repo overrides without overloading the single-target scaffold now used for branched projects.

2. ~~Introduce a real branched editor flow.~~
	The wizard and project edit dialog now support add, remove, rename, and reorder controls for branched scopes plus per-scope target-path and target-key editing. Dedicated UI for custom display labels and per-scope repo overrides is still pending.

3. ~~Split target discovery from target editing.~~
	Bump and tag flows now resolve grouped branch targets through shared scope helpers instead of flattening ad hoc target lists. Wizard and edit validation still use local scope drafts and can be reduced further later.

4. ~~Define bump semantics per branch.~~
	`unified_versioning = true` now means one synchronized project-wide bump and requires a shared resolved version across scopes before apply. `unified_versioning = false` now means isolated per-scope bumps, with the preview selecting one scope at a time and surfacing mismatches before writing any files.

5. ~~Add branch-aware preview and apply flows.~~
	The bump modal now renders grouped scopes, highlights mixed-version states, and applies either a project-wide synchronized bump or a selected-scope bump based on `unified_versioning`.

6. ~~Make git flows branch-aware.~~
	View Changes, tag suggestions, overview recent-changes scope selection, and tag/release actions now resolve the active branched scope, including branch-level repo overrides when present, instead of assuming one project-wide git context.

7. ~~Harden persistence and migration.~~
	Config loading now migrates older branched entries into schema version `2`, promoting legacy top-level targets into a default scope when needed and normalizing missing branch labels during load/save.

8. ~~Add targeted tests before widening the UI further.~~
	Added migration coverage for legacy branched configs and a viewport test that keeps wizard focus visible in short terminals while the new scroll-aware form rendering avoids vertical row collapse.

## Logo Rendering Notes

The current header logo renderer uses the provided `assets/logo-pix.webp` as the source artwork and renders it into terminal cells using half-block characters. It does not intentionally add a second coarse pixelation layer.

Header fallback order now follows this sequence:

- `logo + wide ASCII header`
- `wide ASCII header only`
- `logo + narrow ASCII header`
- `narrow ASCII header only`

Relevant tuning point:

- `TERMINAL_IMAGE_ASPECT_ADJUSTMENT` in `src/branding.rs`

This is only an aspect compensation factor for terminal cells. It should not be treated as an artistic pixelation control.

## Suggested Refactors

- ~~Split `src/app.rs` into `ui`, `dialogs`, `git`, and `targets` modules.~~ Done for the first pass; additional rendering extraction is still worthwhile.
- Introduce typed target handlers for JSON and TOML instead of keeping read/write helpers inline in the app module.
- Add a dedicated settings model for UI preferences and future theming.

## Suggested Release Hardening

- Add explicit `git status --porcelain` checks before bump and tag actions.
- Add confirmation before pushing tags.
- Add confirmation before creating releases.
- Validate remote configuration and default branch assumptions before push/release actions.
- Consider code signing for the Windows MSI and release binary.
- Add CI automation to run the release script and publish artifacts.

## Release Packaging Notes

The current release script lives at `scripts/build-release.ps1`.

Current behavior:

- builds `target/release/cvb.exe`
- creates `dist/cvb-<version>-windows-x64.zip`
- creates `dist/cvb-<version>-windows-x64.msi` when WiX v4 is installed

The MSI currently installs:

- `cvb.exe`
- `README.md`
- `LICENSE`

It also appends the install directory to the system `PATH` so `cvb` works from fresh terminal sessions after install.

## Contact

Engineering contact: `dev@comfyhome.io`
