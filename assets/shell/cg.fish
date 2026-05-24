# ComfyGit shell integration for fish — wrap `cg` so `cg cd <alias> [sub]` changes directory.

# fish often does not put ~/.local/bin on PATH (unlike typical bash setups). AppImage
# `install-shell` places the cg / ComfyGit wrappers there, so prepend it for this session.
set -l _comfygit_local_bin "$HOME/.local/bin"
if test -d "$_comfygit_local_bin"
    contains "$_comfygit_local_bin" $PATH
    or set -gx PATH "$_comfygit_local_bin" $PATH
end

function __comfygit_cli --description 'Resolve ComfyGit executable (PATH, ~/.local/bin, package install)'
    if set -q COMFYGIT_EXE
        and test -x "$COMFYGIT_EXE"
        printf '%s\n' "$COMFYGIT_EXE"
        return 0
    end
    if type -q ComfyGit
        type -p ComfyGit
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

function cg --description 'ComfyGit launcher (supports cg cd <alias> [sub])'
    set -l _exe
    if not set -l _exe (__comfygit_cli)
        echo "ComfyGit executable not found. Run 'cg install-shell' (AppImage) or add ~/.local/bin to fish PATH (or set COMFYGIT_EXE)." >&2
        return 127
    end

    if set -q argv[1]; and test "$argv[1]" = cd
        set -l argc (count $argv)
        set -l target_dir
        switch $argc
            case 2
                set target_dir ("$_exe" pwd $argv[2])
            case 3
                set target_dir ("$_exe" pwd $argv[2] $argv[3])
            case '*'
                echo "usage: cg cd <alias> [sub]" >&2
                return 2
        end
        or return

        set target_dir (string trim -- $target_dir)
        if test -z "$target_dir"
            echo "ComfyGit pwd returned an empty path." >&2
            return 1
        end
        cd -- $target_dir
        or return
    else
        command "$_exe" $argv
    end
end
