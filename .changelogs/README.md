# Changelog History

Newest archived changelogs first. When multiple archived files represent the same version, only the newest archive is included here.

## Changelog v0.10.7
2026-04-19

#### What's changed:

### ✨ New Enhancement: CLI & bmp: Branch b4 bmp, bmp CLI options

* Add overview branch bump dialog to enhance bump workflow   _(d5444ae)_

* Enhance overview bump workflow to support branch creation and management   _(8c22cc3)_

* Implement create_branch_and_switch function for improved branch management   _(ccc54d7)_

* Update repo bump workflow to support branch creation and switching   _(8304d4d)_

* Enhance bump command with options for commit and push workflows   _(7d85908)_

* Add overview branch bump dialog and related functionality   _(950538d)_

---

### Changed in cli

#### 💎 Enhancements

* improve usage instructions for bump command and add action synonyms   _(4e22f52)_

* add synonyms for bump actions and improve usage instructions   _(889bbfd)_

* enhance bump command recognition with synonyms and update usage instructions   _(7e20eda)_

* add support for 'cg pwd -all' command to print all configured repo root directories   _(b9e32ac)_

---

### Changed in Changelogs

#### 🐛 Fix(es)

* bugfix for special character parsing within Specific wrap   _(78d967c)_

---

### General Improvements:

### ✨ New:

#### 🧩 Features

* Implement CLI functionality   _(8ea9c57)_

### 🧩 Features

* Add shell integration scripts for ComfyGit   _(e34b27a)_

* Add shell integration scripts for ComfyGit installation   _(4fc7c98)_

* `cg cd` tests   _(755f48f)_

* Add new binary target 'cg-bin' and restructure main entry point   _(755f48f)_

* Change command from 'cd' to 'pwd' for printing project root path in CLI   _(9f56f30)_

* Initialize project alias as an empty string in ProjectConfig for tests   _(f4ccebe)_

* Add alias field to ProjectSettingsState and update related functionality   _(8646c52)_

* Initialize project alias as an empty string in additional test cases   _(be8a502)_

* Implement CLI dispatch in main function for startup mode handling   _(e97d853)_

* Initialize project alias as an empty string in test cases   _(517bec4)_

* Add alias field to ProjectConfig with default value   _(02e5464)_

* Initialize project alias as an empty string in test configurations   _(2f3e89d)_

### 🐛 Fix(es)

* Simplify sorting logic in find_project_for_cwd and find_scope_for_cwd functions   _(ff107c1)_

### 🔧 Maintenance

* CG version bump to 0.10.7   _(0e1931c)_

* Bump ComfyGit version to 0.10.6 in Cargo.toml and Cargo.lock   _(93cf3bd)_

* CG version bump to 0.10.4   _(77b36fe)_

* CG version bump to 0.10.3   _(b8c69ef)_

* CG version bump to 0.10.2   _(8517e77)_

* CG version bump to 0.10.1   _(d129d39)_

* CG version bump to 0.10.0   _(c96691e)_

* CG version bump to 0.9.2   _(9db8204)_

* Rust 1.94 → 1.95   _(cd6e3a4)_

### ♻️ Refactor

* Update references from cg-bin to ComfyGit in shell scripts   _(4664848)_

### 🚀 Performance

* : Further code optizations   _(bdf4828)_

* Refactor code for improved readability and performance   _(93d64ef)_

* Various performance updates   _(cd6e3a4)_

### 📝 Other

* Merge pull request #22 from comfy-home/0.10.0   _(019e13f)_

* Merge pull request #21 from comfy-home/0.10.7   _(85806d2)_

* Merge pull request #20 from comfy-home/0.10.1   _(5897cea)_

* Merge pull request #19 from comfy-home/0.9.2   _(c1bd621)_

---

## Changelog v0.9.1
2026-04-17

#### What's changed:

### ✨ New in Changelogs:

#### 🧩 Features

* Add support for ignoring specific commits in changelog generation:   _(0450b8b)_
  * this is done simple by adding `~` as subject
  * form 1→ `~: this is my commit note that won't be in the changelog`
  * form 2→ `~{category}: you can just add tilde in front of any category, eg feat, docs, enh, fix, etc`
  * form 3→ `fix: this bugfix part is in the changelog<semicolon> ~: this note, or extended description is not`
  * Enjoy!

---

### General Improvements:

### 🔧 Maintenance

* CG version bump to 0.9.1   _(fe1e31c)_

### 💎 Enhancements

* Update commit message format for release notifications to include tilde prefix so it does not spam changelogs   _(5fccf34)_

### 📝 Other

* Merge pull request #18 from comfy-home/0.9.1   _(dcf71ee)_

* v0.9.0 has just been released via ComfyGit!   _(48ccf8b)_

---

## Changelog v0.9.0
2026-04-17

#### What's changed:

### Changed in Branding

#### 🎨 Visuals

* update narrow ASCII header   _(5b95b61)_

* adjust header height for compact viewports   _(56ac09c)_

* version marker repositioning   _(4acc52a)_

* simplify fallback   _(cb9458b)_

* update base ASCII header   _(d5dbdaf)_

#### 🗑️ Removed

* fallback graphical logo   _(cb9458b)_

---

### General Improvements:

### 🧩 Features

* add footer visibility management to App struct   _(511f95c)_

### 🐛 Fix(es)

* Correct button action   _(6157a49)_

### 🔧 Maintenance

* Update changelog for version 0.8.3 and syncmem entries   _(a4150bf)_

* CVB version bump to 0.9.0   _(ca35229)_

### ℹ️ Documentation

* rename project from ComfyVersionBumper to ComfyGit and update related documentation   _(050afc3)_

### 🎨 Visuals

* update button styles and positions in semver tile rendering   _(a810b55)_

* further adj to compact header   _(c921821)_

* NEW→ Compact header for 22row or less (eg useful for VSCode terminal bottom display)   _(30375b6)_

* footer auto-hide below 25 rows viewport availability   _(511f95c)_

### 🗑️ Removed

* remove title from overview page block   _(055d5bf)_

### 📝 Other

* Merge pull request #17 from comfy-home/0.9.0   _(f1940d3)_

* Merge branch 'main' into 0.9.0   _(0f7c5ac)_

---

## Changelog v0.8.3
2026-04-17

#### What's changed:

### ✨ New Feature: Text Select in input fields

* Implement clipboard copy and paste functionality for selected text using `rightClick` action   _(723c869)_

* Refactor display value methods to return Line type for improved rendering   _(9403a75)_

* Enhance text input handling with click target tracking and selection improvements   _(77bb7ac)_

* Enhance TextInput with selection handling and cursor movement improvements   _(b8e8bdd)_

---

### General Improvements:

### 🧩 Features

* ReleaseNOW! auto-push - implement staging and committing of generated ReleaseNOW files   _(6843d3e)_

* auto-gitignore functionality to ensure .gitignore entry for local changelog syncmem file   _(62e3b68)_

### 🐛 Fix(es)

* Resolved shortcut vs input text focus bug in wizard→enhance quit and shortcut handling in App to respect text input focus states   _(b71da2a)_

### 🔧 Maintenance

* CVB version bump to 0.8.3   _(0a6652c)_

### 📝 Other

* Merge pull request #16 from comfy-home/0.8.3   _(1d9214b)_

* add changelog for version 0.8.2 and standard changelog execution   _(08e13e3)_

<br>

---
... ✨ made with [CVB](https://github.com/comfy-home/perk-ComfyVersionBumper)

---

## Changelog v0.8.2
2026-04-16

#### What's changed:

### Changed in release

#### 🧩 Features

* enhance ReleaseNowDialog and validation with changelog support and scope context - should fix std and sum ignore during rls   _(93bb74a)_

#### 🐛 Fix(es)

* add remote specification for GitHub release creation and push - fixes build error   _(f34eebb)_

* fixes empty CL in ReleaseNOW!   _(81e1b63)_

---

### Changed in changelog

#### 🧩 Features

* add function to rebuild history summary README from changelog history - should fix std and sum ignore during rls   _(8c422a9)_

#### 🐛 Fix(es)

* implement standard changelog execution with improved decision logic - should fix std and sum ignore during rls   _(da8588c)_

---

### General Improvements:

### 🔧 Maintenance

* CVB version bump to 0.8.2   _(90d7063)_

* CVB version bump to 0.8.1   _(0fe61be)_

### 📝 Other

* Merge pull request #15 from comfy-home/0.8.1   _(2fa1fd2)_

<br>

---
... ✨ made with [CVB](https://github.com/comfy-home/perk-ComfyVersionBumper)

---

## Changelog 0.7.3
2026-04-15

#### What's changed:

### 📝 Other

* add empty README   _(6bfa96e)_

* update changelog for version 0.7.2   _(bb9500b)_

<br>

---
... ✨ made with [CVB](https://github.com/comfy-home/perk-ComfyVersionBumper)

---

## Changelog 0.7.2
2026-04-15


#### What's changed:

### ✨ New Feature: Changelog summary Generation

* CVB now generates changelog summary in `.changelogs` folder upon release, and stores it as `README` within the folder, so it's rendered as page on GitHub   _(da19505)_

---

### ✨ New in Changelog Categories:

#### 💎 Enhancements

* add support for 'Broken' category in changelog:   _(da19505)_
  * with synonyms (broken, brkn, brk, notworking, dnw, fail)
  * with header output (⛓️‍💥 Not Working Yet / Broken)
  * should be used eg in cases when commit was tested and did not bring desired result, but you want to keep it for further dev

---

### General Improvements:

### 🐛 Fix(es)

* enhance scrolling behavior in ReleaseNow logger to respect auto-follow setting   _(42cfed0)_

### 🔧 Maintenance

* CVB version bump to 0.7.2   _(278870c)_

### 📝 Other

* add history summary README path to changelog processing in overview bump workflow   _(4d233b2)_

<br>

---
... ✨ made with [CVB](https://github.com/comfy-home/perk-ComfyVersionBumper)

---

## Changelog 0.7.1
2026-04-14

#### What's changed:

### 🧩 Features

* add terminal control sequence stripping for log output in ReleaseNowDialog   _(59735b6)_

* enhance ReleaseNowDialog with body selection functionality and scrolling improvements   _(42efe01)_

* still broken: enhance ReleaseNowDialog with dynamic body viewport height and scrolling adjustments   _(557d591)_

* still broken: enhance ReleaseNowDialog with body viewport height and scrolling improvements   _(2cbbca1)_

* implement function to find archived changelog markdown and add history label candidate generation   _(30c64cf)_

* add function to find archived changelog markdown and refactor selection confirmation logic   _(5609345)_

### 🐛 Fix(es)

* copied log is no longer malformed   _(59735b6)_

* fixes previously broken→add release now log viewport handling in render_release_now_dialog   _(7fdf770)_

* fixes previously broken→add release now log viewport and selection handling in ReleaseNowDialog   _(affa899)_

<br>

---
... ✨ made with [CVB](https://github.com/comfy-home/perk-ComfyVersionBumper)

---

## Changelog 0.7.0
2026-04-14

#### What's changed:

### ✨ New Feature: ReleaseNOW!

#### 🧩 Features

* ...from now on... let's RELEASE!   _(f526cb8)_

---

### ✨ New Feature: DELETE scope/project

#### 🧩 Features

* yeah, bit silly, but I forgot about this one 🤦‍♂️, we can delete now 🧨   _(ee33aa9)_

---

### General Improvements:

### 🧩 Features

* implement cancellation handling and auto-follow controls in ReleaseNOW dialog   _(5b9601c)_

* enhance ReleaseNOW dialog with follow and cancel controls, and update log display   _(1717c68)_

* add controls for auto-follow and cancellation in ReleaseNOW dialog   _(b9452fb)_

* enhance ReleaseNow dialog with running state management and live logging   _(adad7ae)_

* add running state messages and controls to ReleaseNOW dialog   _(7ec6f1d)_

* enhance ReleaseNOW dialog with running state management and logging   _(851f5d8)_

* update button labels in semver and calver tiles with `rls` button   _(d28ca0d)_

* implement ReleaseNOW and Release Notes dialogs in the app   _(41929e8)_

* update dashboard tile action to open ReleaseNow dialog instead of recent changes   _(c32832c)_

* implement ReleaseNOW dialog and associated functionality for release management   _(82eb2f4)_

* implement ReleaseNow dialog and validation logic for release management   _(f526cb8)_

* update package metadata in Cargo.toml   _(bf9fc57)_

* add delete confirmation dialog rendering and integration in the app   _(772330c)_

* demo project: implement placeholder functionality for local-only projects in dashboard overview   _(7d60df4)_

* add delete confirmation dialog and handling for project and scope deletions   _(ee33aa9)_

* add test helper method to ConfigStore for path initialization   _(d15fae3)_

* enhance project settings with scrolling and focus management features   _(f76c372)_

* enhance App struct with new initialization methods and project settings handling   _(df3598c)_

* add license header to rls-now.rs file   _(18129f1)_

* remove changelog path field from project wizard and add release now settings   _(988781a)_

* hide changelog path field in project edit dialog   _(358570f)_

* enhance project settings management with new focus handling and input synchronization   _(3db7b12)_

* enhance changelog preview context collection by including changelog paths   _(a076b63)_

* add release now settings to test configurations and update changelog path handling   _(1f8ad2c)_

* add release now settings to project configuration and enhance changelog path handling   _(f8a39ec)_

* enhance project settings management by adding state handling and refactoring key handling logic   _(dc03265)_

* refactor render logic by removing settings screen and enhancing progress dialog   _(ae5c660)_

* remove changelog_enabled field and update related logic in ProjectWizard   _(4356f60)_

* remove changelog_enabled field and adjust related logic in ProjectEditDialog   _(ba2c6a0)_

* implement project settings tab with changelog generation toggle and layout adjustments   _(a4ba10b)_

* enhance changelog generation logic to support scope-based settings   _(89ed6c0)_

* add ProjectSettings tab to overview and update related logic   _(bb76f2c)_

* add changelog_enabled field to test configurations   _(cbcb82b)_

* add changelog settings to project configuration and enhance migration logic   _(1abd400)_

* enhance project settings tab functionality and streamline dashboard interactions   _(1b08d55)_

* add tui-checkbox dependency to Cargo.toml and Cargo.lock   _(9e62914)_

### 🐛 Fix(es)

* improve path normalization in normalize_pathspec function that was throwing error with Linux AMD64 builds   _(09b4b99)_

### 📝 Other

* Merge pull request #10 from comfy-home/v0.7.0   _(e6f9962)_

<br>

---
... ✨ made with [CVB](https://github.com/comfy-home/perk-ComfyVersionBumper)

---

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

---

## Changelog 0.6.0
2026-04-13

**MAJOR PERFORMANCE REFACTOR**
1. Dashboard recent-changes refreshes no longer run during ordinary redraws.
2. Recent-changes history is lazy-loaded instead of precomputing all tag windows up front.
3. Dashboard tile activity is cached outside the render path and invalidated explicitly after repo-changing actions or manual reload.
4. The main loop redraws on input, resize, or transient UI ticks instead of redrawing every poll interval.
5. Repo-backed actions now show an explicit progress/loading state before work begins.
6. Recent-changes open, scope rotation, history-tab loading, and refresh now run behind the background job boundary.
7. Dashboard changelog preview generation and overview workflow changelog preview generation now run behind the background job boundary.
8. Tag, push, and release operations now run behind the background job boundary.
9. The former dedicated worker-thread transport has been replaced with a Tokio runtime that dispatches typed jobs and routes results back to the UI.
10. Multi-repo changelog preview generation now runs with bounded parallelism instead of serializing each repo scan.
11. Overview activity-cache warmup now runs as a low-priority prefetch/refresh job instead of blocking the UI thread.
12. Remote tag-push and GitHub release creation now run with timeout and retry orchestration.
13. Recent-changes and changelog-preview git scans now run through cancellable git execution, so superseded jobs can interrupt long-running local git subprocesses instead of only being ignored after completion.
14. The recent-changes flow now prefetches the next likely scope and the History tab window on a low-priority lane, so common follow-up navigation can reuse warmed data.

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

---

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

<br>

---
... ✨ made with [ComfyGit](https://github.com/comfy-home/ComfyGit)