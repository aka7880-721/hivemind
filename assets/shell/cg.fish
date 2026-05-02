# ComfyGit shell integration for fish — wrap `cg` so `cg cd <alias>` changes directory.
function cg --wraps ComfyGit --description 'ComfyGit launcher (supports cg cd <alias>)'
    if set -q argv[1]
        and test "$argv[1]" = cd
        if test (count $argv) -ne 2
            echo "usage: cg cd <alias>" >&2
            return 2
        end
        set -l target_dir (command ComfyGit pwd $argv[2])
        or return
        cd $target_dir
        or return
    else
        command ComfyGit $argv
    end
end
