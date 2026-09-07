#compdef zeroproxy

autoload -U is-at-least

_zeroproxy() {
    typeset -A opt_args
    typeset -a _arguments_options
    local ret=1

    if is-at-least 5.2; then
        _arguments_options=(-s -S -C)
    else
        _arguments_options=(-s -C)
    fi

    local context curcontext="$curcontext" state line
    _arguments "${_arguments_options[@]}" : \
'--host=[]:HOST:_default' \
'--port=[]:PORT:_default' \
'--log-filter=[]:LOG_FILTER:_default' \
'--data-dir=[]:DATA_DIR:_files' \
'-h[Print help]' \
'--help[Print help]' \
":: :_zeroproxy_commands" \
"*::: :->zeroproxy" \
&& ret=0
    case $state in
    (zeroproxy)
        words=($line[1] "${words[@]}")
        (( CURRENT += 1 ))
        curcontext="${curcontext%:*:*}:zeroproxy-command-$line[1]:"
        case $line[1] in
            (provider)
_arguments "${_arguments_options[@]}" : \
'-h[Print help]' \
'--help[Print help]' \
":: :_zeroproxy__subcmd__provider_commands" \
"*::: :->provider" \
&& ret=0

    case $state in
    (provider)
        words=($line[1] "${words[@]}")
        (( CURRENT += 1 ))
        curcontext="${curcontext%:*:*}:zeroproxy-provider-command-$line[1]:"
        case $line[1] in
            (list)
_arguments "${_arguments_options[@]}" : \
'--json[]' \
'-h[Print help]' \
'--help[Print help]' \
&& ret=0
;;
(add)
_arguments "${_arguments_options[@]}" : \
'--json[]' \
'-h[Print help]' \
'--help[Print help]' \
':name:_default' \
':config:_default' \
&& ret=0
;;
(help)
_arguments "${_arguments_options[@]}" : \
":: :_zeroproxy__subcmd__provider__subcmd__help_commands" \
"*::: :->help" \
&& ret=0

    case $state in
    (help)
        words=($line[1] "${words[@]}")
        (( CURRENT += 1 ))
        curcontext="${curcontext%:*:*}:zeroproxy-provider-help-command-$line[1]:"
        case $line[1] in
            (list)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(add)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(help)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
        esac
    ;;
esac
;;
        esac
    ;;
esac
;;
(key)
_arguments "${_arguments_options[@]}" : \
'-h[Print help]' \
'--help[Print help]' \
":: :_zeroproxy__subcmd__key_commands" \
"*::: :->key" \
&& ret=0

    case $state in
    (key)
        words=($line[1] "${words[@]}")
        (( CURRENT += 1 ))
        curcontext="${curcontext%:*:*}:zeroproxy-key-command-$line[1]:"
        case $line[1] in
            (list)
_arguments "${_arguments_options[@]}" : \
'--json[]' \
'-h[Print help]' \
'--help[Print help]' \
&& ret=0
;;
(add)
_arguments "${_arguments_options[@]}" : \
'--json[]' \
'-h[Print help]' \
'--help[Print help]' \
':name:_default' \
':key:_default' \
&& ret=0
;;
(help)
_arguments "${_arguments_options[@]}" : \
":: :_zeroproxy__subcmd__key__subcmd__help_commands" \
"*::: :->help" \
&& ret=0

    case $state in
    (help)
        words=($line[1] "${words[@]}")
        (( CURRENT += 1 ))
        curcontext="${curcontext%:*:*}:zeroproxy-key-help-command-$line[1]:"
        case $line[1] in
            (list)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(add)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(help)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
        esac
    ;;
esac
;;
        esac
    ;;
esac
;;
(pool)
_arguments "${_arguments_options[@]}" : \
'-h[Print help]' \
'--help[Print help]' \
":: :_zeroproxy__subcmd__pool_commands" \
"*::: :->pool" \
&& ret=0

    case $state in
    (pool)
        words=($line[1] "${words[@]}")
        (( CURRENT += 1 ))
        curcontext="${curcontext%:*:*}:zeroproxy-pool-command-$line[1]:"
        case $line[1] in
            (list)
_arguments "${_arguments_options[@]}" : \
'--json[]' \
'-h[Print help]' \
'--help[Print help]' \
&& ret=0
;;
(status)
_arguments "${_arguments_options[@]}" : \
'--json[]' \
'-h[Print help]' \
'--help[Print help]' \
':name:_default' \
&& ret=0
;;
(create)
_arguments "${_arguments_options[@]}" : \
'--json[]' \
'-h[Print help]' \
'--help[Print help]' \
':name:_default' \
':proxy_url:_default' \
&& ret=0
;;
(delete)
_arguments "${_arguments_options[@]}" : \
'--json[]' \
'-h[Print help]' \
'--help[Print help]' \
':name:_default' \
&& ret=0
;;
(help)
_arguments "${_arguments_options[@]}" : \
":: :_zeroproxy__subcmd__pool__subcmd__help_commands" \
"*::: :->help" \
&& ret=0

    case $state in
    (help)
        words=($line[1] "${words[@]}")
        (( CURRENT += 1 ))
        curcontext="${curcontext%:*:*}:zeroproxy-pool-help-command-$line[1]:"
        case $line[1] in
            (list)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(status)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(create)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(delete)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(help)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
        esac
    ;;
esac
;;
        esac
    ;;
esac
;;
(tunnel)
_arguments "${_arguments_options[@]}" : \
'-h[Print help]' \
'--help[Print help]' \
":: :_zeroproxy__subcmd__tunnel_commands" \
"*::: :->tunnel" \
&& ret=0

    case $state in
    (tunnel)
        words=($line[1] "${words[@]}")
        (( CURRENT += 1 ))
        curcontext="${curcontext%:*:*}:zeroproxy-tunnel-command-$line[1]:"
        case $line[1] in
            (start)
_arguments "${_arguments_options[@]}" : \
'--provider=[]:PROVIDER:_default' \
'--port=[]:PORT:_default' \
'-h[Print help]' \
'--help[Print help]' \
&& ret=0
;;
(stop)
_arguments "${_arguments_options[@]}" : \
'-h[Print help]' \
'--help[Print help]' \
&& ret=0
;;
(status)
_arguments "${_arguments_options[@]}" : \
'-h[Print help]' \
'--help[Print help]' \
&& ret=0
;;
(help)
_arguments "${_arguments_options[@]}" : \
":: :_zeroproxy__subcmd__tunnel__subcmd__help_commands" \
"*::: :->help" \
&& ret=0

    case $state in
    (help)
        words=($line[1] "${words[@]}")
        (( CURRENT += 1 ))
        curcontext="${curcontext%:*:*}:zeroproxy-tunnel-help-command-$line[1]:"
        case $line[1] in
            (start)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(stop)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(status)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(help)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
        esac
    ;;
esac
;;
        esac
    ;;
esac
;;
(route)
_arguments "${_arguments_options[@]}" : \
'--model=[Model ID (e.g. openai/gpt-4o-mini)]:MODEL:_default' \
'--combo=[Combo name]:COMBO:_default' \
'--prompt=[Prompt text]:PROMPT:_default' \
'--stream[Stream output]' \
'--json[JSON output]' \
'-h[Print help]' \
'--help[Print help]' \
&& ret=0
;;
(completion)
_arguments "${_arguments_options[@]}" : \
'-h[Print help]' \
'--help[Print help]' \
':shell:(bash elvish fish powershell zsh)' \
&& ret=0
;;
(help)
_arguments "${_arguments_options[@]}" : \
":: :_zeroproxy__subcmd__help_commands" \
"*::: :->help" \
&& ret=0

    case $state in
    (help)
        words=($line[1] "${words[@]}")
        (( CURRENT += 1 ))
        curcontext="${curcontext%:*:*}:zeroproxy-help-command-$line[1]:"
        case $line[1] in
            (provider)
_arguments "${_arguments_options[@]}" : \
":: :_zeroproxy__subcmd__help__subcmd__provider_commands" \
"*::: :->provider" \
&& ret=0

    case $state in
    (provider)
        words=($line[1] "${words[@]}")
        (( CURRENT += 1 ))
        curcontext="${curcontext%:*:*}:zeroproxy-help-provider-command-$line[1]:"
        case $line[1] in
            (list)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(add)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
        esac
    ;;
esac
;;
(key)
_arguments "${_arguments_options[@]}" : \
":: :_zeroproxy__subcmd__help__subcmd__key_commands" \
"*::: :->key" \
&& ret=0

    case $state in
    (key)
        words=($line[1] "${words[@]}")
        (( CURRENT += 1 ))
        curcontext="${curcontext%:*:*}:zeroproxy-help-key-command-$line[1]:"
        case $line[1] in
            (list)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(add)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
        esac
    ;;
esac
;;
(pool)
_arguments "${_arguments_options[@]}" : \
":: :_zeroproxy__subcmd__help__subcmd__pool_commands" \
"*::: :->pool" \
&& ret=0

    case $state in
    (pool)
        words=($line[1] "${words[@]}")
        (( CURRENT += 1 ))
        curcontext="${curcontext%:*:*}:zeroproxy-help-pool-command-$line[1]:"
        case $line[1] in
            (list)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(status)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(create)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(delete)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
        esac
    ;;
esac
;;
(tunnel)
_arguments "${_arguments_options[@]}" : \
":: :_zeroproxy__subcmd__help__subcmd__tunnel_commands" \
"*::: :->tunnel" \
&& ret=0

    case $state in
    (tunnel)
        words=($line[1] "${words[@]}")
        (( CURRENT += 1 ))
        curcontext="${curcontext%:*:*}:zeroproxy-help-tunnel-command-$line[1]:"
        case $line[1] in
            (start)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(stop)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(status)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
        esac
    ;;
esac
;;
(route)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(completion)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(help)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
        esac
    ;;
esac
;;
        esac
    ;;
esac
}

(( $+functions[_zeroproxy_commands] )) ||
_zeroproxy_commands() {
    local commands; commands=(
'provider:' \
'key:' \
'pool:' \
'tunnel:' \
'route:' \
'completion:' \
'help:Print this message or the help of the given subcommand(s)' \
    )
    _describe -t commands 'zeroproxy commands' commands "$@"
}
(( $+functions[_zeroproxy__subcmd__completion_commands] )) ||
_zeroproxy__subcmd__completion_commands() {
    local commands; commands=()
    _describe -t commands 'zeroproxy completion commands' commands "$@"
}
(( $+functions[_zeroproxy__subcmd__help_commands] )) ||
_zeroproxy__subcmd__help_commands() {
    local commands; commands=(
'provider:' \
'key:' \
'pool:' \
'tunnel:' \
'route:' \
'completion:' \
'help:Print this message or the help of the given subcommand(s)' \
    )
    _describe -t commands 'zeroproxy help commands' commands "$@"
}
(( $+functions[_zeroproxy__subcmd__help__subcmd__completion_commands] )) ||
_zeroproxy__subcmd__help__subcmd__completion_commands() {
    local commands; commands=()
    _describe -t commands 'zeroproxy help completion commands' commands "$@"
}
(( $+functions[_zeroproxy__subcmd__help__subcmd__help_commands] )) ||
_zeroproxy__subcmd__help__subcmd__help_commands() {
    local commands; commands=()
    _describe -t commands 'zeroproxy help help commands' commands "$@"
}
(( $+functions[_zeroproxy__subcmd__help__subcmd__key_commands] )) ||
_zeroproxy__subcmd__help__subcmd__key_commands() {
    local commands; commands=(
'list:' \
'add:' \
    )
    _describe -t commands 'zeroproxy help key commands' commands "$@"
}
(( $+functions[_zeroproxy__subcmd__help__subcmd__key__subcmd__add_commands] )) ||
_zeroproxy__subcmd__help__subcmd__key__subcmd__add_commands() {
    local commands; commands=()
    _describe -t commands 'zeroproxy help key add commands' commands "$@"
}
(( $+functions[_zeroproxy__subcmd__help__subcmd__key__subcmd__list_commands] )) ||
_zeroproxy__subcmd__help__subcmd__key__subcmd__list_commands() {
    local commands; commands=()
    _describe -t commands 'zeroproxy help key list commands' commands "$@"
}
(( $+functions[_zeroproxy__subcmd__help__subcmd__pool_commands] )) ||
_zeroproxy__subcmd__help__subcmd__pool_commands() {
    local commands; commands=(
'list:' \
'status:' \
'create:' \
'delete:' \
    )
    _describe -t commands 'zeroproxy help pool commands' commands "$@"
}
(( $+functions[_zeroproxy__subcmd__help__subcmd__pool__subcmd__create_commands] )) ||
_zeroproxy__subcmd__help__subcmd__pool__subcmd__create_commands() {
    local commands; commands=()
    _describe -t commands 'zeroproxy help pool create commands' commands "$@"
}
(( $+functions[_zeroproxy__subcmd__help__subcmd__pool__subcmd__delete_commands] )) ||
_zeroproxy__subcmd__help__subcmd__pool__subcmd__delete_commands() {
    local commands; commands=()
    _describe -t commands 'zeroproxy help pool delete commands' commands "$@"
}
(( $+functions[_zeroproxy__subcmd__help__subcmd__pool__subcmd__list_commands] )) ||
_zeroproxy__subcmd__help__subcmd__pool__subcmd__list_commands() {
    local commands; commands=()
    _describe -t commands 'zeroproxy help pool list commands' commands "$@"
}
(( $+functions[_zeroproxy__subcmd__help__subcmd__pool__subcmd__status_commands] )) ||
_zeroproxy__subcmd__help__subcmd__pool__subcmd__status_commands() {
    local commands; commands=()
    _describe -t commands 'zeroproxy help pool status commands' commands "$@"
}
(( $+functions[_zeroproxy__subcmd__help__subcmd__provider_commands] )) ||
_zeroproxy__subcmd__help__subcmd__provider_commands() {
    local commands; commands=(
'list:' \
'add:' \
    )
    _describe -t commands 'zeroproxy help provider commands' commands "$@"
}
(( $+functions[_zeroproxy__subcmd__help__subcmd__provider__subcmd__add_commands] )) ||
_zeroproxy__subcmd__help__subcmd__provider__subcmd__add_commands() {
    local commands; commands=()
    _describe -t commands 'zeroproxy help provider add commands' commands "$@"
}
(( $+functions[_zeroproxy__subcmd__help__subcmd__provider__subcmd__list_commands] )) ||
_zeroproxy__subcmd__help__subcmd__provider__subcmd__list_commands() {
    local commands; commands=()
    _describe -t commands 'zeroproxy help provider list commands' commands "$@"
}
(( $+functions[_zeroproxy__subcmd__help__subcmd__route_commands] )) ||
_zeroproxy__subcmd__help__subcmd__route_commands() {
    local commands; commands=()
    _describe -t commands 'zeroproxy help route commands' commands "$@"
}
(( $+functions[_zeroproxy__subcmd__help__subcmd__tunnel_commands] )) ||
_zeroproxy__subcmd__help__subcmd__tunnel_commands() {
    local commands; commands=(
'start:' \
'stop:' \
'status:' \
    )
    _describe -t commands 'zeroproxy help tunnel commands' commands "$@"
}
(( $+functions[_zeroproxy__subcmd__help__subcmd__tunnel__subcmd__start_commands] )) ||
_zeroproxy__subcmd__help__subcmd__tunnel__subcmd__start_commands() {
    local commands; commands=()
    _describe -t commands 'zeroproxy help tunnel start commands' commands "$@"
}
(( $+functions[_zeroproxy__subcmd__help__subcmd__tunnel__subcmd__status_commands] )) ||
_zeroproxy__subcmd__help__subcmd__tunnel__subcmd__status_commands() {
    local commands; commands=()
    _describe -t commands 'zeroproxy help tunnel status commands' commands "$@"
}
(( $+functions[_zeroproxy__subcmd__help__subcmd__tunnel__subcmd__stop_commands] )) ||
_zeroproxy__subcmd__help__subcmd__tunnel__subcmd__stop_commands() {
    local commands; commands=()
    _describe -t commands 'zeroproxy help tunnel stop commands' commands "$@"
}
(( $+functions[_zeroproxy__subcmd__key_commands] )) ||
_zeroproxy__subcmd__key_commands() {
    local commands; commands=(
'list:' \
'add:' \
'help:Print this message or the help of the given subcommand(s)' \
    )
    _describe -t commands 'zeroproxy key commands' commands "$@"
}
(( $+functions[_zeroproxy__subcmd__key__subcmd__add_commands] )) ||
_zeroproxy__subcmd__key__subcmd__add_commands() {
    local commands; commands=()
    _describe -t commands 'zeroproxy key add commands' commands "$@"
}
(( $+functions[_zeroproxy__subcmd__key__subcmd__help_commands] )) ||
_zeroproxy__subcmd__key__subcmd__help_commands() {
    local commands; commands=(
'list:' \
'add:' \
'help:Print this message or the help of the given subcommand(s)' \
    )
    _describe -t commands 'zeroproxy key help commands' commands "$@"
}
(( $+functions[_zeroproxy__subcmd__key__subcmd__help__subcmd__add_commands] )) ||
_zeroproxy__subcmd__key__subcmd__help__subcmd__add_commands() {
    local commands; commands=()
    _describe -t commands 'zeroproxy key help add commands' commands "$@"
}
(( $+functions[_zeroproxy__subcmd__key__subcmd__help__subcmd__help_commands] )) ||
_zeroproxy__subcmd__key__subcmd__help__subcmd__help_commands() {
    local commands; commands=()
    _describe -t commands 'zeroproxy key help help commands' commands "$@"
}
(( $+functions[_zeroproxy__subcmd__key__subcmd__help__subcmd__list_commands] )) ||
_zeroproxy__subcmd__key__subcmd__help__subcmd__list_commands() {
    local commands; commands=()
    _describe -t commands 'zeroproxy key help list commands' commands "$@"
}
(( $+functions[_zeroproxy__subcmd__key__subcmd__list_commands] )) ||
_zeroproxy__subcmd__key__subcmd__list_commands() {
    local commands; commands=()
    _describe -t commands 'zeroproxy key list commands' commands "$@"
}
(( $+functions[_zeroproxy__subcmd__pool_commands] )) ||
_zeroproxy__subcmd__pool_commands() {
    local commands; commands=(
'list:' \
'status:' \
'create:' \
'delete:' \
'help:Print this message or the help of the given subcommand(s)' \
    )
    _describe -t commands 'zeroproxy pool commands' commands "$@"
}
(( $+functions[_zeroproxy__subcmd__pool__subcmd__create_commands] )) ||
_zeroproxy__subcmd__pool__subcmd__create_commands() {
    local commands; commands=()
    _describe -t commands 'zeroproxy pool create commands' commands "$@"
}
(( $+functions[_zeroproxy__subcmd__pool__subcmd__delete_commands] )) ||
_zeroproxy__subcmd__pool__subcmd__delete_commands() {
    local commands; commands=()
    _describe -t commands 'zeroproxy pool delete commands' commands "$@"
}
(( $+functions[_zeroproxy__subcmd__pool__subcmd__help_commands] )) ||
_zeroproxy__subcmd__pool__subcmd__help_commands() {
    local commands; commands=(
'list:' \
'status:' \
'create:' \
'delete:' \
'help:Print this message or the help of the given subcommand(s)' \
    )
    _describe -t commands 'zeroproxy pool help commands' commands "$@"
}
(( $+functions[_zeroproxy__subcmd__pool__subcmd__help__subcmd__create_commands] )) ||
_zeroproxy__subcmd__pool__subcmd__help__subcmd__create_commands() {
    local commands; commands=()
    _describe -t commands 'zeroproxy pool help create commands' commands "$@"
}
(( $+functions[_zeroproxy__subcmd__pool__subcmd__help__subcmd__delete_commands] )) ||
_zeroproxy__subcmd__pool__subcmd__help__subcmd__delete_commands() {
    local commands; commands=()
    _describe -t commands 'zeroproxy pool help delete commands' commands "$@"
}
(( $+functions[_zeroproxy__subcmd__pool__subcmd__help__subcmd__help_commands] )) ||
_zeroproxy__subcmd__pool__subcmd__help__subcmd__help_commands() {
    local commands; commands=()
    _describe -t commands 'zeroproxy pool help help commands' commands "$@"
}
(( $+functions[_zeroproxy__subcmd__pool__subcmd__help__subcmd__list_commands] )) ||
_zeroproxy__subcmd__pool__subcmd__help__subcmd__list_commands() {
    local commands; commands=()
    _describe -t commands 'zeroproxy pool help list commands' commands "$@"
}
(( $+functions[_zeroproxy__subcmd__pool__subcmd__help__subcmd__status_commands] )) ||
_zeroproxy__subcmd__pool__subcmd__help__subcmd__status_commands() {
    local commands; commands=()
    _describe -t commands 'zeroproxy pool help status commands' commands "$@"
}
(( $+functions[_zeroproxy__subcmd__pool__subcmd__list_commands] )) ||
_zeroproxy__subcmd__pool__subcmd__list_commands() {
    local commands; commands=()
    _describe -t commands 'zeroproxy pool list commands' commands "$@"
}
(( $+functions[_zeroproxy__subcmd__pool__subcmd__status_commands] )) ||
_zeroproxy__subcmd__pool__subcmd__status_commands() {
    local commands; commands=()
    _describe -t commands 'zeroproxy pool status commands' commands "$@"
}
(( $+functions[_zeroproxy__subcmd__provider_commands] )) ||
_zeroproxy__subcmd__provider_commands() {
    local commands; commands=(
'list:' \
'add:' \
'help:Print this message or the help of the given subcommand(s)' \
    )
    _describe -t commands 'zeroproxy provider commands' commands "$@"
}
(( $+functions[_zeroproxy__subcmd__provider__subcmd__add_commands] )) ||
_zeroproxy__subcmd__provider__subcmd__add_commands() {
    local commands; commands=()
    _describe -t commands 'zeroproxy provider add commands' commands "$@"
}
(( $+functions[_zeroproxy__subcmd__provider__subcmd__help_commands] )) ||
_zeroproxy__subcmd__provider__subcmd__help_commands() {
    local commands; commands=(
'list:' \
'add:' \
'help:Print this message or the help of the given subcommand(s)' \
    )
    _describe -t commands 'zeroproxy provider help commands' commands "$@"
}
(( $+functions[_zeroproxy__subcmd__provider__subcmd__help__subcmd__add_commands] )) ||
_zeroproxy__subcmd__provider__subcmd__help__subcmd__add_commands() {
    local commands; commands=()
    _describe -t commands 'zeroproxy provider help add commands' commands "$@"
}
(( $+functions[_zeroproxy__subcmd__provider__subcmd__help__subcmd__help_commands] )) ||
_zeroproxy__subcmd__provider__subcmd__help__subcmd__help_commands() {
    local commands; commands=()
    _describe -t commands 'zeroproxy provider help help commands' commands "$@"
}
(( $+functions[_zeroproxy__subcmd__provider__subcmd__help__subcmd__list_commands] )) ||
_zeroproxy__subcmd__provider__subcmd__help__subcmd__list_commands() {
    local commands; commands=()
    _describe -t commands 'zeroproxy provider help list commands' commands "$@"
}
(( $+functions[_zeroproxy__subcmd__provider__subcmd__list_commands] )) ||
_zeroproxy__subcmd__provider__subcmd__list_commands() {
    local commands; commands=()
    _describe -t commands 'zeroproxy provider list commands' commands "$@"
}
(( $+functions[_zeroproxy__subcmd__route_commands] )) ||
_zeroproxy__subcmd__route_commands() {
    local commands; commands=()
    _describe -t commands 'zeroproxy route commands' commands "$@"
}
(( $+functions[_zeroproxy__subcmd__tunnel_commands] )) ||
_zeroproxy__subcmd__tunnel_commands() {
    local commands; commands=(
'start:' \
'stop:' \
'status:' \
'help:Print this message or the help of the given subcommand(s)' \
    )
    _describe -t commands 'zeroproxy tunnel commands' commands "$@"
}
(( $+functions[_zeroproxy__subcmd__tunnel__subcmd__help_commands] )) ||
_zeroproxy__subcmd__tunnel__subcmd__help_commands() {
    local commands; commands=(
'start:' \
'stop:' \
'status:' \
'help:Print this message or the help of the given subcommand(s)' \
    )
    _describe -t commands 'zeroproxy tunnel help commands' commands "$@"
}
(( $+functions[_zeroproxy__subcmd__tunnel__subcmd__help__subcmd__help_commands] )) ||
_zeroproxy__subcmd__tunnel__subcmd__help__subcmd__help_commands() {
    local commands; commands=()
    _describe -t commands 'zeroproxy tunnel help help commands' commands "$@"
}
(( $+functions[_zeroproxy__subcmd__tunnel__subcmd__help__subcmd__start_commands] )) ||
_zeroproxy__subcmd__tunnel__subcmd__help__subcmd__start_commands() {
    local commands; commands=()
    _describe -t commands 'zeroproxy tunnel help start commands' commands "$@"
}
(( $+functions[_zeroproxy__subcmd__tunnel__subcmd__help__subcmd__status_commands] )) ||
_zeroproxy__subcmd__tunnel__subcmd__help__subcmd__status_commands() {
    local commands; commands=()
    _describe -t commands 'zeroproxy tunnel help status commands' commands "$@"
}
(( $+functions[_zeroproxy__subcmd__tunnel__subcmd__help__subcmd__stop_commands] )) ||
_zeroproxy__subcmd__tunnel__subcmd__help__subcmd__stop_commands() {
    local commands; commands=()
    _describe -t commands 'zeroproxy tunnel help stop commands' commands "$@"
}
(( $+functions[_zeroproxy__subcmd__tunnel__subcmd__start_commands] )) ||
_zeroproxy__subcmd__tunnel__subcmd__start_commands() {
    local commands; commands=()
    _describe -t commands 'zeroproxy tunnel start commands' commands "$@"
}
(( $+functions[_zeroproxy__subcmd__tunnel__subcmd__status_commands] )) ||
_zeroproxy__subcmd__tunnel__subcmd__status_commands() {
    local commands; commands=()
    _describe -t commands 'zeroproxy tunnel status commands' commands "$@"
}
(( $+functions[_zeroproxy__subcmd__tunnel__subcmd__stop_commands] )) ||
_zeroproxy__subcmd__tunnel__subcmd__stop_commands() {
    local commands; commands=()
    _describe -t commands 'zeroproxy tunnel stop commands' commands "$@"
}

if [ "$funcstack[1]" = "_zeroproxy" ]; then
    _zeroproxy "$@"
else
    compdef _zeroproxy zeroproxy
fi
