# Changelog History

Newest archived changelogs first. When multiple archived files represent the same version, only the newest archive is included here.

## Changelog `v0.26.0` <sup><div align="end">🗓️ 2026-05-13</div></sup>

### ✨ New in Auto-README 'What's new' inject!:

#### 🧩 Features

* Improve TopPicks section extraction and footer formatting in changelog markdown. Ensure proper handling of nested details.   _(a0deb22)_

---

## 💬 General Improvements & Fixes:

### 🐛 Fix(es)

* Update "click here" changelog summary formatting with nbsp   _(da18d36)_

### 🔧 Maintenance

* CG app version bump to v0.26.0   _(cb8077c)_

* Update ratatui-comfy-toaster to version 0.3.2 and add default progress bar settings in ToastEngine   _(e0aa8e8)_

### ♻️ Refactor

* Simplify file staging and commit logic for release process. Introduce dedicated functions for handling generated paths and staged changes, enhancing readability and maintainability. Add async support for README auto-injection confirmation.   _(a99b33c)_

* Improve code readability by formatting multiline expressions and enhancing consistency in progress message handling.   _(d5731be)_

### 📝 Other

* Merge pull request #111 (via ComfyGit)   _(40f2e0b)_

---

## Changelog `v0.25.4` <sup><div align="end">🗓️ 2026-05-13</div></sup>

### 💥 💥 💥 This Release's Top Picks ...  💥 💥 💥

#### **1. &nbsp;&nbsp;&nbsp;TOP PICKS EDITOR!**
- Now you can add, and edit your TP from TUI
    - Assigned shortcut `P`
- Fully implemented keyboard shortcuts (ctrl+a/c/v)
- Fully implemented mouse action shortcuts 
    - rightClick to paste/copy, doubleClick to select word, click to position cursor, drag&hold to select

#### **2. &nbsp;&nbsp;&nbsp;Auto-README changelog injection!**
- You can now automatically inject an expandable one-liner with latest changes from the last release, amazing feature if you ask me...! 🤩
- Make sure to check WIKI pages to understand it fully...

#### **3. &nbsp;&nbsp;&nbsp;Misc**
- Added support in Distro for "General" scripts
    - Unlike Win/Arm/Amd/Mac, General is not required to produce any artifacts
    - Useful for small projects (e.g. crates, plugins, etc)
- Project reordering in Projects pane
    - Now you can click&drag your projects to change their order
    - Remember, you can do this for a while also with tiles within the project
- Release Notes Editor in ReleaseNOW got enhanced
    - added mouse and keyboard shortcuts


<sub>...  🎉 Enjoy!</sub>

<br>

### 🐛 Fix(es)

* Update top picks section extraction to correctly identify headings and retain entries until the next section. Enhance tests to verify functionality.   _(896abed)_

### 🔧 Maintenance

* CG app version bump to v0.25.4   _(d7530cf)_

### 💎 Enhancements

* Enhance auto-injection logic to replace existing auto-injected blocks in README files. Introduce a new function to identify and manage existing blocks, ensuring proper updates without duplicating content. Add tests to verify the replacement behavior and preserve non-auto-injected details.   _(4bc8eeb)_

### 🧪 Tests

* Enhance Top Picks merging logic to prioritize edits by slot and header normalization. Introduce a new function for header normalization and update tests to validate behavior with priority and header changes. This is an alternative approach to avoid duplicites that needs to be properly tested   _(5ce4400)_

### 📝 Other

* Merge pull request #110 (via ComfyGit)   _(10be2ff)_

---

## Changelog `v0.25.3` <sup><div align="end">🗓️ 2026-05-13</div></sup>

### 💥 💥 💥 This Release's Top Picks ...  💥 💥 💥

#### **1. &nbsp;&nbsp;&nbsp;TOP PICKS EDITOR! ⭐**
- Now you can add, and edit your TP from TUI
    - Assigned shortcut `P`
- New modal with full MD support
- Fully implemented keyboard shortcuts (ctrl+a/c/v)
- Fully implemented mouse action shortcuts 
    - rightClick to paste/copy, doubleClick to select word, click to position cursor, drag&hold to select

#### **2. &nbsp;&nbsp;&nbsp;Auto-README changelog injection!**
    - You can now automatically inject an expandable one-liner with latest changes from the last release, amazing feature if you ask me...! 🤩
    - Make sure to check WIKI pages to understand it fully...

#### **3. &nbsp;&nbsp;&nbsp;TOP PICKS EDITOR!**
- Now you can add, and edit your TP from TUI
- Assigned shortcut `P`
- New modal with full MD support
- Fully implemented keyboard shortcuts (ctrl+a/c/v)
- Fully implemented mouse action shortcuts (rightClick to paste/copy, doubleClick, click, drag&hold)

#### **4. &nbsp;&nbsp;&nbsp;Misc**
- Added support in Distro for "General" scripts
    - Unlike Win/Arm/Amd/Mac, General is not required to produce any artifacts
    - Useful for small projects (e.g. crates, plugins, etc)
- Project reordering in Projects pane
    - Now you can click&drag your projects to change their order
    - Remember, you can do this for a while also with tiles within the project
- Release Notes Editor in ReleaseNOW got enhanced
    - added mouse and keyboard shortcuts


<sub>...  🎉 Enjoy!</sub>

<br>

### ✨ New in Top Picks:

#### 🧩 Features

* Add Top Picks editor dialog with Markdown support for enhanced user experience   _(0cba1f2)_

* Enhance Top Picks editor with additional keyboard shortcuts and mouse actions for improved usability   _(9651e57)_
* integrate new dialog into the app structure.   _(9651e57)_

* Implement merge functionality for Top Picks, allowing manual edits to override commit-based selections   _(c013785)_
* enhance editor dialog with updated comment syntax and improved parsing logic.   _(c013785)_

* Introduce top picks edits functionality to allow manual overrides and additions to Top Picks   _(d559a63)_
* update rendering logic to incorporate edits in changelog generation   _(d559a63)_
* add tests for new feature.   _(d559a63)_

* Add manual top picks field to ProjectConfig for user-defined selections   _(fd71db6)_
* enhances flexibility in managing top picks.   _(fd71db6)_

* Implement memory management for Top Picks edits   _(284432a)_
* add functions to create, load, save, and clear edits in a dedicated memory directory.   _(284432a)_

* Add rendering logic for Top Picks editor dialog- includes layout, text area, and button row for saving and canceling edits.   _(56a2dfa)_

* Add error handling for clearing saved Top Picks edits after release, also emit warnings if the operation fails, enhancing user feedback during the release process.   _(e349ed1)_

---

### 💫 _Changed in:_ **TUI**

#### 🐛 Fix(es)

* Fixed not working clipboard in "New Project" and "Edit Project" wizards within TUI   _(940193f)_

* Clear project rectangles before rendering dashboard and track project rectangles for hit targets   _(5523b96)_

#### 💎 Enhancements

* Implemented project drag-and-drop functionality in the dashboard, allowing users to reorder projects. Added support for tracking drag state and updating project order with persistence.   _(1018b70)_

#### ♻️ Refactor

* Consolidate transient and sticky toasters into a single toaster instance for improved toast management - taking advantage of new comfy-toaster enhancements   _(17b2ebb)_

* Simplify toaster management by merging transient and sticky toasters into a single instance - taking advantage of new comfy-toaster enhancements   _(2da90a3)_

---

### 💫 _Changed in:_ **Distro Scripts**

#### 🧩 Features

* Add ProjectSettingsReleaseNowGeneral to BrowseTarget and update related logic in app.rs   _(68e5e4e)_

* Add general_script field to ReleaseNowSettings for enhanced configuration options   _(4ec1f6c)_

* Introduce ReleaseNowGeneral field and update related logic for improved project configuration   _(2f1353e)_

* Add general release option to collect_release_now_options for ReleaseNOW launch checks and config   _(4dbfb28)_

---

### 💫 _Changed in:_ **!r**

#### 📝 Other

* Add rls_now_inj module and refactor toast mouse event handling for improved readability   _(850333e)_

* Add readme injection options for enhanced configuration   _(014bd10)_

* Implement readme injection options and related UI updates for enhanced project configuration   _(c9bd894)_

* Implement README injection for "What's new" section with TopPicks support   _(cdd9939)_

* Enhance README injection functionality by adding configuration options for injection enablement and row position, along with updates to validation and execution logic.   _(f625321)_

* Changed injection timing to fix potential issues when user uses eg `cargo publish` for crate projects   _(8f34314)_

* Implement footer stripping logic to remove original ComfyGit footer from changelog markdown, ensuring cleaner output. Update tests to verify footer removal functionality.   _(d82b9bf)_

---

### 💫 _Changed in:_ **RLS Notes Edit**

#### 🧩 Features

* Add release now notes textarea functionality with copy, cut, and select all support   _(b90e84a)_

* Add HitTarget for ReleaseNowNotesField to enhance interaction   _(8f8f02f)_

#### 🐛 Fix(es)

* Fixed ctrl+a visual interpretation   _(c080672)_

* Fixed rightClick paste action   _(0c227fe)_

#### ♻️ Refactor

* Simplify textarea click position calculation and improve visibility handling - fixes miscalculations and cursor positioning via click action   _(354a1f3)_

---

### 💫 _Changed in:_ **!rls**

#### 📝 Other

* Add release title template to ReleaseNowSettings for enhanced configuration   _(e9163ef)_

* Add release title template to ProjectSettings   _(5b87a59)_

* Update release title handling and add template support in ReleaseNowDialog   _(dc6f2f9)_

---

### 💫 _Changed in:_ **ReleaseNOW Output Log in TUI**

#### 🐛 Fix(es)

* Attempted fix for wrong line selection for copying after wrapped line, needs testing - to be done during the next release   _(aeefe88)_

---

## 💬 General Improvements & Fixes:

### 🐛 Fix(es)

* Allow General script to have no artifacts   _(dc6f2f9)_

### 🔧 Maintenance

* CG app version bump to v0.24.0   _(a038cfc)_

* CG app version bump to v0.24.1   _(d2a0180)_

* CG app version bump to v0.24.2   _(9aa86f4)_

* CG app version bump to v0.24.3   _(eb643b2)_

* Update ratatui-comfy-toaster to version 0.3.1 and adjust its path in Cargo.toml   _(4d1f048)_

* CG app version bump to v0.24.4   _(880b969)_

* CG app version bump to v0.24.5   _(d4b71e8)_

* CG app version bump to v0.24.6   _(a8668f1)_

* CG app version bump to v0.24.7   _(a697c3f)_

* CG app version bump to v0.25.0   _(d8447bf)_

* CG app version bump to v0.25.1   _(b4e7ffa)_

* CG app version bump to v0.25.2   _(e0787d7)_

* CG app version bump to v0.25.3   _(2483800)_

### 📱UI Changes

* Add Linux clipboard paste functionality and integrate with key events   _(940193f)_

### 📝 Other

* Merge pull request #99 (via ComfyGit)   _(819cc6a)_

* Merge pull request #100 (via ComfyGit)   _(5770921)_

* Merge pull request #101 (via ComfyGit)   _(ee60115)_

* Merge pull request #102 (via ComfyGit)   _(cf529b7)_

* Merge pull request #104 (via ComfyGit)   _(90c1dec)_

* Merge pull request #103 (via ComfyGit)   _(0368aec)_

* Merge pull request #105 (via ComfyGit)   _(ff6bfa8)_

* Merge pull request #106 (via ComfyGit)   _(378151a)_

* Merge pull request #107 (via ComfyGit)   _(06cc20e)_

* Merge pull request #108 (via ComfyGit)   _(04a6715)_

* Merge pull request #109 (via ComfyGit)   _(47e2d40)_

---

## Changelog `v0.23.6` <sup><div align="end">🗓️ 2026-05-11</div></sup>

### 💥 💥 💥 This Release's Top Picks ...  💥 💥 💥

#### **1. &nbsp;&nbsp;&nbsp;Optional detailed changelog wrapping**
- This is the first RLS changelog that uses a fully automated auto-wrap!
    - Purpose? I'd say 90% of people are not interested in all those nerdy changelogs, and they would very much prefer to see just the most relevant info...that's exactly what this feature is about!
- Can be enabled/disabled in Project Settings
- Is applied only if the current release has Top Picks defined

#### **2. &nbsp;&nbsp;&nbsp;Please meet: the V A R I A T O R**
- The ComfyGitFlow just got even smoother...
- Make sure to check Variator's WIKI to understand how great it is!

#### **3. &nbsp;&nbsp;&nbsp;Misc**
- `cg br end` / `cg pr` flow got enhanced with auto-target misdetection warning with IA CLI flow


<sub>...  🎉 Enjoy!</sub>

<br>

### 💫 _Changed in:_ **RLS Changelog**

#### 🧩 Features

* add support for detailed changelog wrapping based on top picks configuration   _(3c69c2a)_

---

### 💫 _Changed in:_ **!v**

#### 📝 Other

* introduce variator storage for managing commit message configurations with auto-assigned IDs   _(66f785d)_

* integrate variator storage into release notes generation for enhanced commit message management   _(a6b5392)_

* enhance changelog generation by integrating variator storage into parsing and document building functions   _(3a57e15)_

* add variator command support for listing, setting, and clearing variators   _(72bb355)_

* add variator_storage field to ProjectConfig with default initialization   _(9a3dbe2)_

* ensure default values are set for RepoConfig in test cases   _(06daeaf)_

* add chl_vrtr module for enhanced functionality integration   _(642cd56)_

* refactor changelog document building to utilize variator storage and streamline default initialization in tests   _(29a7e72)_

* streamline default initialization in ProjectEditDialog and ProjectWizard to enhance test reliability   _(08ec87a)_

---

### 💫 _Changed in:_ **CLI flows**

#### 💎 Enhancements

* Enhance PR preview functionality to warn users when targeting main/master from a -dev branch   _(1f7cd02)_
* added custom main branch support and improved user prompts for target branch changes.   _(1f7cd02)_

---

### 💫 _Changed in:_ **Top Picks**

#### ♻️ Refactor

* Excluding TopPicks from PR Description/Changelog   _(56fb784)_

---

### 💫 _Changed in:_ **Variator**

#### 🐛 Fix(es)

* Fixed bug that was causing the variator to be global instead of being project-aware   _(d9f783c)_

#### 💎 Enhancements

* Add rename functionality with conflict detection and outcomes   _(6e4ef04)_

* Implement rename command with conflict resolution and IA CLI menu   _(7b7bfed)_

---

## 💬 General Improvements & Fixes:

### 🔧 Maintenance

* CG app version bump to v0.23.0   _(62995b9)_

* CG app version bump to v0.23.1   _(206e355)_

* CG app version bump to v0.23.2   _(46ec1bd)_

* CG app version bump to v0.23.3   _(91279e5)_

* CG app version bump to v0.23.4   _(c52fbb7)_

* CG app version bump to v0.23.5   _(7215e5d)_

* CG app version bump to v0.23.6   _(b520554)_

### 📝 Other

* Merge pull request #92 (via ComfyGit)   _(2ed9fa2)_

* Merge pull request #93 (via ComfyGit)   _(dba12a1)_

* Merge pull request #94 (via ComfyGit)   _(10d3fcd)_

* Merge pull request #95 (via ComfyGit)   _(ed71eaf)_

* Merge pull request #96 (via ComfyGit)   _(219636a)_

* Merge pull request #97 (via ComfyGit)   _(3e547ae)_

* Merge pull request #98 (via ComfyGit)   _(840a762)_

---

## Changelog `v0.22.7` <sup><div align="end">🗓️ 2026-05-10</div></sup>

### 💥 💥 💥 This Release's Top Picks ...  💥 💥 💥

#### **1. &nbsp;&nbsp;&nbsp;This is the bigest addition. Brand new "Top Picks" section!**
- with `cg tp` CLI command for quick print out

#### **2. &nbsp;&nbsp;&nbsp;New CLI enhancement: Merge target change**
- You can now press `X` during `cg br end` if you want current branch merge into a different branch!
- After `X` press you'll be presented with interactive CLI menu

#### **3. &nbsp;&nbsp;&nbsp;"Rename Commit" modal in TUI now displays edited message not  just as a one-liner, user can see the hole message and interact with keyboard/mouse inputs**
#### **4. &nbsp;&nbsp;&nbsp;NEW `cg br cd` CLI command to switch your current branch!**
- Now you can simple call this command, and you'll be presented with an interactive paginated menu with recently used branches
- Workflow got even easier now: ``cg br cd` -> select -> ENTER -> done

#### **5. &nbsp;&nbsp;&nbsp;You can now choose to have commit hashes smaller in release changelogs**
- You can enable/disable this in "Project Settings"


<sub>...  🎉 Enjoy!</sub>

<br>

### ✨ New Enhancement: New CLI Command `cg br cd`

* Implement branch selection UI for navigating and switching branches in the CLI   _(99fd7f5)_

* Add 'cg br cd' command for interactive branch switching in CLI   _(a5e46af)_

---

### ✨ New Feature: Mini Hashes in RLS Changelog

* Add mini_commit_hashes option to ProjectEditDialog and ProjectWizard   _(e8d24a5)_

* Integrate changelog_mini_commit_hashes option into ScopeDraft and related functions   _(a90f717)_

* Implement changelog mini commit hashes functionality for project scopes   _(e99a193)_

* Enhance changelog rendering with mini commit hashes option   _(c65df38)_

* Add changelog_mini_commit_hashes option to CLI configuration   _(5e49d01)_

* Add mini_commit_hashes field to GitScopeContext and update related functions   _(d1dbb01)_

* Update collect_preview_entries_async to include mini_commit_hashes in document rendering   _(17811e3)_

* Add changelogMiniCommitHashes to ProjectSettings for enhanced configuration   _(73168b1)_

---

### ✨ New Feature: Top Picks

* Implement Top Picks changelog feature to highlight significant improvements with priority-based rendering   _(fe4c80b)_

* Extend changelog functionality with support for Top Picks, including parsing and rendering enhancements for priority-based items   _(beb8f79)_

* Add changelog_tp module to support Top Picks functionality   _(fac8125)_

---

### ✨ New Feature: Top Picks - Auto Wrap

* Introduce changelog_wrap_detailed_if_top_picks option for enhanced changelog configuration   _(da660e1)_

* Add wrap_detailed_if_top_picks option to conditionally wrap detailed changelog sections based on Top Picks presence   _(630de0e)_

* Add wrap_detailed_changelog_if_top_picks option to ProjectEditDialog and ProjectWizard for improved changelog customization   _(aa50c29)_

* Add changelog_wrap_detailed_if_top_picks option to ProjectSettings for improved customization of detailed changelog sections.   _(72f23b8)_

* Implement changelog_wrap_detailed_if_top_picks and associated methods for ProjectConfig to enhance detailed changelog behavior based on project type and scope.   _(d04db2a)_

* Add changelog_wrap_detailed_if_top_picks option to multiple test configurations for improved changelog behavior consistency.   _(f49334b)_

* Add changelog_wrap_detailed_if_top_picks option to test configurations for enhanced changelog behavior consistency   _(5472d2f)_

* Implement changelog_wrap_detailed_if_top_picks in ProjectSettingsState and related methods for enhanced changelog customization   _(534a710)_

---

### ✨ New in Top Picks:

#### 🧩 Features

* CLI command `cg tp` to quickly display configured Top Picks for upcoming release, synonyms: `cg topp` and `cg toppicks`   _(dc79116)_

---

### 💫 _Changed in:_ **CLI commands**

#### 💎 Enhancements

* Error handling enhanced with interactive menus for `cg br end`   _(276885b)_

* Enhancinginteractive menu flowfor `cg br end` to avoid further errors and handle them from within CLI   _(0353e69)_

* Add ANSI color support for non-main branch warnings   _(19659dd)_

* Add target branch selection feature in PR preview via X shortcut   _(a71f522)_

* Improve target branch loading and navigation in PR preview with pagination support and time ordering   _(de74f02)_

---

### 💫 _Changed in:_ **CLI Commands**

#### 🐛 Fix(es)

* Fix for not displaying the "ahead" interactive menu in 3-step `cg br end` error handling IA flow   _(c5825f7)_

---

### 💫 _Changed in:_ **Commit Rename Modal**

#### 🐛 Fix(es)

* message body wrapping   _(e97014e)_

* re-enabling keyboard and mouse input functions, similar as they were in one-liner implementation   _(81b00e0)_

* re-enabling keyboard and mouse input functions, similar as they were in one-liner implementation PART2 - enabled keybord input debug   _(4efe1ea)_

* re-enabling keyboard and mouse input functions, similar as they were in one-liner implementation PART3 - disabled keybord input debug, added modal footer shortcut info   _(9740239)_

* re-enabling keyboard and mouse input functions, similar as they were in one-liner implementation PART4 - added more keyboard shortcuts to the footer   _(f6c0a4b)_

* bugfixes PART5 - clipboard functionality, visual interpretation of selected text, mouse input   _(6d5f84a)_

* bugfixes PART6 - further clipboard functionality fixes,  mouse input debug   _(aadf477)_

* bugfixes PART7 - further clipboard functionality fixes,  and mouse input debug   _(2065eec)_

* bugfixes PART8 - further clipboard functionality fixes,  and mouse input debug   _(061a3b1)_

* bugfixes PART9 - mouse input now can register horizontal position   _(6eea32a)_

* bugfixes PART10 - mouse input vertical registration debug   _(ee5b1d0)_

* bugfixes PART11 - further mouse input vertical registration debug   _(9a02abf)_

* bugfixes PART12 - further mouse input vertical registration debug   _(c85140a)_

* bugfixes PART13 - vertical registration fixed, with slight deviation within the next row when the previous line is too wrapped due to a word lenght, but overall acceptable result, fuyoo.   _(948596a)_

* bugfixes PART14 - assigned `Shift+ENTER` for line breakage, added 1 spacer row below the Rename Commit header for better vis   _(e5f0f3f)_

* bugfixes PART15 - fixed layout shift in the modal, removed mouse debug   _(808236c)_

* bugfixes PART16 - added debug for shift+enter to see why it's not working   _(b8f9283)_

* bugfixes PART17 - Switched to `alt+enter` for new line   _(7836651)_

* bugfixes PART18 - Footer consolidation   _(f4c935b)_

* bugfixes PART19 - Footer consolidation p2   _(b25334f)_

* bugfixes PART20 - Investigation of not working click actions (except cursor positioning), fixed click&drag to select   _(ee651c2)_

* bugfixes PART21 - Investigation of not working double-click action to select a word, fixed word start anchor   _(16aa432)_

* bugfixes PART22 - word end anchor fix for double-click word select   _(b80f0d9)_

* bugfixes PART23 - update newline insertion to support Ctrl+Enter in addition to Alt+Enter so users can use what they are used to   _(ed43b2c)_

* bugfixes PART24 - hm, ctrl not working. added debug   _(942dcf0)_

* bugfixes PART25 - reverted back to sole use of `alt+enter` due to terminal limitations   _(dfe2997)_

#### 💎 Enhancements

* Refactor commit message handling to use TuiTextArea for multi-line input with Markdown support   _(6d102f0)_
* update rendering logic to accommodate new editor structure and improve user experience.   _(6d102f0)_

* Update paragraph rendering to enable text wrapping for improved display of multi-line content in the TUI interface.   _(0f95198)_

---

### 💫 _Changed in:_ **Top Picks**

#### 🐛 Fix(es)

* adjust bullet level handling and rendering for top-level lists   _(d1a39c2)_
* add test for immediate list following header   _(d1a39c2)_

* enhance detection of top pick references by refining bullet level checks in message parsing   _(3c387ce)_

#### ♻️ Refactor

* streamline extraction and merging of top picks   _(bb4a7b8)_
* enhance detection of top pick configurations and improve commit parsing logic   _(bb4a7b8)_

---

## 💬 General Improvements & Fixes:

### 🐛 Fix(es)

* code formatting   _(b94d54e)_

* Fixed paragraph parsing when (Specific) modifier is NOT used:   _(d0f1c45)_
  * Makes this indented line possible
    * And this one as well :)

### 🔧 Maintenance

* CG app version bump to v0.22.0   _(ace4472)_

* CG app version bump to v0.22.1   _(2438f65)_

* CG app version bump to v0.22.2   _(08ca736)_

* CG app version bump to v0.22.3   _(52952ea)_

* CG app version bump to v0.22.4   _(541f65b)_

* CG app version bump to v0.22.5   _(612b854)_

* CG app version bump to v0.22.6   _(2692919)_

* CG app version bump to v0.22.7   _(f67c4f8)_

### 📝 Other

* Merge pull request #84 (via ComfyGit)   _(f6c120e)_

* Merge pull request #85 (via ComfyGit)   _(314ee40)_

* Merge pull request #86 (via ComfyGit)   _(7613d94)_

* Merge pull request #87 (via ComfyGit)   _(a9553d1)_

* Merge pull request #88 (via ComfyGit)   _(69fc3b7)_

* Merge pull request #89 (via ComfyGit)   _(006f33c)_

* Merge pull request #90 (via ComfyGit)   _(0b5c19f)_

* Merge pull request #91 (via ComfyGit)   _(714dff4)_

---

## Changelog `v0.21.4` <sup><div align="end">🗓️ 2026-05-08</div></sup>

### 💫 _Changed in:_ **RLS Changelog**

#### 💎 Enhancements

* Tightening (Specific) detection   _(ed36e7f)_

#### ♻️ Refactor

* Date display reorganization   _(217b97b)_

* New format for (Specific) grouper, `### 💫 _Changed in:_ **{subject}**`   _(476b6cf)_

* Removal of `What's new:` line   _(2cc48b5)_

* General header change   _(865fe1d)_

* General header change →h2   _(40c05de)_

* Exclude "via ComfyGit" from scope extraction in PR merge messages   _(95a9b08)_

* Integrate hide options for PR and bump messages into GitScopeContext and related functions   _(be5afe2)_

---

### 💫 _Changed in:_ **Hiding of PR & bump messages in RLS changelog**

#### 🧩 Features

* Add options to hide PR and bump messages in changelog generation   _(d5b759f)_

* Implement filters to hide PR and version bump messages in changelog generation   _(9687a02)_

* Add default settings to hide PR and bump messages   _(4495aad)_

* Add methods to manage visibility of PR and bump messages for specific scopes   _(d86c720)_

* Set default values to hide PR and bump messages in changelog configuration   _(9d4db2f)_

* Add default configuration options to hide PR and bump messages   _(420eb40)_

* Implement dual checkbox for hiding PR and bump messages in project settings   _(3c4cf8e)_

* Set default values for hiding PR and bump messages in project edit and test configurations   _(b18d2da)_

* Update default settings to include visibility options for PR and bump messages   _(c7e8491)_

* Add labels for new options to hide PR and bump messages in project settings   _(86b3ab8)_

---

### 💫 _Changed in:_ **RLS Changelog Preview**

#### ♻️ Refactor

* Use scope settings for hiding PR and bump messages in changelog generation   _(9669cda)_

* Enhance document generation to respect hide options for PR and bump messages   _(ffa1b39)_

---

## 💬 General Improvements & Fixes:

### 🐛 Fix(es)

* footer tests   _(03731c2)_

* Update footer formatting and enhance prefixed clause detection with new tests   _(7defdd9)_

* Simplify conditional logic for category and specific part extraction   _(d89685d)_

* Refactor conditional structure for improved readability in prefix parsing   _(7228a83)_

* Update error message to include command options for clarity   _(984d5a8)_

* code clean-up   _(4392629)_

* simplify conditional logic in prefix parsing   _(69ea469)_

* improve readability of conditional logic in prefix parsing   _(1b153f4)_

* fixed scroll anchor at the bottom of "Release Notes Preview" and "ReleaseNOW log"   _(d3c9350)_

* fixed inability to use `SPACE` in "Rename Commit" modal   _(3fcc598)_

* Bugfix for added CLICK event support within "Rename Commit" modal where it assumed non-custom layout width   _(78e5970)_

* Bugfix for added CLICK event support within "Rename Commit" modal - fixed event catching timing issues   _(dde47ec)_

* fixes no visual representation of the selected text in Rename Commit modal   _(7cc9a39)_

* Simplify method calls for improved readability in key event handling   _(0cd422c)_

### 🔧 Maintenance

* CG app version bump to v0.21.0   _(6f6e6de)_

* CG app version bump to v0.21.1   _(054d700)_

* CG app version bump to v0.21.2   _(3d79712)_

* CG app version bump to v0.21.4   _(6d5702d)_

* code clean-up   _(ab34dd5)_

* CG app version bump to v0.21.3   _(0b26c16)_

### 💎 Enhancements

* Added `Source` and `Target` info also to the bottom within `cg br end` CLI output   _(a500064)_

Added HOME/END Key Support. Works in:   _(87eab65)_
* Release Notes Preview
* ReleaseNOW log (running & completed)
* Changelog Preview

* Added CLICK event support within "Rename Commit" modal   _(b6e0807)_

* Implemented copy and paste functionality in text input, including support for CTRL+C and right-click context menu actions in "Rename Commit"   _(9f962a7)_

### 📝 Other

* Merge pull request #79 (via ComfyGit)   _(87e7d55)_

* Merge pull request #80 (via ComfyGit)   _(a6b0116)_

* Merge pull request #81 (via ComfyGit)   _(ceb33e9)_

* Merge pull request #82 (via ComfyGit)   _(cb9984e)_

* Merge remote-tracking branch 'origin/main' into HEAD   _(f5ce24b)_

* Merge pull request #83 (via ComfyGit)   _(67ae8be)_

---

## Changelog v0.20.2
2026-05-07

#### What's new:

### Changed in QuickDownloads

#### ♻️ Refactor

* Changed unavailable tooltip formulation   _(8f47c4d)_

---

### Changed in Clipboard

#### 🧩 Features

* Enhance clipboard functionality for Linux   _(7a8815f)_

* Implement Linux clipboard support via command line utilities, final fix for Linux. Successfully tested on Fedora 44 KDE Plasma Spin   _(b5777ae)_

---

### Changed in ReleaseNowDialog

#### 🧩 Features

* Add elapsed time tracking and display in the UI   _(d18c70e)_

* Refine elapsed time handling and UI display, fixes mishandling and overrun when event is completed or cancelled   _(63ee12d)_

---

### Changed in render

#### 🐛 Fix(es)

* Update highlight style in the Projects pane to use black foreground for better visibility   _(b4f2cdf)_

---

### Changed in Tiles

#### 🐛 Fix(es)

* Updated icons and spacing   _(35af617)_

---

### Changed in via ComfyGit

#### 📝 Other

* Merge pull request #77 (via ComfyGit)   _(cb3dd5a)_

* Merge pull request #78 (via ComfyGit)   _(33f1da8)_

---

### 🛠️ General:

### 🧩 Features

* Add integration_mode and version_scheme fields to ScopeDraft struct, so they are per-scope, not per-project   _(a1ce957)_

### 🐛 Fix(es)

* update formatting in format_tile_info_row assertion   _(b0ca65b)_

### 🔧 Maintenance

* CG app version bump to v0.20.0   _(49684a6)_

* CG app version bump to v0.20.1   _(021e8c5)_

* CG app version bump to v0.20.2   _(d4df357)_

### ♻️ Refactor

* Refactor integration mode and version scheme handling to be scope-specific, improving project management for branched projects.   _(28b3955)_

* rename format_tile_info_row to format_tile_dev_info_row and reformat replacing "->" with "last"   _(b5c0bee)_

* update format in RLS row within tiles, remove arrow and improve readability   _(edebb1b)_

<br>

---

## Changelog v0.19.5
2026-05-06

#### What's new:

### Changed in QuickDownloads

#### ♻️ Refactor

* changed "Position" selector style   _(b0e85e9)_

* added spacing to macOS icons   _(0cbc1c9)_

---

### Changed in TUI

#### 🐛 Fix(es)

* added clipboard fallback   _(15e9d7b)_

---

### 🛠️ General:

### 🔧 Maintenance

* CG app version bump to v0.19.5   _(7ded7ce)_

<br>

---

## Changelog v0.19.4
2026-05-06

#### What's new:

### Changed in QuickDownloads

#### 🐛 Fix(es)

* space fix 2   _(aef6b9c)_

---

### 🛠️ General:

### 🔧 Maintenance

* CG app version bump to v0.19.4   _(2186b11)_

<br>

---

## Changelog v0.19.3
2026-05-06

#### What's new:

### ✨ New Enhancement: `fish` shell integration

* add ComfyGit integration for fish shell to support 'cg cd <alias>' command   _(0876e0f)_

---

### ✨ New Feature: Quick-Downloads section in ReleaseNOW

implement QuickDownloadSlots for artifact management in GitHub releases/alpha/:   _(d823874)_
* Will provide a nice MD table within release
* 2 positions /top or bottom/

---

### ✨ New in CLI commands:

#### 💎 Enhancements

enhance branch name suggestions with semver dev branch handling and add canonical label function:   _(6d6d1e5)_
* User now can add specification to `-dev` branch via option 2 `--specific`

---

### Changed in CLI commands

#### ♻️ Refactor

* update branch name normalization to use semver_dev_branch_canonical_label   _(febe564)_

---

### Changed in via ComfyGit

#### 📝 Other

* Merge pull request #70 (via ComfyGit)   _(f4aa42d)_

* Merge pull request #71 (via ComfyGit)   _(2e7a6e9)_

* Merge pull request #72 (via ComfyGit)   _(367c46e)_

* Merge pull request #73 (via ComfyGit)   _(8c16e7c)_

* Merge pull request #74 (via ComfyGit)   _(ae9fd8a)_

* Merge pull request #75 (via ComfyGit)   _(6b7f9cb)_

* Merge pull request #76 (via ComfyGit)   _(19829c3)_

---

### Changed in UI

#### 🧩 Features

* enhance screen navigation by allowing 's' and 'S' keys to access UiSettings   _(4829d95)_

* add 'S' key shortcut to footer hints   _(e88b21f)_

update key bindings for dashboard navigation:   _(0035bda)_
* added ESC handler to exit Settings

#### ♻️ Refactor

* REMOVAL of the main tab navigation and related functionality, update screen rendering logic   _(fa709f1)_

---

### Changed in shell-integration

#### 🧩 Features

* add support for fish shell in ComfyGit installation script   _(159d620)_

* implement install-shell command for Unix, enhancing ComfyGit integration with shell environments   _(32c7cac)_

* enhance AppImage `install-shell` command dispatching by normalizing CLI arguments   _(c4a0f3e)_

* improve ComfyGit executable resolution in shell scripts   _(dfb06e2)_

* add support for PowerShell integration and improve PATH handling   _(894ce33)_

final fix for AppImage CLI functionality, tested on Fedora, in:   _(99432ac)_
* fish
* zsh
* pwsh
* bash

<sup>💡 >> all good!</sup>

---

### Changed in QuickDownloads

#### 🧩 Features

* add QuickDownloadsPosition enum and ReleaseNowQuickDownloadsSettings struct for enhanced quick download management   _(6075136)_

* add RlsQd tab and Quick Downloads fields to enhance project settings management   _(5b35e73)_

* integrate ReleaseNowQuickDownloadsSettings into ReleaseNowDialog and related structures for improved quick download functionality   _(8f7acb6)_

#### 🐛 Fix(es)

* remove duplicate LOGO_BASE and ensure consistent usage in HTML section   _(62c7f3f)_

* space fix 1   _(393f36b)_

#### 💎 Enhancements

several additions and bugfixes:   _(1e469f4)_
* Red fallback icon for missing artifacts
* Stripped HTML fallback for non-icon artifacts
* New asset paths
* non-breaking spaces fix

#### ♻️ Refactor

* improve code readability by formatting and restructuring related sections in config and project settings files   _(4a4b0ef)_

---

### Changed in git

#### ♻️ Refactor

* rename and enhance remote URL parsing function to return owner and repo as a tuple   _(fc3dc24)_

---

### 🛠️ General:

### 🔧 Maintenance

* CG app version bump to v0.18.0   _(1114822)_

* CG app version bump to v0.18.1   _(de0d8be)_

* CG app version bump to v0.18.2   _(e33fc44)_

* CG app version bump to v0.18.3   _(eb6550b)_

* CG app version bump to v0.19.0   _(ccbf707)_

* CG app version bump to v0.19.1   _(3e66488)_

* CG app version bump to v0.19.2   _(b3a481d)_

* CG app version bump to v0.19.3   _(7227bc0)_

### 💎 Enhancements

* added possibility to uninstall shell-integration via CLI →`cg uninstall-shell`   _(928d3de)_

### 📝 Other

* Revert "~portable logo 40x40"   _(7cb6b17)_

* Merge branch 'main' into 0.19.x--rlsNOW-quick-download-header   _(635c1bf)_

* Merge branch 'main' into 0.19.x--rlsNOW-quick-download-header   _(1255f58)_

* Merge branch 'main' into 0.19.x--rlsNOW-quick-download-header   _(f750147)_

* Merge branch 'main' into 0.19.x--rlsNOW-quick-download-header   _(efafc5f)_

* ReleaseNOW! → v0.19.1 has just been released via ComfyGit!"   _(c05182e)_

* Merge branch 'main' into v0.19.2-dev--bugfixes   _(926e58d)_

<br>

---

## Changelog v0.19.2
2026-05-06

#### What's new:

### ✨ New Enhancement: `fish` shell integration

* add ComfyGit integration for fish shell to support 'cg cd <alias>' command   _(0876e0f)_

---

### ✨ New Feature: Quick-Downloads section in ReleaseNOW

implement QuickDownloadSlots for artifact management in GitHub releases/alpha/:   _(d823874)_
* Will provide a nice MD table within release
* 2 positions /top or bottom/

---

### ✨ New in CLI commands:

#### 💎 Enhancements

enhance branch name suggestions with semver dev branch handling and add canonical label function:   _(6d6d1e5)_
* User now can add specification to `-dev` branch via option 2 `--specific`

---

### Changed in CLI commands

#### ♻️ Refactor

* update branch name normalization to use semver_dev_branch_canonical_label   _(febe564)_

---

### Changed in via ComfyGit

#### 📝 Other

* Merge pull request #70 (via ComfyGit)   _(f4aa42d)_

* Merge pull request #71 (via ComfyGit)   _(2e7a6e9)_

* Merge pull request #72 (via ComfyGit)   _(367c46e)_

* Merge pull request #73 (via ComfyGit)   _(8c16e7c)_

* Merge pull request #74 (via ComfyGit)   _(ae9fd8a)_

* Merge pull request #75 (via ComfyGit)   _(6b7f9cb)_

* Merge pull request #76 (via ComfyGit)   _(19829c3)_

---

### Changed in UI

#### 🧩 Features

* enhance screen navigation by allowing 's' and 'S' keys to access UiSettings   _(4829d95)_

* add 'S' key shortcut to footer hints   _(e88b21f)_

update key bindings for dashboard navigation:   _(0035bda)_
* added ESC handler to exit Settings

#### ♻️ Refactor

* REMOVAL of the main tab navigation and related functionality, update screen rendering logic   _(fa709f1)_

---

### Changed in shell-integration

#### 🧩 Features

* add support for fish shell in ComfyGit installation script   _(159d620)_

* implement install-shell command for Unix, enhancing ComfyGit integration with shell environments   _(32c7cac)_

* enhance AppImage `install-shell` command dispatching by normalizing CLI arguments   _(c4a0f3e)_

* improve ComfyGit executable resolution in shell scripts   _(dfb06e2)_

* add support for PowerShell integration and improve PATH handling   _(894ce33)_

final fix for AppImage CLI functionality, tested on Fedora, in:   _(99432ac)_
* fish
* zsh
* pwsh
* bash

<sup>💡 >> all good!</sup>

---

### Changed in QuickDownloads

#### 🧩 Features

* add QuickDownloadsPosition enum and ReleaseNowQuickDownloadsSettings struct for enhanced quick download management   _(6075136)_

* add RlsQd tab and Quick Downloads fields to enhance project settings management   _(5b35e73)_

* integrate ReleaseNowQuickDownloadsSettings into ReleaseNowDialog and related structures for improved quick download functionality   _(8f7acb6)_

#### 💎 Enhancements

several additions and bugfixes:   _(1e469f4)_
* Red fallback icon for missing artifacts
* Stripped HTML fallback for non-icon artifacts
* New asset paths
* non-breaking spaces fix

#### ♻️ Refactor

* improve code readability by formatting and restructuring related sections in config and project settings files   _(4a4b0ef)_

---

### Changed in git

#### ♻️ Refactor

* rename and enhance remote URL parsing function to return owner and repo as a tuple   _(fc3dc24)_

---

### 🛠️ General:

### 🔧 Maintenance

* CG app version bump to v0.18.0   _(1114822)_

* CG app version bump to v0.18.1   _(de0d8be)_

* CG app version bump to v0.18.2   _(e33fc44)_

* CG app version bump to v0.18.3   _(eb6550b)_

* CG app version bump to v0.19.0   _(ccbf707)_

* CG app version bump to v0.19.1   _(3e66488)_

* CG app version bump to v0.19.2   _(b3a481d)_

### 💎 Enhancements

* added possibility to uninstall shell-integration via CLI →`cg uninstall-shell`   _(928d3de)_

### 📝 Other

* Revert "~portable logo 40x40"   _(7cb6b17)_

* Merge branch 'main' into 0.19.x--rlsNOW-quick-download-header   _(635c1bf)_

* Merge branch 'main' into 0.19.x--rlsNOW-quick-download-header   _(1255f58)_

* Merge branch 'main' into 0.19.x--rlsNOW-quick-download-header   _(f750147)_

* Merge branch 'main' into 0.19.x--rlsNOW-quick-download-header   _(efafc5f)_

* ReleaseNOW! → v0.19.1 has just been released via ComfyGit!"   _(c05182e)_

* Merge branch 'main' into v0.19.2-dev--bugfixes   _(926e58d)_

<br>

---

## Changelog v0.17.6
2026-05-01

#### What's new:

### Changed in Linux

#### 🐛 Fix(es)

* Fixed clipboard behaviour /tested in Fedora/   _(b96cec5)_

---

### Changed in via ComfyGit

#### 📝 Other

* Merge pull request #69 (via ComfyGit)   _(98b6b2d)_

---

### Changed in ReleaseNOW

#### 🐛 Fix(es)

* fixed ability to pass arguments from within Distro path, eg `--win64`   _(12a6729)_

---

### 🛠️ General:

### 🐛 Fix(es)

* bugfix for custom named `main` checkbox in PSS   _(ef7eb7e)_

* changed font color and highlight in Projects pane for selected project   _(ac51b4f)_

* fixes render bugs in PSS   _(7f51778)_

* removed leftover text from PSS   _(5842f6e)_

* layout changes in PSS   _(0e9a78a)_

### 🔧 Maintenance

* CG app version bump to v0.17.6   _(12fd42c)_

* add document-features dependency   _(6ec8d0b)_

<br>

---

## Changelog v0.17.5
2026-04-27

#### What's new:

### Changed in Failed Merging

#### 🧩 Features

* added "Remote URL" parsing for use with other functions   _(8436048)_

#### 💎 Enhancements

* `cg merge` ASCII table now provides a direct link to conflict resolutions (if they exists)   _(0026560)_

* `cg branch done` / `cg br end` now provides a direct link to conflict resolutions if the flow ended with conflict   _(c2b3ac1)_

New improvements:   _(1784274)_
* Refactored `cg merge`
  * User now can invoke VSCode merge conflict resolver directly from CLI table
  * Works also from an external terminal /if VSCode is installed/

---

### Changed in CLI

#### 💎 Enhancements

* Brand new `cg new` wizard, and `cg new <action> <option>` flow   _(42ea145)_

* New commands - `cg reroot` and `cg reroot rebase` /needs testing/   _(ba5ba68)_

---

### Changed in via ComfyGit

#### 📝 Other

* Merge pull request #63 (via ComfyGit)   _(79460bf)_

* Merge pull request #64 (via ComfyGit)   _(f3f798f)_

* Merge pull request #65 (via ComfyGit)   _(d9e5df2)_

* Merge pull request #66 (via ComfyGit)   _(e9602f4)_

* Merge pull request #67 (via ComfyGit)   _(955bdd5)_

* Merge pull request #68 (via ComfyGit)   _(0e82cb7)_

---

### 🛠️ General:

### 🧩 Features

* automatically deletes the temp worktree created during merge conflict resolution   _(321fe0c)_

### 🐛 Fix(es)

* fixes bad patch branch naming options if created from non-main branch eg `.x`   _(1bfbde1)_

* fixes bad package version bump if `cg bmp ... 4/5` is performed from non-main branch eg `.x`   _(fdcb0f6)_

### 🔧 Maintenance

* CG app version bump to v0.17.0   _(2c438d8)_

* CG app version bump to v0.17.1   _(4ace72e)_

* CG app version bump to v0.17.2   _(64c930b)_

* CG app version bump to v0.17.3   _(b5d0401)_

* CG app version bump to v0.17.4   _(21410bd)_

* CG app version bump to v0.17.5   _(02b9440)_

<br>

---

## Changelog v0.16.5
2026-04-26

#### What's new:

### ✨ New Feature: `cg merge` CLI command

* Interactive merging wizard, listing all available PR's with details   _(503190b)_

---

### ✨ New in CLI commands:

#### 💎 Enhancements

`cg branch done`:   _(361d334)_
* used when user wants to end work on the branch they are currently on, and quickly create PR→check→merge
* `done` synonyms→`end`, `close`, `merge`, `mrg`, `mg`

---

### Changed in `cg merge`

#### 🐛 Fix(es)

* fixed 100ms redraw bug in CLI   _(e177f44)_

#### 💎 Enhancements

* proper ASCII table   _(e177f44)_

---

### Changed in via ComfyGit

#### 📝 Other

* Merge pull request #50 (via ComfyGit)   _(f41ee99)_

* Merge pull request #51 (via ComfyGit)   _(b5d3d7f)_

* Merge pull request #53 (via ComfyGit)   _(a0ec8ff)_

* Merge pull request #54 (via ComfyGit)   _(2ccd0b2)_

* Merge pull request #55 (via ComfyGit)   _(687211b)_

* Merge pull request #56 (via ComfyGit)   _(f17ee96)_

* Merge pull request #58 (via ComfyGit)   _(9938ec8)_

* Merge pull request #60 (via ComfyGit)   _(e40ec0d)_

* Merge pull request #61 (via ComfyGit)   _(28a04bb)_

* Merge pull request #62 (via ComfyGit)   _(22dbb47)_

---

### Changed in `cg branch done`

#### 🐛 Fix(es)

* added delay for mergeability check,, plus 2 retries   _(84ac7b5)_

---

### Changed in `cg pr`

#### 🐛 Fix(es)

* fixed failing PR creation when user uses a custom name for `main` branch   _(14295a8)_

---

### Changed in CLI

#### 🧩 Features

* add functionality to handle unpublished target branches during PR creation   _(26b4836)_

* enhance version bumping logic with semver support and branch handling   _(5a45730)_

#### 🐛 Fix(es)

* fix for `failed to derive the source rls` error   _(01fde72)_

#### ♻️ Refactor

* streamline version bump logic and enhance branch handling   _(b8ac27f)_

* improve unpublished branch error handling and rename related functions   _(66635db)_

---

### Changed in Git

#### 🧩 Features

* enhance branch name suggestion logic with bump action handling   _(f5ff190)_

* implement branch publishing with upstream and remove unused remote resolution function   _(848c81a)_

* add functionality to publish branches with upstream and resolve push remote names   _(01ea318)_

* refactor branch name suggestion to use a request struct for improved clarity and maintainability   _(d82d023)_

---

### Changed in TUI

#### 🧩 Features

* integrate BumpAction for branch name suggestion in bump workflow   _(9935115)_

* refactor branch name suggestion to use a structured request for improved clarity   _(f643993)_

---

### 🛠️ General:

### 🧩 Features

* `cg merge #<#>` direct merge call, mostly for internal use   _(da12c74)_

### 🐛 Fix(es)

* fixes `bump` option 5 being unable to publish new branch   _(df45395)_

* added checks and validations for `merge` and `pr` flows   _(f2b4467)_

### 🔧 Maintenance

* CG app version bump to v0.15.0   _(168af46)_

* CG app version bump to v0.15.1   _(c51c879)_

* CG app version bump to v0.15.2   _(a1bd25c)_

* CG app version bump to v0.15.1   _(f512b1d)_

* CG app version bump to v0.15.3   _(abfae76)_

* CG app version bump to v0.15.4   _(9957b85)_

* CG app version bump to v0.15.5   _(9655deb)_

* CG app version bump to v0.16.0   _(06a557f)_

* CG app version bump to v0.16.1   _(e49a51c)_

* CG app version bump to v0.16.2   _(e0a27c2)_

* CG app version bump to v0.16.3   _(6478532)_

* CG app version bump to v0.16.4   _(f9f4347)_

* CG app version bump to v0.16.5   _(61bb1f3)_

### 📝 Other

* Merge remote-tracking branch 'origin/0.15.x' into v0-15-2-dev   _(19278a4)_

* Merge pull request #52 from comfy-home/0.15.x   _(b6de02b)_

<br>

---

## Changelog v0.14.2
2026-04-25

#### What's changed:

### Changed in CLI

#### 🐛 Fix(es)

* Linux → fixed bad render of `cg br <action> 4/5` interactive menu   _(251cf87)_

* Windows → fixed not working `cg cd <alias>` in CMD (command prompt)   _(3544d8b)_

#### 💎 Enhancements

* added colors to render of `cg br <action> 4/5` interactive menu   _(251cf87)_

---

### Changed in `cg pr`

#### 🐛 Fix(es)

Two modifications:   _(924c379)_
* fixed cursor bug in CLI editor mode
* added CLI buttons instead of `ctrl+E` to save due to poor shortcut management of some dev apps (eg VSCode)

#### 💎 Enhancements

Complete refactor of Pull Request creation via CLI:   _(5f958fb)_
* Before:
  * `cg pr`
  * 5-second preview
  * done
* Now:
  * `cg pr`
  * 30-second preview
  * pause preview to edit with `E`
  * cancel 30-second preview timer with ENTER at any time to proceed

<sup>💡 >> Enjoy!</sup>

added third button to CLI, so there are:   _(9a82a24)_
* <Save Changes> - guess what it does :)
* <Discard Changes> - ditto
* <Terminate> - kills the whole PR flow

---

### 🛠️ General:

### 🧩 Features

* enhance shell integration installation script with user and global support   _(efd42b3)_

### 🔧 Maintenance

* CG app version bump to v0.14.0   _(7c0441c)_

* CG app version bump to v0.14.1   _(cca809a)_

* CG app version bump to v0.14.2   _(6e19274)_

### 📝 Other

* Merge pull request #45 from comfy-home/v0.14.1-dev   _(7133eab)_

* update the changelog heading from "What's changed" to "What's new"   _(aa5d703)_

* Merge pull request #46 from comfy-home/v0.14.2-dev   _(b83f09c)_

* Merge pull request #47 from comfy-home/0.14.x   _(bcafaba)_

<br>

---

## Changelog v0.13.2
2026-04-24

#### What's changed:

### ✨ New Feature: Pull Requests via CLI

Brand new `cg pr` CLI command:   _(18a9303)_
* Fully automated
* Aware of ancestory
* With automated context parsing as PR changelog into PR description

<sup>💡 >> alpha implementation - needs further testing</sup>

---

### ✨ New in `cg pr`:

#### 💎 Enhancements

* added a proper `create` to existing `dry run`   _(180ece2)_

---

### Changed in Git

#### 🐛 Fix(es)

attempted fix to ancestory detection, outcome:   _(0b8f811)_
* System is able to detect all ancestors now
* System retained the original over 60% speed boost against alpha after this upgrade
* System can't determine the correct point of origin yet

<sup>💡 >> Needs more work...</sup>

attempted fix to point of origin detection:   _(c611a8c)_
* System now can determine PoO
  * Upsides:
    * correct execution (as expected) of `cg br ..` and `cg pr`
  * Downsides:
    * Execution of `cg br` is much slower and can take over 10 seconds due to extensive validations
* Known bugs:
  * `cg br` renders +1 branch as merged when current possition is 0 and main is located at -1

<sup>💡 >> Still needs testin but it appers that speed has to be sacrificed to make it work properly unless Git updates its logic and defines branches as more than just pointers and allows relations between them and commits. In theory this can be implemented within ComfyGit, but much later due to complexity</sup>

---

### Changed in Changelog Ordering Logic

#### 💎 Enhancements

Ordering now follows new rules:   _(bafc628)_
* ordering within one group changed to oldest→latest
  * this should help end-users with context understanding
* Ordering by category/modifier is now:
  * Release notes
  * `!` Breaking Change
  * `@.` New announcement
  * `@` New in...
  * `...()` "Changed in", eg `enh(Login Flow)`
  * `...` "Standard", eg `fix`, or `feat`

---

### Changed in Issue #40

#### 🐛 Fix(es)

attempted fix for #40:   _(90a9e2a)_
* confirmed resolution when repeated reproducing steps
* confirmed fix for win and pwsh 7.6.1

<sup>💡 >> not closing the issue yet, has to be tested at least on one more platform</sup>

---

### 🛠️ General:

### 🔧 Maintenance

* CG app version bump to v0.13.0   _(158db05)_

* CG app version bump to v0.13.1   _(33b0aac)_

* CG app version bump to v0.13.2   _(2bb2055)_

### 📝 Other

* Merge branch '0.13.x' into v0.13.1-dev   _(03a81d2)_

* Revert "Merge branch '0.13.x' into v0.13.1-dev"   _(35953ca)_

* Merge pull request #41 from comfy-home/v0.13.1-dev   _(4b86966)_

* Merge pull request #42 from comfy-home/v0.13.2-dev   _(70e5887)_

* Merge pull request #43 from comfy-home/0.13.x   _(436586f)_

<br>

---

## Changelog v0.12.4
2026-04-23

#### What's changed:

### ✨ New Feature: Branch Auto-Name

New feature implemented, included in:   _(7f9682d)_
* CLI
* TUI

<sup>💡 >> This is alpha implementation that needs to be properly tested in dev env and prod</sup>

---

### ✨ New Enhancement: Paragraph & Indentation Parser

Makes the following changelog format possible:   _(005b166)_
* This is major element
  * This indented sub-information for it
    * This is double-sub indented info
  * One more sub-info
* Another major
  * With this sub-info

<sup>💡 >> This is end message to sum it up</sup>

---

### ✨ New in `cg branch`:

#### 💎 Enhancements

Improvements:   _(d143100)_
* Speed → `cg br` is now about 66% faster than before
* This significantly improves all dependant functions such as `cg br ..`
* enjoy!

---

### ✨ New in New CLI commands:

#### 🧩 Features

Added:   _(f0df64c)_
* `cg branch up`
* `cg branch main`

---

### ✨ New in Bumping via TUI and CLI:

#### 💎 Enhancements

Enhanced with:   _(96b5b80)_
* Verifications and alternate flow in case of verification failure
* New tile bump modal in case of verification failure
* 5th, local bump option without push

---

### ✨ New Feature: `cg commit rename`

* code cleanup   _(c590eea)_

* mouse selection handling in TUI - click-through fix   _(d2839a0)_

* mouse selection handling in TUI for wrapped messages   _(def5a79)_

* enhance warning TUI message for renaming commits with detailed conditions   _(c801f34)_

* enhance CLI warning message for renaming commits with detailed conditions   _(0e87215)_

* add selection and scrolling functionality to RecentChanges dialog   _(fd2b795)_

* enhance recent changes display with selection and scrolling functionality for rn function   _(614a58a)_

* add commit rename dialog rendering functionality for TUI   _(90e276c)_

* key handling for rename function   _(2603f87)_

* Commit rename functionality with user prompts added to CLI and TUI   _(1283025)_

---

### Changed in Paragraph & Indentation Parser

#### 🐛 Fix(es)

* minor adjustments   _(5d8b9bb)_

---

### 🛠️ General:

### 🐛 Fix(es)

* fixes not working `ctrl+c` interruptions in CLI commands   _(d143100)_

### 🔧 Maintenance

* CG app version bump to v0.12.4   _(74c04af)_

* CG app version bump to v0.12.3   _(5ba7477)_

* dep update `reqwest` 0.12→0.13   _(6d2396c)_

* CG app version bump to v0.12.2   _(3ee0dfb)_

* CG app version bump to v0.12.1   _(da71ca6)_

* CG app version bump to v0.12.0   _(94ea405)_

### 📝 Other

* Merge pull request #35 from comfy-home/v0.12.x   _(02d8f0a)_

* Merge pull request #34 from comfy-home/v0.12.4-dev   _(7433646)_

* Merge pull request #33 from comfy-home/v0.12.3-dev   _(8f40340)_

* Merge pull request #32 from comfy-home/v0.12.2-dev   _(98328d7)_

* comfy-home/perk-ComfyVersionBumper into v0.12.x   _(29a4fa3)_

* Merge pull request #31 from comfy-home/v0.12.1-dev   _(b28f84d)_

* CLI help modifications   _(88d7e6d)_

* CLI help modifications   _(b09e1ec)_

<br>

---

## Changelog v0.11.3
2026-04-22

#### What's changed:

### 🔧 Maintenance

* CG app version bump to v0.11.3   _(85c5c41)_

<br>

---

## Changelog v0.11.2
2026-04-22

#### What's changed:

### Changed in Previous Release in rls-changelog

#### 🐛 Fix(es)

* refactor→ previous public release in changelog header   _(bf32735)_

* refactor→ replace manual GitHub release query with crate function for latest public release tag   _(a8de61d)_

---

### 🛠️ General:

### 🔧 Maintenance

* CG app version bump to v0.11.2   _(857da06)_

* update changelog for version 0.10.11 and sync metadata   _(ec6b60d)_

### 📝 Other

* Merge pull request #30 from comfy-home/v0.11.x   _(83861cb)_

* Merge pull request #29 from comfy-home/v0.11.2   _(af7b774)_

<br>

---

## Changelog 0.10.11
2026-04-21

#### What's changed:

### Changed in CLI

#### 💎 Enhancements

* `cg branch` improved with sequential rendering   _(d366eb8)_

---

### Changed in TUI

#### 💎 Enhancements

* code cleanup   _(8f09f0b)_

* add variable height to bump popups and better render logic   _(1e9964a)_

* update branch bump dialog title and increase popup height   _(f42b2e4)_
* add scrolling functionality   _(f42b2e4)_

---

### 🛠️ General:

### 🔧 Maintenance

* CG version bump to 0.10.11   _(95b8eca)_

### 💎 Enhancements

* minor wording adjustments in commits and changlogs   _(acd0c5c)_

### 📝 Other

* Merge pull request #27 from comfy-home/v0.10.11+   _(fbc3726)_

<br>

---

## Changelog v0.10.10
2026-04-20

#### What's changed:

### ✨ New Feature: Custom `main` Branch Name

* From now on, ComfyGit recognises 3 main forms:   _(4185baa)_
  * `main`
  * `master`
  * `custom_main` → set in "Project Settings/General"

---

### ✨ New Enhancement: New CLI commands

* Added support for the following:   _(2dd811f)_
  * `cg branch` → prints the current branch plus a compact ASCII branch tree. Synonyms = `br`, `brn`, and `brnch`
  * `cg v <alias>` → prints Project Name, Current Version, Last Bump, and Last Release for the matched project.

---

### Changed in ReleaseNOW!

#### 💎 Enhancements

* improve stream line handling and add unit test for line collection   _(fb8956c)_

---

### Changed in `cg branch` CLI command

#### 🐛 Fix(es)

* fixes tree rendering   _(cfc33b9)_

#### 💎 Enhancements

* implemented timeline awareness   _(19780c6)_

* refactor branch topology to branch diagram structure for improved rendering   _(3a84c70)_

---

### Changed in Custom `main` Branch Name

#### 🧩 Features

* propagate default RepoConfig values for custom main branch support   _(9a51239)_

#### 🐛 Fix(es)

* clarify changelog generation message to refer to repo's mainline   _(a0b3260)_

#### 💎 Enhancements

* improve main branch resolution and switching logic to support custom main branch names   _(3188547)_

* integrate main branch name handling in repo state collection and switching   _(a0535e0)_

* `is_mainline_branch`  refactor to support custom main branch names in changelog decisions   _(a5c1bc6)_

---

### Changed in `cg bmp <action> 4` CLI command

#### 💎 Enhancements

* improve non-main branch warning and add user confirmation prompt   _(4fc32c4)_

* add a branch-check into CLI env   _(7616bdf)_

---

### General Improvements:

### 🔧 Maintenance

* CG version bump to 0.10.10   _(f812d70)_

### 🧪 Tests

* refactor branch handling to use topology structure for improved branch status rendering   _(b12e0b5)_

### 📝 Other

* Merge pull request #26 from comfy-home/0.10.8+   _(49fb8b8)_

* Merge pull request #25 from comfy-home/0.10.10   _(5b51a67)_

<br>

---

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

<br>

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

<br>

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

<br>

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



---
... ✨ made with [ComfyGit](https://github.com/comfy-home/ComfyGit)