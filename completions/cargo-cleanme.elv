
use builtin;
use str;

set edit:completion:arg-completer[cargo] = {|@words|
    fn spaces {|n|
        builtin:repeat $n ' ' | str:join ''
    }
    fn cand {|text desc|
        edit:complex-candidate $text &display=$text' '(spaces (- 14 (wcswidth $text)))$desc
    }
    var command = 'cargo'
    for word $words[1..-1] {
        if (str:has-prefix $word '-') {
            break
        }
        set command = $command';'$word
    }
    var completions = [
        &'cargo'= {
            cand -h 'Print help'
            cand --help 'Print help'
            cand cargo-cleanme 'Clean inactive Cargo build artifacts through Cargo, or reconcile the machine read-only'
            cand help 'Print this message or the help of the given subcommand(s)'
        }
        &'cargo;cargo-cleanme'= {
            cand --config 'config'
            cand --format 'Select human-readable output, the versioned JSON automation contract, or one bounded ASCII summary line for unattended schedulers'
            cand --no-progress 'Disable transient progress UI (useful for benchmarks and debugging)'
            cand --stats 'Print detailed scan/cleanup counters and phase timings to stderr. Never changes deterministic stdout. `--no-progress --stats` is the canonical benchmark/debug combination'
            cand --dry-run 'Simulate routine maintenance: run every decision, proof, and reporting step, but invoke no `cargo clean` process at all'
            cand --dryrun 'Historical spelling of `--dry-run`; retained as a hidden alias'
            cand -h 'Print help (see more with ''--help'')'
            cand --help 'Print help (see more with ''--help'')'
            cand -V 'Print version'
            cand --version 'Print version'
            cand scan 'Reconcile the machine read-only: full by default, explicit with ROOT, routine inventory with `--known`'
            cand config 'config'
            cand clean 'Preview or execute cleanup of revalidated Cargo build artifacts'
            cand update 'Update this cargo-cleanme to the latest stable published release'
        }
        &'cargo;cargo-cleanme;scan'= {
            cand --config 'config'
            cand --format 'Select human-readable output, the versioned JSON automation contract, or one bounded ASCII summary line for unattended schedulers'
            cand --known 'Routine read-only inventory over the maintenance scope'
            cand --full 'Historical spelling of no-root Full `scan`; retained as a hidden alias'
            cand --no-progress 'Disable transient progress UI (useful for benchmarks and debugging)'
            cand --stats 'Print detailed scan/cleanup counters and phase timings to stderr. Never changes deterministic stdout. `--no-progress --stats` is the canonical benchmark/debug combination'
            cand -h 'Print help (see more with ''--help'')'
            cand --help 'Print help (see more with ''--help'')'
        }
        &'cargo;cargo-cleanme;config'= {
            cand --config 'config'
            cand --format 'Select human-readable output, the versioned JSON automation contract, or one bounded ASCII summary line for unattended schedulers'
            cand --no-progress 'Disable transient progress UI (useful for benchmarks and debugging)'
            cand --stats 'Print detailed scan/cleanup counters and phase timings to stderr. Never changes deterministic stdout. `--no-progress --stats` is the canonical benchmark/debug combination'
            cand -h 'Print help (see more with ''--help'')'
            cand --help 'Print help (see more with ''--help'')'
            cand path 'path'
            cand show 'show'
            cand edit 'edit'
        }
        &'cargo;cargo-cleanme;config;path'= {
            cand --config 'config'
            cand --format 'Select human-readable output, the versioned JSON automation contract, or one bounded ASCII summary line for unattended schedulers'
            cand --no-progress 'Disable transient progress UI (useful for benchmarks and debugging)'
            cand --stats 'Print detailed scan/cleanup counters and phase timings to stderr. Never changes deterministic stdout. `--no-progress --stats` is the canonical benchmark/debug combination'
            cand -h 'Print help (see more with ''--help'')'
            cand --help 'Print help (see more with ''--help'')'
        }
        &'cargo;cargo-cleanme;config;show'= {
            cand --config 'config'
            cand --format 'Select human-readable output, the versioned JSON automation contract, or one bounded ASCII summary line for unattended schedulers'
            cand --no-progress 'Disable transient progress UI (useful for benchmarks and debugging)'
            cand --stats 'Print detailed scan/cleanup counters and phase timings to stderr. Never changes deterministic stdout. `--no-progress --stats` is the canonical benchmark/debug combination'
            cand -h 'Print help (see more with ''--help'')'
            cand --help 'Print help (see more with ''--help'')'
        }
        &'cargo;cargo-cleanme;config;edit'= {
            cand --config 'config'
            cand --format 'Select human-readable output, the versioned JSON automation contract, or one bounded ASCII summary line for unattended schedulers'
            cand --no-progress 'Disable transient progress UI (useful for benchmarks and debugging)'
            cand --stats 'Print detailed scan/cleanup counters and phase timings to stderr. Never changes deterministic stdout. `--no-progress --stats` is the canonical benchmark/debug combination'
            cand -h 'Print help (see more with ''--help'')'
            cand --help 'Print help (see more with ''--help'')'
        }
        &'cargo;cargo-cleanme;clean'= {
            cand --min-reclaimable-bytes 'Require at least this many reclaimable bytes per workspace'
            cand --older-than 'Require this many seconds of inactivity'
            cand --include 'Include canonical workspace roots matching this glob (repeatable)'
            cand --exclude 'Exclude canonical workspace roots matching this glob (repeatable)'
            cand --profile 'Ask Cargo to clean one named profile. Selector byte estimates are unknown'
            cand --package 'Clean one validated workspace package through qualified Cargo versions'
            cand --config 'config'
            cand --format 'Select human-readable output, the versioned JSON automation contract, or one bounded ASCII summary line for unattended schedulers'
            cand --known 'Clean bounded roots selected by the current Routine policy'
            cand --full 'Complete Full reconciliation before cleaning its learned roots'
            cand --dry-run 'Application simulation: run the full decision/proof/report path but invoke no Cargo clean command. The default mode is Execute'
            cand --cargo-preview 'Cargo preview: after the same complete final proof, invoke Cargo''s own `clean --dry-run --verbose`. Never removes anything'
            cand --yes 'Historical spelling of the (now default) Execute mode; retained as a hidden alias'
            cand --dryrun 'Historical spelling of `--dry-run`; retained as a hidden alias'
            cand --no-progress 'Disable transient progress UI (useful for benchmarks and debugging)'
            cand --stats 'Print detailed scan/cleanup counters and phase timings to stderr. Never changes deterministic stdout. `--no-progress --stats` is the canonical benchmark/debug combination'
            cand -h 'Print help (see more with ''--help'')'
            cand --help 'Print help (see more with ''--help'')'
        }
        &'cargo;cargo-cleanme;update'= {
            cand --config 'config'
            cand --format 'Select human-readable output, the versioned JSON automation contract, or one bounded ASCII summary line for unattended schedulers'
            cand --dry-run 'Resolve the plan and report it without acquiring or replacing bytes'
            cand --no-progress 'Disable transient progress UI (useful for benchmarks and debugging)'
            cand --stats 'Print detailed scan/cleanup counters and phase timings to stderr. Never changes deterministic stdout. `--no-progress --stats` is the canonical benchmark/debug combination'
            cand -h 'Print help (see more with ''--help'')'
            cand --help 'Print help (see more with ''--help'')'
        }
        &'cargo;help'= {
            cand cargo-cleanme 'Clean inactive Cargo build artifacts through Cargo, or reconcile the machine read-only'
            cand help 'Print this message or the help of the given subcommand(s)'
        }
        &'cargo;help;cargo-cleanme'= {
            cand scan 'Reconcile the machine read-only: full by default, explicit with ROOT, routine inventory with `--known`'
            cand config 'config'
            cand clean 'Preview or execute cleanup of revalidated Cargo build artifacts'
            cand update 'Update this cargo-cleanme to the latest stable published release'
        }
        &'cargo;help;cargo-cleanme;scan'= {
        }
        &'cargo;help;cargo-cleanme;config'= {
            cand path 'path'
            cand show 'show'
            cand edit 'edit'
        }
        &'cargo;help;cargo-cleanme;config;path'= {
        }
        &'cargo;help;cargo-cleanme;config;show'= {
        }
        &'cargo;help;cargo-cleanme;config;edit'= {
        }
        &'cargo;help;cargo-cleanme;clean'= {
        }
        &'cargo;help;cargo-cleanme;update'= {
        }
        &'cargo;help;help'= {
        }
    ]
    $completions[$command]
}
