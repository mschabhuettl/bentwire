# bentwire shell integration for bash and zsh.
#
# Source it from ~/.bashrc or ~/.zshrc:
#     . /path/to/bentwire/clients/shell/bentwire.sh
#
# The character reacts to
#   - a failed command: an attention animation and "It looks like your build failed."
#     (or a matching sentence for tests and other commands);
#   - a long-running command: after BENTWIRE_LONG_SECONDS it plays Processing, and when
#     the command ends it congratulates you (or tells you it failed).
#
# Only the `bentwire` CLI is used. Every call runs in the background with a short timeout,
# so the prompt never waits, and nothing is sent when bentwired is not running.
#
# Settings (environment variables, read at each prompt):
#   BENTWIRE_SHELL_OFF=1        disable everything
#   BENTWIRE_LONG_SECONDS=10    threshold for "long-running"
#   BENTWIRE_SHELL_IGNORE="..." commands that never count as long-running (interactive
#                               programs); default below
#   BENTWIRE_CLI=bentwire       the CLI to call

[ -n "${BASH_VERSION:-}${ZSH_VERSION:-}" ] || return 0
case $- in *i*) ;; *) return 0 ;; esac

_bw_default_ignore="exit logout vi vim nvim nano emacs less more most man info ssh mosh telnet top htop btop watch tmux screen fg bg jobs bentwire"

# Runs "$@" detached: no job notices, no waiting, no output.
_bw_bg() {
    ( "$@" </dev/null >/dev/null 2>&1 & )
}

_bw_cli() {
    if command -v timeout >/dev/null 2>&1; then
        timeout 3 "${BENTWIRE_CLI:-bentwire}" "$@"
    else
        "${BENTWIRE_CLI:-bentwire}" "$@"
    fi
}

# True when bentwired owns its bus name. Checked without D-Bus activation, so a stopped
# daemon is never started by the shell.
_bw_running() {
    if command -v busctl >/dev/null 2>&1; then
        busctl --user --no-pager status org.bentwire.Agent >/dev/null 2>&1
    elif command -v dbus-send >/dev/null 2>&1; then
        dbus-send --session --print-reply --dest=org.freedesktop.DBus /org/freedesktop/DBus \
            org.freedesktop.DBus.NameHasOwner string:org.bentwire.Agent 2>/dev/null | grep -q true
    else
        return 0
    fi
}

# Sends one or more CLI calls, separated by "--", in order: _bw_send play X -- say "Y"
_bw_send_now() {
    _bw_running || return 0
    local args=()
    while [ $# -gt 0 ]; do
        if [ "$1" = "--" ]; then
            [ ${#args[@]} -gt 0 ] && _bw_cli "${args[@]}"
            args=()
        else
            args+=("$1")
        fi
        shift
    done
    [ ${#args[@]} -gt 0 ] && _bw_cli "${args[@]}"
    return 0
}

# Runs "$@" holding this shell's send lock, so the daemon gets the calls in the order the
# shell decided them (the Processing timer and the end of a command race otherwise).
_bw_locked() {
    if command -v flock >/dev/null 2>&1; then
        (
            flock -w 15 9 || exit 0
            "$@"
        ) 9>"$(_bw_state_file).lock"
    else
        "$@"
    fi
}

_bw_send() {
    _bw_bg _bw_locked _bw_send_now "$@"
}

# First word of a command line, skipping assignments and wrappers like sudo or time.
_bw_command_word() {
    local w words
    if [ -n "${ZSH_VERSION:-}" ]; then
        # shellcheck disable=SC2206 # zsh splitting, no globbing
        words=(${=1})
    else
        read -r -a words <<<"$1"
    fi
    for w in "${words[@]}"; do
        case $w in
            *=*|sudo|doas|env|time|nice|nohup|command|exec|builtin) continue ;;
            *) printf '%s\n' "${w##*/}"; return ;;
        esac
    done
}

_bw_failure_text() { # <command line> <status>
    local first
    first=$(_bw_command_word "$1")
    case " $1 " in
        *" test "*|*" tests "*|*pytest*|*" check "*|*ctest*) echo "It looks like some tests failed." ;;
        *)
            case $first in
                make|cmake|ninja|meson|cargo|go|npm|pnpm|yarn|gcc|g++|cc|c++|clang|clang++|rustc|mvn|gradle|dotnet|tsc|javac)
                    echo "It looks like your build failed." ;;
                *) echo "It looks like that command failed (exit status $2)." ;;
            esac
            ;;
    esac
}

_bw_state_file() {
    printf '%s/bentwire-shell-%s\n' "${XDG_RUNTIME_DIR:-${TMPDIR:-/tmp}}" "$$"
}

# Called right before a command runs.
_bw_preexec() { # <command line>
    [ "${BENTWIRE_SHELL_OFF:-}" = 1 ] && return
    _bw_cmd=$1
    _bw_start=$SECONDS
    local first long=${BENTWIRE_LONG_SECONDS:-10} token
    first=$(_bw_command_word "$1")
    case " ${BENTWIRE_SHELL_IGNORE-$_bw_default_ignore} " in
        *" $first "*) _bw_watch=0; return ;;
    esac
    _bw_watch=1
    token="$$.$SECONDS.$RANDOM"
    _bw_token=$token
    printf '%s\n' "$token" >"$(_bw_state_file)" 2>/dev/null || return
    # The timer plays Processing only if this command is still running when it fires.
    _bw_bg _bw_timer "$token" "$long"
}

_bw_timer() { # <token> <seconds>
    local shell_pid=$$
    sleep "$2"
    kill -0 "$shell_pid" 2>/dev/null || return 0
    # "long" is marked under the lock: the end of the command can only see it, and send
    # its stop, once Processing has been sent
    _bw_locked _bw_timer_fire "$1"
}

_bw_timer_fire() { # <token>
    [ "$(cat "$(_bw_state_file)" 2>/dev/null)" = "$1" ] || return 0
    printf '%s\n' "$1" >"$(_bw_state_file).long"
    _bw_send_now play Processing
}

# Called before each prompt with the status of the command that just finished.
_bw_precmd() { # <status>
    local rc=$1
    [ -n "${_bw_cmd+x}" ] || return 0
    local cmd=$_bw_cmd elapsed=$((SECONDS - ${_bw_start:-$SECONDS})) token=
    unset _bw_cmd
    if [ "${_bw_watch:-0}" = 1 ]; then
        token=${_bw_token:-}
        : >"$(_bw_state_file)" 2>/dev/null # cancels a timer that has not fired yet
    fi
    [ "${BENTWIRE_SHELL_OFF:-}" = 1 ] && token=
    [ "${BENTWIRE_SHELL_OFF:-}" = 1 ] && [ -z "$token" ] && return 0
    local text=
    case $rc in 0|130|148) ;; *) text=$(_bw_failure_text "$cmd" "$rc") ;; esac
    [ -z "$token" ] && [ -z "$text" ] && return 0
    # Whether Processing was sent is known only under the send lock (the timer marks it
    # there), so the decision runs in the background too and the prompt never waits.
    _bw_bg _bw_locked _bw_finish "$token" "$rc" "$text" "$elapsed" "$(_bw_command_word "$cmd")"
    return 0
}

_bw_finish() { # <token or ""> <status> <failure text or ""> <seconds> <command word>
    local long=0 f
    f=$(_bw_state_file).long
    if [ -n "$1" ] && [ "$(cat "$f" 2>/dev/null)" = "$1" ]; then
        long=1
        rm -f "$f"
    fi
    if [ "${BENTWIRE_SHELL_OFF:-}" = 1 ]; then
        [ $long = 1 ] && _bw_send_now stop --all
    elif [ -n "$3" ] && [ $long = 1 ]; then
        _bw_send_now stop --all -- play GetAttention -- say "$3"
    elif [ -n "$3" ]; then
        _bw_send_now play GetAttention -- say "$3"
    elif [ $long = 1 ] && [ "$2" != 0 ]; then
        _bw_send_now stop --all # interrupted or suspended: end Processing, no success
    elif [ $long = 1 ]; then
        _bw_send_now stop --all -- play Congratulate -- say "Done after $4 s: $5"
    fi
}

# zsh also runs zshexit hooks when a subshell such as `(exit 1)` exits: clean up only
# in the interactive shell itself
_bw_exit() {
    [ "${ZSH_SUBSHELL:-0}" = 0 ] || return 0
    # only the timer token: detached sends may still need the .lock and .long files
    rm -f "$(_bw_state_file)" 2>/dev/null
}

if [ -n "${ZSH_VERSION:-}" ]; then
    _bw_zsh_preexec() { _bw_preexec "$1"; }
    _bw_zsh_precmd() { _bw_precmd $?; }
    autoload -Uz add-zsh-hook
    add-zsh-hook preexec _bw_zsh_preexec
    add-zsh-hook precmd _bw_zsh_precmd
    add-zsh-hook zshexit _bw_exit
else
    # PROMPT_COMMAND gets _bw_bash_prompt first (it needs $?) and _bw_bash_arm last, so
    # the DEBUG trap ignores the prompt commands and fires once for the next command line.
    # both keep $? for the user's own PROMPT_COMMAND entries
    _bw_bash_prompt() { _bw_precmd "$1"; return "$1"; }
    _bw_bash_arm() { local s=$?; _bw_armed=1; return $s; }
    _bw_bash_debug() {
        [ -n "${_bw_armed:-}" ] || return 0
        [ -n "${COMP_LINE:-}" ] && return 0
        unset _bw_armed
        _bw_preexec "$BASH_COMMAND"
    }
    if declare -p PROMPT_COMMAND 2>/dev/null | grep -q '^declare -a'; then
        PROMPT_COMMAND=('_bw_bash_prompt $?' "${PROMPT_COMMAND[@]}" _bw_bash_arm)
    else
        # shellcheck disable=SC2128,SC2178 # PROMPT_COMMAND is a string here
        PROMPT_COMMAND="_bw_bash_prompt \$?${PROMPT_COMMAND:+;$PROMPT_COMMAND};_bw_bash_arm"
    fi
    if [ -n "${bash_preexec_imported:-}${__bp_imported:-}" ]; then
        # bash-preexec owns the DEBUG trap and calls preexec_functions with the command line
        _bw_bp_preexec() { _bw_preexec "$1"; }
        preexec_functions+=(_bw_bp_preexec)
    elif [ -z "$(trap -p DEBUG)" ]; then
        trap '_bw_bash_debug' DEBUG
    fi
    # With a foreign DEBUG trap and no bash-preexec, nothing is reported.
fi
