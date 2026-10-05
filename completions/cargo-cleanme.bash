_cargo() {
    local i cur prev opts cmd
    COMPREPLY=()
    if [[ "${BASH_VERSINFO[0]}" -ge 4 ]]; then
        cur="$2"
    else
        cur="${COMP_WORDS[COMP_CWORD]}"
    fi
    prev="$3"
    cmd=""
    opts=""

    for i in "${COMP_WORDS[@]:0:COMP_CWORD}"
    do
        case "${cmd},${i}" in
            ",$1")
                cmd="cargo"
                ;;
            cargo,cargo-cleanme)
                cmd="cargo__subcmd__cargo__subcmd__cleanme"
                ;;
            cargo,help)
                cmd="cargo__subcmd__help"
                ;;
            cargo__subcmd__cargo__subcmd__cleanme,clean)
                cmd="cargo__subcmd__cargo__subcmd__cleanme__subcmd__clean"
                ;;
            cargo__subcmd__cargo__subcmd__cleanme,config)
                cmd="cargo__subcmd__cargo__subcmd__cleanme__subcmd__config"
                ;;
            cargo__subcmd__cargo__subcmd__cleanme,scan)
                cmd="cargo__subcmd__cargo__subcmd__cleanme__subcmd__scan"
                ;;
            cargo__subcmd__cargo__subcmd__cleanme,update)
                cmd="cargo__subcmd__cargo__subcmd__cleanme__subcmd__update"
                ;;
            cargo__subcmd__cargo__subcmd__cleanme__subcmd__config,edit)
                cmd="cargo__subcmd__cargo__subcmd__cleanme__subcmd__config__subcmd__edit"
                ;;
            cargo__subcmd__cargo__subcmd__cleanme__subcmd__config,path)
                cmd="cargo__subcmd__cargo__subcmd__cleanme__subcmd__config__subcmd__path"
                ;;
            cargo__subcmd__cargo__subcmd__cleanme__subcmd__config,show)
                cmd="cargo__subcmd__cargo__subcmd__cleanme__subcmd__config__subcmd__show"
                ;;
            cargo__subcmd__help,cargo-cleanme)
                cmd="cargo__subcmd__help__subcmd__cargo__subcmd__cleanme"
                ;;
            cargo__subcmd__help,help)
                cmd="cargo__subcmd__help__subcmd__help"
                ;;
            cargo__subcmd__help__subcmd__cargo__subcmd__cleanme,clean)
                cmd="cargo__subcmd__help__subcmd__cargo__subcmd__cleanme__subcmd__clean"
                ;;
            cargo__subcmd__help__subcmd__cargo__subcmd__cleanme,config)
                cmd="cargo__subcmd__help__subcmd__cargo__subcmd__cleanme__subcmd__config"
                ;;
            cargo__subcmd__help__subcmd__cargo__subcmd__cleanme,scan)
                cmd="cargo__subcmd__help__subcmd__cargo__subcmd__cleanme__subcmd__scan"
                ;;
            cargo__subcmd__help__subcmd__cargo__subcmd__cleanme,update)
                cmd="cargo__subcmd__help__subcmd__cargo__subcmd__cleanme__subcmd__update"
                ;;
            cargo__subcmd__help__subcmd__cargo__subcmd__cleanme__subcmd__config,edit)
                cmd="cargo__subcmd__help__subcmd__cargo__subcmd__cleanme__subcmd__config__subcmd__edit"
                ;;
            cargo__subcmd__help__subcmd__cargo__subcmd__cleanme__subcmd__config,path)
                cmd="cargo__subcmd__help__subcmd__cargo__subcmd__cleanme__subcmd__config__subcmd__path"
                ;;
            cargo__subcmd__help__subcmd__cargo__subcmd__cleanme__subcmd__config,show)
                cmd="cargo__subcmd__help__subcmd__cargo__subcmd__cleanme__subcmd__config__subcmd__show"
                ;;
            *)
                ;;
        esac
    done

    case "${cmd}" in
        cargo)
            opts="-h --help cargo-cleanme help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 1 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        cargo__subcmd__cargo__subcmd__cleanme)
            opts="-h -V --config --no-progress --stats --format --dry-run --dryrun --help --version scan config clean update"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 2 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --config)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --format)
                    COMPREPLY=($(compgen -W "human json log" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        cargo__subcmd__cargo__subcmd__cleanme__subcmd__clean)
            opts="-h --known --full --min-reclaimable-bytes --older-than --include --exclude --profile --package --dry-run --cargo-preview --yes --dryrun --config --no-progress --stats --format --help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --min-reclaimable-bytes)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --older-than)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --include)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --exclude)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --profile)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --package)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --config)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --format)
                    COMPREPLY=($(compgen -W "human json log" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        cargo__subcmd__cargo__subcmd__cleanme__subcmd__config)
            opts="-h --config --no-progress --stats --format --help path show edit"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --config)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --format)
                    COMPREPLY=($(compgen -W "human json log" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        cargo__subcmd__cargo__subcmd__cleanme__subcmd__config__subcmd__edit)
            opts="-h --config --no-progress --stats --format --help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --config)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --format)
                    COMPREPLY=($(compgen -W "human json log" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        cargo__subcmd__cargo__subcmd__cleanme__subcmd__config__subcmd__path)
            opts="-h --config --no-progress --stats --format --help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --config)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --format)
                    COMPREPLY=($(compgen -W "human json log" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        cargo__subcmd__cargo__subcmd__cleanme__subcmd__config__subcmd__show)
            opts="-h --config --no-progress --stats --format --help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --config)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --format)
                    COMPREPLY=($(compgen -W "human json log" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        cargo__subcmd__cargo__subcmd__cleanme__subcmd__scan)
            opts="-h --known --full --config --no-progress --stats --format --help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --config)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --format)
                    COMPREPLY=($(compgen -W "human json log" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        cargo__subcmd__cargo__subcmd__cleanme__subcmd__update)
            opts="-h --dry-run --config --no-progress --stats --format --help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --config)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --format)
                    COMPREPLY=($(compgen -W "human json log" -- "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        cargo__subcmd__help)
            opts="cargo-cleanme help"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 2 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        cargo__subcmd__help__subcmd__cargo__subcmd__cleanme)
            opts="scan config clean update"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        cargo__subcmd__help__subcmd__cargo__subcmd__cleanme__subcmd__clean)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        cargo__subcmd__help__subcmd__cargo__subcmd__cleanme__subcmd__config)
            opts="path show edit"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        cargo__subcmd__help__subcmd__cargo__subcmd__cleanme__subcmd__config__subcmd__edit)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        cargo__subcmd__help__subcmd__cargo__subcmd__cleanme__subcmd__config__subcmd__path)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        cargo__subcmd__help__subcmd__cargo__subcmd__cleanme__subcmd__config__subcmd__show)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 5 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        cargo__subcmd__help__subcmd__cargo__subcmd__cleanme__subcmd__scan)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        cargo__subcmd__help__subcmd__cargo__subcmd__cleanme__subcmd__update)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 4 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        cargo__subcmd__help__subcmd__help)
            opts=""
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 3 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
    esac
}

if [[ "${BASH_VERSINFO[0]}" -eq 4 && "${BASH_VERSINFO[1]}" -ge 4 || "${BASH_VERSINFO[0]}" -gt 4 ]]; then
    complete -F _cargo -o nosort -o bashdefault -o default cargo
else
    complete -F _cargo -o bashdefault -o default cargo
fi
