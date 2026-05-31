# Project Settings — Git

Per-repo Git workflow options for projects that use a remote (not **Local-only**).

## Scope header

**Project/Scope** shows the active overview tile scope (branch/module name for branched projects, or the project name for all-in-one).

## After-merge source branch

| Policy | Effect on `cg merge` |
|--------|----------------------|
| **Kept as is** | Remote source branch is kept (GitLab: `--remove-source-branch=false`). |
| **DELETED (remote)** | Remote source branch is removed after merge (GitLab: `--remove-source-branch=true`; GitHub: `git push <remote> --delete <branch>`). |
| **DELETED (remote+local)** | Same as remote delete, then switches to the merge target branch (when needed) and runs `git branch -d` on the source branch locally. |

Use **← / →** on the policy row to cycle options.

`cg merge` and `cg br end` wait up to three times (5 seconds apart) when the forge reports merge checks still in progress (for example GitLab `checking`), then switch to the merge target branch and `git pull --ff-only` after a successful merge.

## ComfyGitFlow

When **ComfyGitFlow** is disabled in **General**, hint lines point you there. Additional ComfyGitFlow-only Git settings will appear in a later release once ComfyGitFlow is enabled.

## Navigation

| Key | Action |
|-----|--------|
| **[** / **]** | Previous / next settings sub-tab |
| **Tab** / **↑↓** | Move between fields |
