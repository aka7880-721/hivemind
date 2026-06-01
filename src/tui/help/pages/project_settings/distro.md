# Project Settings — Distro

Distribution and packaging targets for the project.

- Entered as a full system path with a flag eg:
 `/home/my/path/scripts/resleaseNOW.sh --win64`

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
