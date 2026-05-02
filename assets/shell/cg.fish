# ComfyGit shell integration for fish — wrap `cg` so `cg cd <alias>` changes directory.

function __comfygit_cli --description 'Resolve ComfyGit executable (PATH, ~/.local/bin, package install)'
    if set -q COMFYGIT_EXE
        and test -x "$COMFYGIT_EXE"
        printf '%s\n' "$COMFYGIT_EXE"
        return 0
    end
    if command -sq ComfyGit
        printf '%s\n' (command -v ComfyGit)
        return 0
    end
    set -l _local "$HOME/.local/bin/ComfyGit"
    if test -x "$_local"
        printf '%s\n' "$_local"
        return 0
    end
    if test -x /usr/local/bin/ComfyGit
        printf '%s\n' /usr/local/bin/ComfyGit
        return 0
    end
    return 1
end

function cg --wraps ComfyGit --description 'ComfyGit launcher (supports cg cd <alias>)'
    set -l _exe (__comfygit_cli)
    if test $status -ne 0
        or test -z "$_exe"
        echo "ComfyGit executable not found. Run 'cg install-shell' (AppImage) or add ~/.local/bin to fish PATH (or set COMFYGIT_EXE)." >&2
        return 127
    end

    if set -q argv[1]
        and test "$argv[1]" = cd
        if test (count $argv) -ne 2
            echo "usage: cg cd <alias>" >&2
            return 2
        end
        set -l target_dir ("$_exe" pwd $argv[2])
        or return
        cd $target_dir
        or return
    else
        "$_exe" $argv
    end
end
