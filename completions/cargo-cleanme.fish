# Print an optspec for argparse to handle cmd's options that are independent of any subcommand.
function __fish_cargo_global_optspecs
    string join \n h/help
end

function __fish_cargo_needs_command
    # Figure out if the current invocation already has a command.
    set -l cmd (commandline -opc)
    set -e cmd[1]
    argparse -s (__fish_cargo_global_optspecs) -- $cmd 2>/dev/null
    or return
    if set -q argv[1]
        # Also print the command, so this can be used to figure out what it is.
        echo $argv[1]
        return 1
    end
    return 0
end

function __fish_cargo_using_subcommand
    set -l cmd (__fish_cargo_needs_command)
    test -z "$cmd"
    and return 1
    contains -- $cmd[1] $argv
end

complete -c cargo -n "__fish_cargo_needs_command" -s h -l help -d 'Print help'
complete -c cargo -n "__fish_cargo_needs_command" -f -a "cargo-cleanme" -d 'Find inactive Cargo artifacts and safely preview their cleanup'
complete -c cargo -n "__fish_cargo_needs_command" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c cargo -n "__fish_cargo_using_subcommand cargo-cleanme; and not __fish_seen_subcommand_from scan config clean update" -l config -r -F
complete -c cargo -n "__fish_cargo_using_subcommand cargo-cleanme; and not __fish_seen_subcommand_from scan config clean update" -l format -d 'Select human-readable output or the versioned JSON automation contract' -r -f -a "human\t''
json\t''"
complete -c cargo -n "__fish_cargo_using_subcommand cargo-cleanme; and not __fish_seen_subcommand_from scan config clean update" -l no-progress -d 'Disable transient progress UI (useful for benchmarks and debugging)'
complete -c cargo -n "__fish_cargo_using_subcommand cargo-cleanme; and not __fish_seen_subcommand_from scan config clean update" -l stats -d 'Print detailed scan/cleanup counters and phase timings to stderr. Never changes deterministic stdout. `--no-progress --stats` is the canonical benchmark/debug combination'
complete -c cargo -n "__fish_cargo_using_subcommand cargo-cleanme; and not __fish_seen_subcommand_from scan config clean update" -s h -l help -d 'Print help'
complete -c cargo -n "__fish_cargo_using_subcommand cargo-cleanme; and not __fish_seen_subcommand_from scan config clean update" -s V -l version -d 'Print version'
complete -c cargo -n "__fish_cargo_using_subcommand cargo-cleanme; and not __fish_seen_subcommand_from scan config clean update" -f -a "scan"
complete -c cargo -n "__fish_cargo_using_subcommand cargo-cleanme; and not __fish_seen_subcommand_from scan config clean update" -f -a "config"
complete -c cargo -n "__fish_cargo_using_subcommand cargo-cleanme; and not __fish_seen_subcommand_from scan config clean update" -f -a "clean" -d 'Preview or execute cleanup of revalidated Cargo build artifacts'
complete -c cargo -n "__fish_cargo_using_subcommand cargo-cleanme; and not __fish_seen_subcommand_from scan config clean update" -f -a "update" -d 'Update this cargo-cleanme to the latest stable published release'
complete -c cargo -n "__fish_cargo_using_subcommand cargo-cleanme; and __fish_seen_subcommand_from scan" -l config -r -F
complete -c cargo -n "__fish_cargo_using_subcommand cargo-cleanme; and __fish_seen_subcommand_from scan" -l format -d 'Select human-readable output or the versioned JSON automation contract' -r -f -a "human\t''
json\t''"
complete -c cargo -n "__fish_cargo_using_subcommand cargo-cleanme; and __fish_seen_subcommand_from scan" -l full
complete -c cargo -n "__fish_cargo_using_subcommand cargo-cleanme; and __fish_seen_subcommand_from scan" -l no-progress -d 'Disable transient progress UI (useful for benchmarks and debugging)'
complete -c cargo -n "__fish_cargo_using_subcommand cargo-cleanme; and __fish_seen_subcommand_from scan" -l stats -d 'Print detailed scan/cleanup counters and phase timings to stderr. Never changes deterministic stdout. `--no-progress --stats` is the canonical benchmark/debug combination'
complete -c cargo -n "__fish_cargo_using_subcommand cargo-cleanme; and __fish_seen_subcommand_from scan" -s h -l help -d 'Print help'
complete -c cargo -n "__fish_cargo_using_subcommand cargo-cleanme; and __fish_seen_subcommand_from config" -l config -r -F
complete -c cargo -n "__fish_cargo_using_subcommand cargo-cleanme; and __fish_seen_subcommand_from config" -l format -d 'Select human-readable output or the versioned JSON automation contract' -r -f -a "human\t''
json\t''"
complete -c cargo -n "__fish_cargo_using_subcommand cargo-cleanme; and __fish_seen_subcommand_from config" -l no-progress -d 'Disable transient progress UI (useful for benchmarks and debugging)'
complete -c cargo -n "__fish_cargo_using_subcommand cargo-cleanme; and __fish_seen_subcommand_from config" -l stats -d 'Print detailed scan/cleanup counters and phase timings to stderr. Never changes deterministic stdout. `--no-progress --stats` is the canonical benchmark/debug combination'
complete -c cargo -n "__fish_cargo_using_subcommand cargo-cleanme; and __fish_seen_subcommand_from config" -s h -l help -d 'Print help'
complete -c cargo -n "__fish_cargo_using_subcommand cargo-cleanme; and __fish_seen_subcommand_from config" -f -a "path"
complete -c cargo -n "__fish_cargo_using_subcommand cargo-cleanme; and __fish_seen_subcommand_from config" -f -a "show"
complete -c cargo -n "__fish_cargo_using_subcommand cargo-cleanme; and __fish_seen_subcommand_from config" -f -a "edit"
complete -c cargo -n "__fish_cargo_using_subcommand cargo-cleanme; and __fish_seen_subcommand_from clean" -l min-reclaimable-bytes -d 'Require at least this many reclaimable bytes per workspace' -r
complete -c cargo -n "__fish_cargo_using_subcommand cargo-cleanme; and __fish_seen_subcommand_from clean" -l older-than -d 'Require this many seconds of inactivity' -r
complete -c cargo -n "__fish_cargo_using_subcommand cargo-cleanme; and __fish_seen_subcommand_from clean" -l include -d 'Include canonical workspace roots matching this glob (repeatable)' -r
complete -c cargo -n "__fish_cargo_using_subcommand cargo-cleanme; and __fish_seen_subcommand_from clean" -l exclude -d 'Exclude canonical workspace roots matching this glob (repeatable)' -r
complete -c cargo -n "__fish_cargo_using_subcommand cargo-cleanme; and __fish_seen_subcommand_from clean" -l profile -d 'Ask Cargo to clean one named profile. Selector byte estimates are unknown' -r
complete -c cargo -n "__fish_cargo_using_subcommand cargo-cleanme; and __fish_seen_subcommand_from clean" -l package -d 'Clean one validated workspace package through qualified Cargo versions' -r
complete -c cargo -n "__fish_cargo_using_subcommand cargo-cleanme; and __fish_seen_subcommand_from clean" -l config -r -F
complete -c cargo -n "__fish_cargo_using_subcommand cargo-cleanme; and __fish_seen_subcommand_from clean" -l format -d 'Select human-readable output or the versioned JSON automation contract' -r -f -a "human\t''
json\t''"
complete -c cargo -n "__fish_cargo_using_subcommand cargo-cleanme; and __fish_seen_subcommand_from clean" -l known -d 'Clean bounded roots selected by the current Routine policy'
complete -c cargo -n "__fish_cargo_using_subcommand cargo-cleanme; and __fish_seen_subcommand_from clean" -l full -d 'Complete Full reconciliation before cleaning its learned roots'
complete -c cargo -n "__fish_cargo_using_subcommand cargo-cleanme; and __fish_seen_subcommand_from clean" -l dry-run -d 'Cargo preview: invoke Cargo\'s own dry-run (the default mode). Distinct from `--dryrun` (cargo-cleanme simulation)'
complete -c cargo -n "__fish_cargo_using_subcommand cargo-cleanme; and __fish_seen_subcommand_from clean" -l dryrun -d 'Application simulation: run the full decision/progress/report path but invoke no Cargo clean command. Distinct from `--dry-run`'
complete -c cargo -n "__fish_cargo_using_subcommand cargo-cleanme; and __fish_seen_subcommand_from clean" -l yes -d 'Execute Cargo clean after per-workspace revalidation'
complete -c cargo -n "__fish_cargo_using_subcommand cargo-cleanme; and __fish_seen_subcommand_from clean" -l no-progress -d 'Disable transient progress UI (useful for benchmarks and debugging)'
complete -c cargo -n "__fish_cargo_using_subcommand cargo-cleanme; and __fish_seen_subcommand_from clean" -l stats -d 'Print detailed scan/cleanup counters and phase timings to stderr. Never changes deterministic stdout. `--no-progress --stats` is the canonical benchmark/debug combination'
complete -c cargo -n "__fish_cargo_using_subcommand cargo-cleanme; and __fish_seen_subcommand_from clean" -s h -l help -d 'Print help'
complete -c cargo -n "__fish_cargo_using_subcommand cargo-cleanme; and __fish_seen_subcommand_from update" -l config -r -F
complete -c cargo -n "__fish_cargo_using_subcommand cargo-cleanme; and __fish_seen_subcommand_from update" -l format -d 'Select human-readable output or the versioned JSON automation contract' -r -f -a "human\t''
json\t''"
complete -c cargo -n "__fish_cargo_using_subcommand cargo-cleanme; and __fish_seen_subcommand_from update" -l dry-run -d 'Resolve the plan and report it without acquiring or replacing bytes'
complete -c cargo -n "__fish_cargo_using_subcommand cargo-cleanme; and __fish_seen_subcommand_from update" -l no-progress -d 'Disable transient progress UI (useful for benchmarks and debugging)'
complete -c cargo -n "__fish_cargo_using_subcommand cargo-cleanme; and __fish_seen_subcommand_from update" -l stats -d 'Print detailed scan/cleanup counters and phase timings to stderr. Never changes deterministic stdout. `--no-progress --stats` is the canonical benchmark/debug combination'
complete -c cargo -n "__fish_cargo_using_subcommand cargo-cleanme; and __fish_seen_subcommand_from update" -s h -l help -d 'Print help (see more with \'--help\')'
complete -c cargo -n "__fish_cargo_using_subcommand help; and not __fish_seen_subcommand_from cargo-cleanme help" -f -a "cargo-cleanme" -d 'Find inactive Cargo artifacts and safely preview their cleanup'
complete -c cargo -n "__fish_cargo_using_subcommand help; and not __fish_seen_subcommand_from cargo-cleanme help" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c cargo -n "__fish_cargo_using_subcommand help; and __fish_seen_subcommand_from cargo-cleanme" -f -a "scan"
complete -c cargo -n "__fish_cargo_using_subcommand help; and __fish_seen_subcommand_from cargo-cleanme" -f -a "config"
complete -c cargo -n "__fish_cargo_using_subcommand help; and __fish_seen_subcommand_from cargo-cleanme" -f -a "clean" -d 'Preview or execute cleanup of revalidated Cargo build artifacts'
complete -c cargo -n "__fish_cargo_using_subcommand help; and __fish_seen_subcommand_from cargo-cleanme" -f -a "update" -d 'Update this cargo-cleanme to the latest stable published release'
