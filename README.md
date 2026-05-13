<p align="center">
  <img src="https://github.com/comfy-home/ComfyGit/blob/main/assets/docs-assets/cg-header.png" width="768" alt="ComfyGit Header">
</p>
<p align="center">
  <strong>⭐ ⭐ ⭐ &nbsp&nbspRatatui-powered Git workflow automation with custom CLI &nbsp&nbsp⭐ ⭐ ⭐</strong><br><br><sup>inc.</sup><br><br><sup>Built-in Multi-Project Management&nbsp&nbsp&nbsp&nbsp&nbsp&nbsp |&nbsp&nbsp&nbsp&nbsp&nbsp&nbsp Automatic Version Bumper&nbsp&nbsp&nbsp&nbsp&nbsp&nbsp |&nbsp&nbsp&nbsp&nbsp&nbsp&nbsp Advanced Changelog Generator</sup><br>
</p>
<br>
<p align="center">
  <a href="https://github.com/comfy-home/ComfyGit/releases"><img src="https://img.shields.io/github/v/release/comfy-home/ComfyGit?style=plastic" alt="GitHub Release"></a>&nbsp&nbsp&nbsp&nbsp&nbsp
  <a href="https://github.com/comfy-home/ComfyGit/blob/main/LICENSE.md"><img src="https://img.shields.io/badge/license-SA--PS-blue?style=plastic" alt="License: SA-PS"></a>&nbsp&nbsp&nbsp&nbsp&nbsp
  <a href="https://github.com/comfy-home/ComfyGit"><img src="https://img.shields.io/badge/Rust-2024%20Edition-orange?style=plastic&logo=rust" alt="Rust 2024 Edition"></a>
</p>

---

<details>
<summary>A word from the developer...</summary>
<br>
Hi all 👋
<br>  
I'm Tom, lead developer here at ComfyHome — also known as “the guy who keeps accidentally starting new projects”.

We’ve got a bunch of exciting things coming later in 2026, but before all that… let’s talk about ComfyGit.

Before I dive into the technical wizardry, let me tell you how this whole thing accidentally happened.

It all started with a friendly chat with two newbie developer friends. They asked me what I use for project management and versioning — you know, normal questions.
Unfortunately, my answer was… “Uh… a weird little custom mini‑app I hardcoded years ago and never spoke of again.”
Not exactly shareable.
So I told them, “Maybe I’ll look into it one day.”

Fast‑forward a couple of weeks: I needed a break from our main project, so I thought, “Hey, I’ll just port that mini‑app. Two or three days, tops.”
Yeah. Two to three days.
You already know where this is going 🤦‍♂️
Another rabbit hole. A deep one.

Step one was research. As always.
To my surprise, I couldn’t find anything even remotely close to what our mini‑app did. Ideally, I’d just send my friends a couple of links and call it a day.
But no. The universe said: “Build it yourself.”

So here we are — roughly 450 work‑hours later — and I’m releasing ComfyGit v0.26 as a Public Release Beta.

As far as I’m concerned, ComfyGit is a unique tool. I’m pretty sure some of its features don’t exist anywhere else — and there’s plenty more coming. 
Out of all, I'll mention `cg cd <alias>`. A very simple feature that completely changed my workflow.

Use just the bits you need, or adopt it as a full workflow companion. Totally your call.

Honestly… I really hope that now that I’m releasing it, nobody shows up saying, “Hey bro, you suck at research — here’s a tool that already does it all.”
If that happens, I’ll simply pretend I didn’t see the message and go lie face‑down on the nearest carpet for a bit.

There are definitely still some kinks to iron out in ComfyGit, so if you’ve got feedback, I’d love to hear it. I promise, I'll look into any meaningful feedback... 

Feature‑wise, ComfyGit is about 60% of what I want it to be for v1.0 — which means the fun is just getting started!

Enjoy!

</details>

---

<details><summary>👀 What's new in v0.26.0 - The first ever public release!</summary>

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
- Fixed bug that termianted ReleaseNOW flow with auto-inject enabled
- Several consolidation pre-release fixes

<sub>...  🎉 Enjoy!</sub>

<br>

---
<sup>... ✨ auto-injected by [ComfyGit](https://github.com/comfy-home/ComfyGit)</sup>

---

</details>


---

<details><summary>📜 Table of Contents</summary>
  
## Table of Contents

- [Table of Contents](#table-of-contents)
- [Overview](#overview)
- [Features](#features)
  - [🎯 Core Capabilities](#-core-capabilities)
  - [🚀 Advanced Features](#-advanced-features)
- [Installation](#installation)
  - [From Release Page (Recommended)](#from-release-page-recommended)
    - [Windows](#windows)
  - [Alternatively via CLI](#alternatively-via-cli)
    - [RPM (Fedora, SUSE, etc.)](#rpm-fedora-suse-etc)
    - [DEB (Ubuntu, Debian, etc.)](#deb-ubuntu-debian-etc)
    - [AppImage](#appimage)
    - [Cargo Install](#cargo-install)
  - [Shell Integration](#shell-integration)
- [Update](#update)
- [Quick Start](#quick-start)
  - [1. Launch the TUI](#1-launch-the-tui)
  - [2. Add Your First Project](#2-add-your-first-project)
  - [3. Set an Alias (Optional but STRONGLY Recommended)](#3-set-an-alias-optional-but-strongly-recommended)
  - [4. Basic Version Bump](#4-basic-version-bump)
  - [5. Generate Changelog](#5-generate-changelog)
- [CLI Reference](#cli-reference)
  - [General Commands](#general-commands)
  - [Branching Commands](#branching-commands)
  - [Version Bumping](#version-bumping)
  - [Commit Management](#commit-management)
- [TUI Overview](#tui-overview)
  - [Dashboard](#dashboard)
  - [Project Settings](#project-settings)
    - [General Tab](#general-tab)
    - [Distro Tab](#distro-tab)
    - [RlsQd Tab (Release QuickDownloads)](#rlsqd-tab-release-quickdownloads)
  - [Branch Workflows](#branch-workflows)
- [Project Configuration](#project-configuration)
  - [Project Types](#project-types)
    - [All-in-One](#all-in-one)
    - [Branched](#branched)
  - [Version Schemes](#version-schemes)
  - [Changelog Settings](#changelog-settings)
- [Changelog Features](#changelog-features)
  - [Standard Changelog](#standard-changelog)
  - [ReleaseNOW Changelog](#releasenow-changelog)
  - [Top Picks](#top-picks)
- [Branching Workflows](#branching-workflows)
  - [Branch Tree Visualization](#branch-tree-visualization)
  - [Workflow: Start New Feature](#workflow-start-new-feature)
  - [Syncing with Parent](#syncing-with-parent)
  - [Switching Branches](#switching-branches)
- [Integration Modes](#integration-modes)
- [Tips \& Best Practices](#tips--best-practices)
  - [1. Use Aliases for Quick Navigation](#1-use-aliases-for-quick-navigation)
  - [2. Conventional Commits for Better Changelogs](#2-conventional-commits-for-better-changelogs)
  - [3. Branched Projects for Monorepos](#3-branched-projects-for-monorepos)
  - [4. Shell Integration for Power Users](#4-shell-integration-for-power-users)
  - [5. Variator Storage for Automation](#5-variator-storage-for-automation)
  - [6. Hide Unwanted Commits](#6-hide-unwanted-commits)
- [Configuration File](#configuration-file)
- [License](#license)
- [Support](#support)

</details>

---

## Overview

**ComfyGit** is a powerful Terminal User Interface (TUI) and CLI tool designed to streamline Git workflows, automate version management, and generate beautiful changelogs. Built with [Ratatui](https://github.com/ratatui/ratatui) in Rust, it provides an intuitive visual interface while maintaining full CLI accessibility.

Whether you're managing a simple single-package project or a complex multi-scope monorepo, ComfyGit adapts to your workflow with flexible project configurations and intelligent automation.

---

## Features

### 🎯 Core Capabilities

- **🖥️ Interactive TUI** — Beautiful terminal interface with mouse support, keyboard navigation, and real-time previews
- **📦 Version Management** — Automated SemVer/CalVer bumping with target file updates (Cargo.toml, package.json, etc.)
- **🌳 Smart Branching** — Visual branch trees, parent/child navigation, and guided merge workflows
- **📝 Changelog Generation** — Multiple formats: Standard, ReleaseNOW (with QuickDownloads), and Top Picks
- **🔧 GitHub Integration** — PR creation, merge management, and release automation


### 🚀 Advanced Features

- **Multi-Scope Projects** — Manage branched projects with different version schemes per scope
- **Shell Integration** — `cg cd <alias>` command to jump to any project from anywhere
- **Commit Management** — Rename and safely delete commits with revert automation
- **Variator Storage** — Custom key-value storage per project for automation scripts
- **ReleaseNOW** — Generate release-ready changelogs with QuickDownloads integration
- **Top Picks** — Highlight significant improvements in a dedicated release summary section

>[!TIP]
> Make sure to visit [ComfyGit's WIKI](https://github.com/comfy-home/ComfyGit/wiki)
> 
> We’re rolling out lots of deep‑info educational CG usage scenarios with full step‑by‑step breakdowns — and some are already live!

---

## Installation

> [!NOTE]
> ComfyGit will be soon available via Distro channels like `flatpak` and similar.
>
> To be announced when it happens...

### From Release Page (Recommended)

1. Download the latest installation package from [GitHub Releases](https://github.com/comfy-home/ComfyGit/releases)
2. Install via `apt`, `dnf`, or whatever applies to your OS.

#### Windows

1. Just download the MSI from [GitHub Releases](https://github.com/comfy-home/ComfyGit/releases)
2. Double-click to install
3. Done

### Alternatively via CLI

#### RPM (Fedora, SUSE, etc.)

```bash
# Download (replace <> with actual version and architecture)
wget https://github.com/comfy-home/ComfyGit/releases/latest/ComfyGit-<version>-<architecture>.rpm
chmod +x ComfyGit-<version>-<architecture>.rpm
sudo dnf install ./ComfyGit-<version>-<architecture>.rpm
```

#### DEB (Ubuntu, Debian, etc.)

```bash
# Download (replace <> with actual version and architecture)
wget https://github.com/comfy-home/ComfyGit/releases/latest/ComfyGit-<version>-<architecture>.deb
chmod +x ComfyGit-<version>-<architecture>.deb
sudo apt install ./ComfyGit-<version>-<architecture>.deb
```

#### AppImage

> [!WARNING]
> Unlike `deb`, `rpm`, or `msi`, AppImage requires a manual shell integration installation.
> 
> I think all devs understand why...

```bash
# Download (replace <> with actual version and architecture)
wget https://github.com/comfy-home/ComfyGit/releases/latest/ComfyGit-<version>-<architecture>.AppImage
chmod +x ComfyGit-<version>-<architecture>.AppImage

# Install shell integration
./ComfyGit-<version>-<architecture>.AppImage install-shell

# Or run directly
./ComfyGit-<version>-<architecture>.AppImage
```

#### Cargo Install

```bash
cargo install --git https://github.com/comfy-home/ComfyGit

# Install shell integration
ComfyGit install-shell
```

### Shell Integration

Shell integration enables the `cg` command and the powerful `cg cd <alias>` feature:

```bash
# Install for current user
ComfyGit install-shell

# Uninstall
ComfyGit uninstall-shell
```

Supported shells:
- Bash
- Zsh
- Fish
- PowerShell (pwsh)

After installation, open a new shell session or run:
```bash
source ~/.config/comfygit/cg.sh  # bash/zsh
```

## Update
- Simply download the new installation package and reinstall

>[!NOTE]
> Auto-update feature to be implemented very soon!

---

## Quick Start

### 1. Launch the TUI

```bash
cg
# or
comfygit
```

### 2. Add Your First Project

In the TUI:
1. Navigate to **Projects** tab
2. Press `n` or click **New Project**
3. Follow the wizard to configure your project

### 3. Set an Alias (Optional but STRONGLY Recommended)

1. Select your project → **Project Settings** → **General**
2. Set an **Alias** (e.g., "myapp")
3. Now you can use: `cg cd myapp`

### 4. Basic Version Bump

```bash
cd /path/to/your/project
cg bmp minor

# or if you already configure your alias:
cg cd <alias>
cg bmp .. #`..` is synonym for minor

```

### 5. Generate Changelog

In the TUI Overview, select your project and press `C` to **Generate Changelog** or use the ReleaseNOW workflow via **RLS** tile button or `R` shortcut.

---

## CLI Reference

### General Commands

| Command | Description |
|---------|-------------|
| `cg` / `comfygit` | Launch interactive TUI |
| `cg -v` / `--version` | Show version and GitHub update status |
| `cg pwd <alias>` | Print configured project root path |
| `cg pwd -all` | Print all configured repo roots |
| `cg cd <alias>` | Change directory to project root (requires shell integration) |
| `cg install-shell` | Install bash/zsh/fish/pwsh integration |
| `cg uninstall-shell` | Remove shell integration |
| `cg v <alias>` | Show project version, last bump, and release info |

### Branching Commands

| Command | Description |
|---------|-------------|
| `cg branch` | Show current branch and compact branch tree |
| `cg branch up` / `cg branch ..` | Switch to parent branch |
| `cg branch main` / `cg branch ~` | Switch to main/master/custom main |
| `cg branch done` | Create PR, merge it, switch to target, and sync |
| `cg branch cd` | Interactively choose and switch to a recent branch |
| `cg pr` | Generate PR title/body for current branch |
| `cg pr --main` | Generate PR against main/master |
| `cg merge` | Interactively merge an open PR |
| `cg merge #67` | Merge PR #67 directly |
| `cg reroot` | Merge selected source branch into current non-main branch |
| `cg reroot rebase` | Rebase current branch onto selected source |

### Version Bumping

| Command | Description |
|---------|-------------|
| `cg bmp major` / `cg bmp .` | Bump major version |
| `cg bmp minor` / `cg bmp ..` | Bump minor version |
| `cg bmp patch` / `cg bmp ...` | Bump patch version |
| `cg bmp auto` | Auto-determine bump type from commits |
| `cg bmp cal` | Bump using CalVer scheme |

**With Options:**
| `cg bmp <action> <option>` | Description |
|----------------------------|-------------|
| `1` | Just bump version |
| `2` | Bump & Commit (local) |
| `3` | Bump & Commit & Push |
| `4` | Branch & Bump & Commit (prompts for branch) |
| `5` | Branch & Bump & Commit & Push |

### Commit Management

| Command | Description |
|---------|-------------|
| `cg commit del <hash>` | Safely revert and push a published commit |
| `cg commit rename <target>` | Rename commit (hash or HEAD offset) |

**Aliases:**
- `commit`: `cmt`, `com`, `ct`
- `del`: `rm`, `rem`, `delete`, `drop`, `erase`
- `rename`: `rn`, `rnm`, `reword`, `rwrd`, `rwd`

---

## TUI Overview

### Dashboard

The main dashboard provides an overview of all configured projects with:
- Project tiles showing version, scheme, and status
- Quick actions via keyboard shortcuts or mouse
- Branch trees for complex projects
- Changelog preview integration

**Key Shortcuts:**
- `↑`/`↓` or `j`/`k` — Navigate projects/scopes
- `Tab` — Cycle through tabs (Overview, Projects, Settings)
- `Enter` — Activate selected item
- `q` — Quit
- `?` — Help

### Project Settings

Access via **Projects** → **Project Settings** (or press `i` on a project).

#### General Tab
- **Alias** — Short name for `cg cd <alias>`
- **Custom Main Branch** — Override default main branch name
- **Changelog Settings** — Enable/configure changelog generation
- **Mini Commit Hashes** — Compact hash display in changelogs

#### Distro Tab
- **ReleaseNOW** — Enable release automation scripts per platform
- Configure Windows, Linux ARM, Linux AMD, and macOS release scripts

#### RlsQd Tab (Release QuickDownloads)
- Enable QuickDownloads integration
- Position (Top/Bottom) and footer message customization

### Branch Workflows

ComfyGit provides guided workflows for common branching operations:

1. **New Branch** (`n`) — Create feature branch with optional bump
2. **Commit & Push** (`p`) — Commit changes and push to remote
3. **Pull Request** — Create PR with generated title/body
4. **Merge** — Merge PR with upstream sync
5. **Done** — Complete workflow: PR → Merge → Switch → Sync

---

## Project Configuration

### Project Types

#### All-in-One
Single codebase with unified versioning. Ideal for:
- Single-package projects
- Simple applications
- Libraries with one version source

**Configuration:**
```toml
project_type = "all_in_one"
version_scheme = "SemVer"
targets = [{ path = "Cargo.toml", key_path = "package.version", format = "Toml" }]
```

#### Branched
Multi-scope projects with independent versioning per branch. Ideal for:
- Monorepos with multiple services
- Projects with separate frontend/backend versions
- Microservices with independent release cycles

**Configuration:**
```toml
project_type = "branched"
unified_versioning = false  # or true for shared version

[[branches]]
name = "core"
label = "Core"
scope_kind = "Module"  # or "Branch", "Service"
version_scheme = "SemVer"
changelog_enabled = true
changelog_path = "CHANGELOG.md"
```

### Version Schemes

| Scheme | Description | Example |
|--------|-------------|---------|
| **SemVer** | Semantic Versioning (MAJOR.MINOR.PATCH) | `1.2.3` → `1.3.0` (minor bump) |
| **CalVer** | Calendar Versioning (YYYY.MM.MICRO) | `2026.05.1` → `2026.05.2` |

**CalVer Variants:**
- `YYYY.MM.MICRO` — Year, Month, Micro
- `YYYY.0M.0D` — Year, Zero-padded Month, Day
- `YYYY.MM.MINOR.MICRO` — Hybrid with minor/micro

### Changelog Settings

```toml
[changelog]
enabled = true
file_path = "CHANGELOG.md"
hide_pr_messages = false
hide_bump_messages = false
mini_commit_hashes = false
```

---

## Changelog Features

ComfyGit supports multiple changelog formats with different use cases:

### Standard Changelog

Generated from conventional commits with categorized sections:

```markdown
## Changelog `v1.2.0` <sup><div align="end">🗓️ 2026-05-11</div></sup>

### 🧩 Features
* Added user authentication _(#a1b2c3d)_
* Implemented dark mode _(#e4f5g6h)_

### 🐛 Fix(es)
* Resolved memory leak in parser _(#i7j8k9l)_

---
... ✨ made with [ComfyGit](https://github.com/comfy-home/ComfyGit)
```

**Commit Categories:**
- `feat:` / `ft:` → 🧩 Features
- `fix:` / `bf:` → 🐛 Fix(es)
- `enh:` / `imp:` → 💎 Enhancements
- `docs:` / `dox:` → ℹ️ Documentation
- `ui:` / `gui:` → 📱 UI Changes
- `style:` / `vis:` → 🎨 Visuals
- `ref:` / `refactor:` → ♻️ Refactor
- `perf:` / `opt:` → 🚀 Performance
- `test:` / `tst:` → 🧪 Tests
- `chore:` / `dep:` → 🔧 Maintenance
- `build:` / `rls:` → 📦 Build
- `broken:` / `brk:` → ⛓️‍💥 Not Working Yet

**Special Prefixes:**
- `!` — Breaking change (appears in 💥 Breaking Changes section)
- `@` — New feature announcement (appears before categories)
- `@.` — Dotted new announcement (higher priority)
- `~` — Ignored (excluded from changelog)

### ReleaseNOW Changelog

Extended changelog format for GitHub releases with optional QuickDownloads integration.

**Features:**
- Previous public release reference
- QuickDownloads section (Top or Bottom position)
- Footer customization
- Markdown formatting optimized for GitHub

### Top Picks

A dedicated section for highlighting the most significant improvements in a release.

**Syntax in commit messages:**
```bash
# Priority 1-20 (top1 = highest position)
git commit -m "top3: *Included bugfix you've been waiting for... **Sorts these on Linux: ***Failed autostart"

# Without priority (appears after prioritized items, sorted alphabetically)
git commit -m "feat: New feature; top: *Another important improvement"
```

**Formatting Rules:**
- `*` — Header (required)
- `**` — First-level bullet
- `***` — Second-level bullet (nested)

**Example Output:**
```markdown
### 💥 💥 💥 This Release's Top Picks ...  💥 💥 💥

#### **1.    Included bugfix you all have been waiting for...**
- Sorts these on Linux:
    - Failed autostart
    - Bad render
- Sorts this on macOS
    - Failed autostart

#### **2.    This is the third big improvement**

<sub>...  🎉 Enjoy!</sub>

<br>
```

**Important:** Top Picks configuration commits don't appear in the standard changelog—only in the Top Picks section.

---

## Branching Workflows

### Branch Tree Visualization

ComfyGit displays your branch structure visually:

```
main ─┬─ feature/auth ─┬─ feature/oauth
      │                └─ feature/2fa
      ├─ feature/api
      └─ fix/memory-leak
```

### Workflow: Start New Feature

1. **Create Branch:**
   ```bash
   cg new
   # or in TUI: press 'n'
   ```

2. **Make Changes & Commit:**
   ```bash
   # edit files...
   cg commit  # or use your regular git workflow
   ```

3. **Create PR:**
   ```bash
   cg pr
   ```

4. **Complete (Merge & Cleanup):**
   ```bash
   cg branch done
   ```

### Syncing with Parent

```bash
# Merge parent into current branch
cg reroot

## or use synonym:

cg rrt

# Or rebase instead

cg reroot rebase

## or use synonym:

cg rrt rbs
```

### Switching Branches

```bash
# Interactive branch switcher
cg branch cd

## or use synonym:

cg br cd

# Jump to parent
cg branch up

## or use synonym:

cg br ..

# Jump to main

cg branch main

## or use synonym:

cg br main

```

---

## Integration Modes

ComfyGit supports different levels of Git integration:

| Mode | Description | Use Case |
|------|-------------|----------|
| **GitLocalOnly** | Local git operations only | Simple projects, no GitHub |
| **GitHubEnabled** | Full GitHub integration | PRs, merges, releases |
| **GitLabEnabled** | GitLab integration (planned) | GitLab workflows |

Configure in Project Settings or directly in config:
```toml
integration_mode = "GitHubEnabled"
```

---

## Tips & Best Practices

### 1. Use Aliases for Quick Navigation

Set short, memorable aliases for frequently accessed projects:
- `myapp` → `/home/user/projects/my-application`
- `api` → `/home/user/work/api-service`

Then jump instantly from anywhere via:
```bash
cg cd myapp
```

### 2. Conventional Commits for Better Changelogs

Structure commits for automatic categorization:
```bash
git commit -m "feat(auth): add OAuth2 provider support"
git commit -m "fix(parser): resolve edge case with nested arrays"
git commit -m "!breaking: change default API response format"
```

### 3. Branched Projects for Monorepos

For repositories with multiple independently-versioned components:

```toml
project_type = "branched"

[[branches]]
name = "frontend"
label = "Frontend"
version_scheme = "SemVer"

[[branches]]
name = "backend"
label = "Backend API"
version_scheme = "SemVer"
```

### 4. Shell Integration for Power Users

Add to your shell profile for enhanced productivity:

```bash
# Bash/Zsh: Enable shell integration
eval "$(cg install-shell --print)"

# Fish
source (cg install-shell --print | psub)
```

### 5. Variator Storage for Automation

Store commit message definitions, and call them out via simple `(!{})`

Store custom data for CI/CD scripts:
```bash
# Set a value
cg var set deploy-target production

# Retrieve in scripts
DEPLOY_TARGET=$(cg var get deploy-target)
```

### 6. Hide Unwanted Commits

Use the `~` prefix for commits that shouldn't appear in changelogs:
```bash
git commit -m "~chore: update internal dependencies"
```

---

## Configuration File

ComfyGit stores configuration in platform-appropriate locations:

| Platform | Path |
|----------|------|
| Linux | `~/.config/comfygit/config.toml` |
| macOS | `~/Library/Application Support/comfygit/config.toml` |
| Windows | `%APPDATA%\comfygit\config\config.toml` |

**Example Configuration:**
```toml
schema_version = 4

[ui]
accent_color = "cyan"
show_mouse_hints = true
show_tab_hints = true

[[projects]]
name = "My Application"
alias = "myapp"
project_type = "all_in_one"
integration_mode = "GitHubEnabled"
version_scheme = "SemVer"

[projects.changelog]
enabled = true
file_path = "CHANGELOG.md"
hide_pr_messages = true
hide_bump_messages = true
mini_commit_hashes = false

[[projects.targets]]
label = "Version"
path = "Cargo.toml"
key_path = "package.version"
format = "Toml"
```

---

## License

ComfyGit is licensed under the **Source-Available - Protected Source License (SA-PS)**.

- ✅ **Free for personal use**
- 💼 **Commercial license required** for larger teams

See [LICENSE.md](LICENSE.md) for full terms.

---

## Support

- 📧 **Email:** support@comfyhome.io | dev@comfyhome.io
- 🐛 **Issues:** [GitHub Issues](https://github.com/comfy-home/ComfyGit/issues)
- 💬 **Discussions:** [GitHub Discussions](https://github.com/comfy-home/ComfyGit/discussions)
- 🏠 **Homepage:** https://github.com/comfy-home/ComfyGit

---

<p align="center">
  <sub>Made with ❤️ by <a href="https://comfyhome.io">ComfyHome™</a></sub>
</p>

