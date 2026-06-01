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

## GitLab+GitHub mirror sync

For **GitLab+GitHub** projects only:

| Policy | Effect |
|--------|--------|
| **Automatically after each merge** (default) | After `cg merge` / `cg br end`, ComfyGit runs `cg sync --yes` when remotes are out of sync. |
| **Manually, or using external service (e.g. GitLab repo mirroring)** | No automatic sync; use `cg sync` when you want to push both remotes. |

`cg sync` reports **in sync** (green) or **out of sync** (red) and accepts `--yes` to push without a confirmation prompt.

## ComfyGitFlow

When **ComfyGitFlow** is disabled in **General**, hint lines point you there. Additional ComfyGitFlow-only Git settings will appear in a later release once ComfyGitFlow is enabled.

## Navigation

| Key | Action |
|-----|--------|
| **[** / **]** | Previous / next settings sub-tab |
| **Tab** / **↑↓** | Move between fields |
