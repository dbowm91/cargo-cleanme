
using namespace System.Management.Automation
using namespace System.Management.Automation.Language

Register-ArgumentCompleter -Native -CommandName 'cargo' -ScriptBlock {
    param($wordToComplete, $commandAst, $cursorPosition)

    $commandElements = $commandAst.CommandElements
    $command = @(
        'cargo'
        for ($i = 1; $i -lt $commandElements.Count; $i++) {
            $element = $commandElements[$i]
            if ($element -isnot [StringConstantExpressionAst] -or
                $element.StringConstantType -ne [StringConstantType]::BareWord -or
                $element.Value.StartsWith('-') -or
                $element.Value -eq $wordToComplete) {
                break
        }
        $element.Value
    }) -join ';'

    $completions = @(switch ($command) {
        'cargo' {
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('cargo-cleanme', 'cargo-cleanme', [CompletionResultType]::ParameterValue, 'Clean inactive Cargo build artifacts through Cargo, or reconcile the machine read-only')
            [CompletionResult]::new('help', 'help', [CompletionResultType]::ParameterValue, 'Print this message or the help of the given subcommand(s)')
            break
        }
        'cargo;cargo-cleanme' {
            [CompletionResult]::new('--config', '--config', [CompletionResultType]::ParameterName, 'config')
            [CompletionResult]::new('--format', '--format', [CompletionResultType]::ParameterName, 'Select human-readable output, the versioned JSON automation contract, or one bounded ASCII summary line for unattended schedulers')
            [CompletionResult]::new('--no-progress', '--no-progress', [CompletionResultType]::ParameterName, 'Disable transient progress UI (useful for benchmarks and debugging)')
            [CompletionResult]::new('--stats', '--stats', [CompletionResultType]::ParameterName, 'Print detailed scan/cleanup counters and phase timings to stderr. Never changes deterministic stdout. `--no-progress --stats` is the canonical benchmark/debug combination')
            [CompletionResult]::new('--dry-run', '--dry-run', [CompletionResultType]::ParameterName, 'Simulate: run every decision, proof, and reporting step, but invoke no `cargo clean` process at all. Honoured wherever it appears, so the `cargo cleanme --dry-run clean ROOT` spelling cannot fall back to Execute')
            [CompletionResult]::new('--dryrun', '--dryrun', [CompletionResultType]::ParameterName, 'Historical spelling of `--dry-run`; retained as a hidden alias')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('scan', 'scan', [CompletionResultType]::ParameterValue, 'Reconcile the machine read-only: full by default, explicit with ROOT, routine inventory with `--known`')
            [CompletionResult]::new('config', 'config', [CompletionResultType]::ParameterValue, 'config')
            [CompletionResult]::new('clean', 'clean', [CompletionResultType]::ParameterValue, 'Preview or execute cleanup of revalidated Cargo build artifacts')
            [CompletionResult]::new('update', 'update', [CompletionResultType]::ParameterValue, 'Update this cargo-cleanme to the latest stable published release')
            break
        }
        'cargo;cargo-cleanme;scan' {
            [CompletionResult]::new('--config', '--config', [CompletionResultType]::ParameterName, 'config')
            [CompletionResult]::new('--format', '--format', [CompletionResultType]::ParameterName, 'Select human-readable output, the versioned JSON automation contract, or one bounded ASCII summary line for unattended schedulers')
            [CompletionResult]::new('--known', '--known', [CompletionResultType]::ParameterName, 'Routine read-only inventory over the maintenance scope')
            [CompletionResult]::new('--full', '--full', [CompletionResultType]::ParameterName, 'Historical spelling of no-root Full `scan`; retained as a hidden alias')
            [CompletionResult]::new('--no-progress', '--no-progress', [CompletionResultType]::ParameterName, 'Disable transient progress UI (useful for benchmarks and debugging)')
            [CompletionResult]::new('--stats', '--stats', [CompletionResultType]::ParameterName, 'Print detailed scan/cleanup counters and phase timings to stderr. Never changes deterministic stdout. `--no-progress --stats` is the canonical benchmark/debug combination')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            break
        }
        'cargo;cargo-cleanme;config' {
            [CompletionResult]::new('--config', '--config', [CompletionResultType]::ParameterName, 'config')
            [CompletionResult]::new('--format', '--format', [CompletionResultType]::ParameterName, 'Select human-readable output, the versioned JSON automation contract, or one bounded ASCII summary line for unattended schedulers')
            [CompletionResult]::new('--no-progress', '--no-progress', [CompletionResultType]::ParameterName, 'Disable transient progress UI (useful for benchmarks and debugging)')
            [CompletionResult]::new('--stats', '--stats', [CompletionResultType]::ParameterName, 'Print detailed scan/cleanup counters and phase timings to stderr. Never changes deterministic stdout. `--no-progress --stats` is the canonical benchmark/debug combination')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('path', 'path', [CompletionResultType]::ParameterValue, 'path')
            [CompletionResult]::new('show', 'show', [CompletionResultType]::ParameterValue, 'show')
            [CompletionResult]::new('edit', 'edit', [CompletionResultType]::ParameterValue, 'edit')
            break
        }
        'cargo;cargo-cleanme;config;path' {
            [CompletionResult]::new('--config', '--config', [CompletionResultType]::ParameterName, 'config')
            [CompletionResult]::new('--format', '--format', [CompletionResultType]::ParameterName, 'Select human-readable output, the versioned JSON automation contract, or one bounded ASCII summary line for unattended schedulers')
            [CompletionResult]::new('--no-progress', '--no-progress', [CompletionResultType]::ParameterName, 'Disable transient progress UI (useful for benchmarks and debugging)')
            [CompletionResult]::new('--stats', '--stats', [CompletionResultType]::ParameterName, 'Print detailed scan/cleanup counters and phase timings to stderr. Never changes deterministic stdout. `--no-progress --stats` is the canonical benchmark/debug combination')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            break
        }
        'cargo;cargo-cleanme;config;show' {
            [CompletionResult]::new('--config', '--config', [CompletionResultType]::ParameterName, 'config')
            [CompletionResult]::new('--format', '--format', [CompletionResultType]::ParameterName, 'Select human-readable output, the versioned JSON automation contract, or one bounded ASCII summary line for unattended schedulers')
            [CompletionResult]::new('--no-progress', '--no-progress', [CompletionResultType]::ParameterName, 'Disable transient progress UI (useful for benchmarks and debugging)')
            [CompletionResult]::new('--stats', '--stats', [CompletionResultType]::ParameterName, 'Print detailed scan/cleanup counters and phase timings to stderr. Never changes deterministic stdout. `--no-progress --stats` is the canonical benchmark/debug combination')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            break
        }
        'cargo;cargo-cleanme;config;edit' {
            [CompletionResult]::new('--config', '--config', [CompletionResultType]::ParameterName, 'config')
            [CompletionResult]::new('--format', '--format', [CompletionResultType]::ParameterName, 'Select human-readable output, the versioned JSON automation contract, or one bounded ASCII summary line for unattended schedulers')
            [CompletionResult]::new('--no-progress', '--no-progress', [CompletionResultType]::ParameterName, 'Disable transient progress UI (useful for benchmarks and debugging)')
            [CompletionResult]::new('--stats', '--stats', [CompletionResultType]::ParameterName, 'Print detailed scan/cleanup counters and phase timings to stderr. Never changes deterministic stdout. `--no-progress --stats` is the canonical benchmark/debug combination')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            break
        }
        'cargo;cargo-cleanme;clean' {
            [CompletionResult]::new('--min-reclaimable-bytes', '--min-reclaimable-bytes', [CompletionResultType]::ParameterName, 'Require at least this many reclaimable bytes per workspace')
            [CompletionResult]::new('--older-than', '--older-than', [CompletionResultType]::ParameterName, 'Require this many seconds of inactivity')
            [CompletionResult]::new('--include', '--include', [CompletionResultType]::ParameterName, 'Include canonical workspace roots matching this glob (repeatable)')
            [CompletionResult]::new('--exclude', '--exclude', [CompletionResultType]::ParameterName, 'Exclude canonical workspace roots matching this glob (repeatable)')
            [CompletionResult]::new('--profile', '--profile', [CompletionResultType]::ParameterName, 'Ask Cargo to clean one named profile. Selector byte estimates are unknown')
            [CompletionResult]::new('--package', '--package', [CompletionResultType]::ParameterName, 'Clean one validated workspace package through qualified Cargo versions')
            [CompletionResult]::new('--config', '--config', [CompletionResultType]::ParameterName, 'config')
            [CompletionResult]::new('--format', '--format', [CompletionResultType]::ParameterName, 'Select human-readable output, the versioned JSON automation contract, or one bounded ASCII summary line for unattended schedulers')
            [CompletionResult]::new('--known', '--known', [CompletionResultType]::ParameterName, 'Clean bounded roots selected by the current Routine policy')
            [CompletionResult]::new('--full', '--full', [CompletionResultType]::ParameterName, 'Complete Full reconciliation before cleaning its learned roots')
            [CompletionResult]::new('--dry-run', '--dry-run', [CompletionResultType]::ParameterName, 'Application simulation: run the full decision/proof/report path but invoke no Cargo clean command. The default mode is Execute')
            [CompletionResult]::new('--cargo-preview', '--cargo-preview', [CompletionResultType]::ParameterName, 'Cargo preview: after the same complete final proof, invoke Cargo''s own `clean --dry-run --verbose`. Never removes anything')
            [CompletionResult]::new('--yes', '--yes', [CompletionResultType]::ParameterName, 'Historical spelling of the (now default) Execute mode; retained as a hidden alias')
            [CompletionResult]::new('--dryrun', '--dryrun', [CompletionResultType]::ParameterName, 'Historical spelling of `--dry-run`; retained as a hidden alias')
            [CompletionResult]::new('--no-progress', '--no-progress', [CompletionResultType]::ParameterName, 'Disable transient progress UI (useful for benchmarks and debugging)')
            [CompletionResult]::new('--stats', '--stats', [CompletionResultType]::ParameterName, 'Print detailed scan/cleanup counters and phase timings to stderr. Never changes deterministic stdout. `--no-progress --stats` is the canonical benchmark/debug combination')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            break
        }
        'cargo;cargo-cleanme;update' {
            [CompletionResult]::new('--config', '--config', [CompletionResultType]::ParameterName, 'config')
            [CompletionResult]::new('--format', '--format', [CompletionResultType]::ParameterName, 'Select human-readable output, the versioned JSON automation contract, or one bounded ASCII summary line for unattended schedulers')
            [CompletionResult]::new('--dry-run', '--dry-run', [CompletionResultType]::ParameterName, 'Resolve the plan and report it without acquiring or replacing bytes')
            [CompletionResult]::new('--no-progress', '--no-progress', [CompletionResultType]::ParameterName, 'Disable transient progress UI (useful for benchmarks and debugging)')
            [CompletionResult]::new('--stats', '--stats', [CompletionResultType]::ParameterName, 'Print detailed scan/cleanup counters and phase timings to stderr. Never changes deterministic stdout. `--no-progress --stats` is the canonical benchmark/debug combination')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            break
        }
        'cargo;help' {
            [CompletionResult]::new('cargo-cleanme', 'cargo-cleanme', [CompletionResultType]::ParameterValue, 'Clean inactive Cargo build artifacts through Cargo, or reconcile the machine read-only')
            [CompletionResult]::new('help', 'help', [CompletionResultType]::ParameterValue, 'Print this message or the help of the given subcommand(s)')
            break
        }
        'cargo;help;cargo-cleanme' {
            [CompletionResult]::new('scan', 'scan', [CompletionResultType]::ParameterValue, 'Reconcile the machine read-only: full by default, explicit with ROOT, routine inventory with `--known`')
            [CompletionResult]::new('config', 'config', [CompletionResultType]::ParameterValue, 'config')
            [CompletionResult]::new('clean', 'clean', [CompletionResultType]::ParameterValue, 'Preview or execute cleanup of revalidated Cargo build artifacts')
            [CompletionResult]::new('update', 'update', [CompletionResultType]::ParameterValue, 'Update this cargo-cleanme to the latest stable published release')
            break
        }
        'cargo;help;cargo-cleanme;scan' {
            break
        }
        'cargo;help;cargo-cleanme;config' {
            [CompletionResult]::new('path', 'path', [CompletionResultType]::ParameterValue, 'path')
            [CompletionResult]::new('show', 'show', [CompletionResultType]::ParameterValue, 'show')
            [CompletionResult]::new('edit', 'edit', [CompletionResultType]::ParameterValue, 'edit')
            break
        }
        'cargo;help;cargo-cleanme;config;path' {
            break
        }
        'cargo;help;cargo-cleanme;config;show' {
            break
        }
        'cargo;help;cargo-cleanme;config;edit' {
            break
        }
        'cargo;help;cargo-cleanme;clean' {
            break
        }
        'cargo;help;cargo-cleanme;update' {
            break
        }
        'cargo;help;help' {
            break
        }
    })

    $completions.Where{ $_.CompletionText -like "$wordToComplete*" } |
        Sort-Object -Property ListItemText
}
