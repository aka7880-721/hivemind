## Changelog 0.6.1
2026-04-13

#### What's changed:

### Changed in Tiles

#### 🐛 Fix(es)

* fixes visual indication bug for the focused tile when changed via mouse event in Branched project   _(56dcce1)_

---

### General Improvements:

### 🐛 Fix(es)

* fixes ordering bug in gitlog history   _(9e98e17)_

* update licensing statement in COPYRIGHT file to remove version reference   _(e2fa12d)_

### 💎 Enhancements

* implement tag sorting functionality for history   _(9e98e17)_

* add tests for semantic versioning order   _(9e98e17)_

* improve changelog rendering to include general improvements after specific sections   _(f995eaf)_

* adds `---` separator after Specific section   _(f995eaf)_

* G/C shorcut handling in Branched projects   _(56dcce1)_

* update changelog preview dialog for multi-line release notes and improve layout constraints   _(e271165)_

* improve changelog preview functionality with tui-markdown   _(b17078e)_

* Release Notes with multi-line markdown support   _(b17078e)_

* add new dependencies to Cargo.lock and update Cargo.toml for tui-markdown   _(ea01edb)_

* update changelog for 0.6.0 with major performance refactor details   _(bd6cb68)_

<br>

---
... ✨ made with [CVB](https://github.com/comfy-home/perk-ComfyVersionBumper)

## Changelog 0.6.0
2026-04-13

**What Wa Fixed Acro Phae 1-5**

#### What's changed:

### ✨ New Feature: Loading Modals

#### 🧩 Features

* add progress dialog rendering for improved user feedback during operations   _(13deb18)_

* Sentence definitions   _(b204e50)_

### 🧩 Features

* add functions to get current branch and switch to main branch with optional remote sync   _(c93979b)_

* enhance repo branch management with new state collection and switching functions   _(88cdbcd)_

* add footer_content to UiSettings and implement FooterContent enum   _(79e223a)_

### 🐛 Fix(es)

* update project summary color to darker shade of grey   _(8a32713)_

* update default focus field in ProjectWizard and reorder visible fields   _(c7b3543)_

### 🔧 Maintenance

* update libc to version 0.2.185 and update checksum   _(cc644fe)_

* CVB version bump to 0.4.0   _(12534f5)_

* remove outdated developer notes from README   _(c8fc5a5)_

* CVB version bump to 0.3.10   _(d4cb026)_

### 💎 Enhancements

* add support for overview activity job management and background job checks   _(e2220d0)_

* enhance overview bump workflow with cancellation support and improved staged files check   _(8f233d1)_

* add cancellation support to non-main repo state collection and staged paths functions   _(98fca6d)_

### ℹ️ Documentation

* add documentation for Git-related utilities   _(a2bfda6)_

### ♻️ Refactor

* enhance current branch function to support cancellation   _(73a495a)_

* enhance tile rendering with improved styling support for semver and calver tiles   _(baa9eb6)_

* add cancellation support for recent changes and changelog preview operations   _(54ececd)_

* add cancellation support to changelog preview dialog and entry collection functions   _(07a96da)_

* add cancellation support to recent changes and history loading functions   _(db25844)_

* enhance git command handling with cancellation support and improved async execution   _(e73a689)_

* tokio async enhance background job handling with structured messaging and improved concurrency   _(4fcf7d5)_

* remove unused overview activity cache functions and update changelog preview functions for async handling   _(e5936fc)_

* update tokio dependency to include macros and time features   _(9373a32)_

* update background job handling to use Tokio for improved concurrency   _(fb271d0)_

* add tokio dependency for improved asynchronous handling   _(9a5586a)_

* extracted reusable preview builders here so changelog jobs can run off the UI thread   _(5b5249e)_

* implement background job handling for UI tasks and improve progress management   _(5c7fb10)_

* add changelog field label and add test for branched projects   _(06458d6)_

* remove ApplyOverviewVersionAndTag action for streamlined hit action handling   _(4804043)_

* update border style colors for improved visibility in selected and unselected states   _(32a6313)_

* update hit action for tag rectangle to open overview tag dialog   _(783e4a2)_

* simplify hit action resolution logic and add open overview tag dialog action   _(cb11f75)_

* replace open_overview_changelog_preview_if_enabled with should_open_overview_changelog_preview for improved clarity and functionality   _(5166a98)_

* implement pending UI job handling for improved responsiveness and user feedback   _(b204e50)_

* remove redundant toaster tick calls in draw method for cleaner logic   _(7aceeda)_

* optimize UI polling and drawing logic for improved responsiveness   _(4ce7c1e)_

* update recent changes dialog shortcuts to include reload functionality   _(33a2046)_

* streamline overview cache management and enhance activity summary loading   _(6f32ef8)_

* enhance dashboard project data handling and add reload functionality   _(042b2cf)_

* update navigation instructions to include refresh functionality for current scope   _(d66a794)_

* dashboard recent-changes refresh is no longer triggered on steady-state redraws   _(b541eb2)_

* simplify ensure_dashboard_recent_changes function by removing redundant error handling   _(b541eb2)_

* enhance RecentChangesDialog to manage history loading and state   _(f314cbf)_

* improve recent changes dialog handling and add refresh functionality   _(507dbeb)_

* pass 2: removed dead code leftovers from 0.4.0   _(6b93d37)_

* fixing bad merge mistake after commit ammend, pass 1   _(b5c382d)_

* navigation and new footer settings   _(8580efd)_

### 🗑️ Removed

* remove unused run_gh_checked function to clean up code   _(1fc73ae)_

* open_overview_changelog_preview_if_enabled   _(5166a98)_

### 📝 Other

* Merge pull request #9 from comfy-home/v0.6.0   _(7f40e56)_

* comfy-home/perk-ComfyVersionBumper   _(b3dedbf)_

* Merge pull request #8 from comfy-home/v0.5.0   _(66fdebd)_

* extracted rendering to render.rs, approx 1000LoC less   _(c9c4417)_
* Impl: are we on main? fn   _(c9c4417)_

* source app.rs → here   _(01fa83b)_
* rendering related code   _(01fa83b)_

* add render file   _(476efd1)_

* extracted git and overview stuff to separate files, ~900 lines   _(8580efd)_

* source: app.rs → here   _(10236af)_
* dashboard overview related stuff   _(10236af)_

* source: app.rs → here   _(a87f627)_
* git related operations   _(a87f627)_

* add overview.rs   _(95cadbb)_

* add git_flow.rs   _(2e41c0d)_

<br>

---
... ✨ made with [CVB](https://github.com/comfy-home/perk-ComfyVersionBumper)

## Changelog 0.5.0
2026-04-12

Tet

#### What's changed:

### ✨ New in Tiles:

#### 🧩 Features

* add reset functionality for pending version   _(82d2727)_

* add reset_rect to OverviewTileHotspots for semver and calver tiles. Just click on `ver.`   _(253a7c9)_

### ✨ New Enhancement: Changelog Preview

#### 💎 Enhancements

* implement changelog preview handling   _(82d2727)_

### ✨ New:

#### 🧩 Features

* implement changelog categories and parsing logic   _(12d41bd)_

### 🧩 Features

* enhance changelog preview dialog with save functionality and updated instructions   _(024d698)_

* integrate changelog archiving into overview bump workflow   _(6196622)_

* enhance changelog parsing and rendering   _(8982abc)_

* add support for multiple specific headings and temporary changelog writing   _(8982abc)_

* add save functionality for changelog preview to root as changelog_temp.md   _(bf0991c)_

* add archive changelog on tag creation to `.changelogs` folder   _(bf0991c)_

* update key bindings for recent changes and changelog preview   _(b458d95)_
* enhance dialog handling & wiring in the new stuff   _(b458d95)_

* update dialog shortcuts and header for changelog preview functionality   _(35f3541)_

* implement functions to build and write changelog markdown from git log   _(e96279a)_

* add changelog preview functionality and related dialog handling   _(4294afc)_

* add function to load recent change range from Git scope   _(9a6744e)_

* add functionality to append stage paths for repo bump operations   _(5840154)_

* add changelog preview functionality to overview bump workflow   _(efde529)_

* implement changelog preview dialog rendering functionality   _(4eb290c)_

* add changelog settings to ProjectWizard and implement related functionality   _(174f199)_

* add changelog settings to ProjectEditDialog and update apply logic   _(45add08)_

* add default changelog settings to test configurations   _(4b84e14)_

* add changelog settings and update schema version   _(71f8dde)_

* enable changelog in ProjectWizard and update focus field   _(9a34886)_

### 🐛 Fix(es)

* update commit bullet formatting in changelog rendering   _(ec97af8)_

* update test identifier format in README   _(2e54b57)_

* update test identifier test format in README   _(d97f2f1)_

* update test identifier format in README   _(3b88b7c)_

* update visible fields to include changelog options before repo fields   _(e692677)_

* correct enhancement category label in changelog   _(f8c8024)_

### 🔧 Maintenance

* add changelog module to main.rs   _(310a119)_

* add changelog file with copyright and licensing information   _(d67b696)_

### ℹ️ Documentation

* update README with test identifier   _(5180697)_

<br>

---
... ✨ made with [CVB](https://github.com/comfy-home/perk-ComfyVersionBumper)