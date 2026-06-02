# Project Settings — Distro

Distribution and packaging targets for the project.

- Entered as a full system path with a flag eg:
 `/home/my/path/scripts/resleaseNOW.sh --win64`

- **General** script is used e.g. for a publish script that does not compile directly, but uses an external runner, not necessary remote (for example `cargo publish`). A general script can push to GitHub, or GitLab; however, it produces only changelog (without QD), and source archive.

## Release title

Use case:
- your project has a long name, e.g. `ratatui-comfy-toaster`
- if you want package to have a shorter name:
    - set "Release title" e.g. to `rct-{version}`
    - now your released package for v1.2.3 will be named "rct-v1.2.3.ext" 


## Navigation

| Key | Action |
|-----|--------|
| **Tab** | Move between fields |
| **[** / **]** | Switch sub-tabs |
| **Ctrl+O** | Browse paths |
| **PgUp / PgDn** | Scroll the form |

Set target formats, output locations, and distro-specific options used during bump and release workflows.
