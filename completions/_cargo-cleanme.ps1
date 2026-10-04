
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
            [CompletionResult]::new('cargo-cleanme', 'cargo-cleanme', [CompletionResultType]::ParameterValue, 'Find inactive Cargo artifacts and safely preview their cleanup')
            [CompletionResult]::new('help', 'help', [CompletionResultType]::ParameterValue, 'Print this message or the help of the given subcommand(s)')
            break
        }
        'cargo;cargo-cleanme' {
            [CompletionResult]::new('--config', '--config', [CompletionResultType]::ParameterName, 'config')
            [CompletionResult]::new('--format', '--format', [CompletionResultType]::ParameterName, 'Select human-readable output or the versioned JSON automation contract')
            [CompletionResult]::new('--no-progress', '--no-progress', [CompletionResultType]::ParameterName, 'Disable transient progress UI (useful for benchmarks and debugging)')
            [CompletionResult]::new('--stats', '--stats', [CompletionResultType]::ParameterName, 'Print detailed scan/cleanup counters and phase timings to stderr. Never changes deterministic stdout. `--no-progress --stats` is the canonical benchmark/debug combination')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('scan', 'scan', [CompletionResultType]::ParameterValue, 'scan')
            [CompletionResult]::new('config', 'config', [CompletionResultType]::ParameterValue, 'config')
            [CompletionResult]::new('clean', 'clean', [CompletionResultType]::ParameterValue, 'Preview or execute cleanup of revalidated Cargo build artifacts')
            [CompletionResult]::new('update', 'update', [CompletionResultType]::ParameterValue, 'Update this cargo-cleanme to the latest stable published release')
            break
        }
        'cargo;cargo-cleanme;scan' {
            [CompletionResult]::new('--config', '--config', [CompletionResultType]::ParameterName, 'config')
            [CompletionResult]::new('--format', '--format', [CompletionResultType]::ParameterName, 'Select human-readable output or the versioned JSON automation contract')
            [CompletionResult]::new('--full', '--full', [CompletionResultType]::ParameterName, 'full')
            [CompletionResult]::new('--no-progress', '--no-progress', [CompletionResultType]::ParameterName, 'Disable transient progress UI (useful for benchmarks and debugging)')
            [CompletionResult]::new('--stats', '--stats', [CompletionResultType]::ParameterName, 'Print detailed scan/cleanup counters and phase timings to stderr. Never changes deterministic stdout. `--no-progress --stats` is the canonical benchmark/debug combination')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            break
        }
        'cargo;cargo-cleanme;config' {
            [CompletionResult]::new('--config', '--config', [CompletionResultType]::ParameterName, 'config')
            [CompletionResult]::new('--format', '--format', [CompletionResultType]::ParameterName, 'Select human-readable output or the versioned JSON automation contract')
            [CompletionResult]::new('--no-progress', '--no-progress', [CompletionResultType]::ParameterName, 'Disable transient progress UI (useful for benchmarks and debugging)')
            [CompletionResult]::new('--stats', '--stats', [CompletionResultType]::ParameterName, 'Print detailed scan/cleanup counters and phase timings to stderr. Never changes deterministic stdout. `--no-progress --stats` is the canonical benchmark/debug combination')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('path', 'path', [CompletionResultType]::ParameterValue, 'path')
            [CompletionResult]::new('show', 'show', [CompletionResultType]::ParameterValue, 'show')
            [CompletionResult]::new('edit', 'edit', [CompletionResultType]::ParameterValue, 'edit')
            break
        }
        'cargo;cargo-cleanme;config;path' {
            [CompletionResult]::new('--config', '--config', [CompletionResultType]::ParameterName, 'config')
            [CompletionResult]::new('--format', '--format', [CompletionResultType]::ParameterName, 'Select human-readable output or the versioned JSON automation contract')
            [CompletionResult]::new('--no-progress', '--no-progress', [CompletionResultType]::ParameterName, 'Disable transient progress UI (useful for benchmarks and debugging)')
            [CompletionResult]::new('--stats', '--stats', [CompletionResultType]::ParameterName, 'Print detailed scan/cleanup counters and phase timings to stderr. Never changes deterministic stdout. `--no-progress --stats` is the canonical benchmark/debug combination')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            break
        }
        'cargo;cargo-cleanme;config;show' {
            [CompletionResult]::new('--config', '--config', [CompletionResultType]::ParameterName, 'config')
            [CompletionResult]::new('--format', '--format', [CompletionResultType]::ParameterName, 'Select human-readable output or the versioned JSON automation contract')
            [CompletionResult]::new('--no-progress', '--no-progress', [CompletionResultType]::ParameterName, 'Disable transient progress UI (useful for benchmarks and debugging)')
            [CompletionResult]::new('--stats', '--stats', [CompletionResultType]::ParameterName, 'Print detailed scan/cleanup counters and phase timings to stderr. Never changes deterministic stdout. `--no-progress --stats` is the canonical benchmark/debug combination')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            break
        }
        'cargo;cargo-cleanme;config;edit' {
            [CompletionResult]::new('--config', '--config', [CompletionResultType]::ParameterName, 'config')
            [CompletionResult]::new('--format', '--format', [CompletionResultType]::ParameterName, 'Select human-readable output or the versioned JSON automation contract')
            [CompletionResult]::new('--no-progress', '--no-progress', [CompletionResultType]::ParameterName, 'Disable transient progress UI (useful for benchmarks and debugging)')
            [CompletionResult]::new('--stats', '--stats', [CompletionResultType]::ParameterName, 'Print detailed scan/cleanup counters and phase timings to stderr. Never changes deterministic stdout. `--no-progress --stats` is the canonical benchmark/debug combination')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
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
            [CompletionResult]::new('--format', '--format', [CompletionResultType]::ParameterName, 'Select human-readable output or the versioned JSON automation contract')
            [CompletionResult]::new('--known', '--known', [CompletionResultType]::ParameterName, 'Clean bounded roots selected by the current Routine policy')
            [CompletionResult]::new('--full', '--full', [CompletionResultType]::ParameterName, 'Complete Full reconciliation before cleaning its learned roots')
            [CompletionResult]::new('--dry-run', '--dry-run', [CompletionResultType]::ParameterName, 'Cargo preview: invoke Cargo''s own dry-run (the default mode). Distinct from `--dryrun` (cargo-cleanme simulation)')
            [CompletionResult]::new('--dryrun', '--dryrun', [CompletionResultType]::ParameterName, 'Application simulation: run the full decision/progress/report path but invoke no Cargo clean command. Distinct from `--dry-run`')
            [CompletionResult]::new('--yes', '--yes', [CompletionResultType]::ParameterName, 'Execute Cargo clean after per-workspace revalidation')
            [CompletionResult]::new('--no-progress', '--no-progress', [CompletionResultType]::ParameterName, 'Disable transient progress UI (useful for benchmarks and debugging)')
            [CompletionResult]::new('--stats', '--stats', [CompletionResultType]::ParameterName, 'Print detailed scan/cleanup counters and phase timings to stderr. Never changes deterministic stdout. `--no-progress --stats` is the canonical benchmark/debug combination')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            break
        }
        'cargo;cargo-cleanme;update' {
            [CompletionResult]::new('--config', '--config', [CompletionResultType]::ParameterName, 'config')
            [CompletionResult]::new('--format', '--format', [CompletionResultType]::ParameterName, 'Select human-readable output or the versioned JSON automation contract')
            [CompletionResult]::new('--dry-run', '--dry-run', [CompletionResultType]::ParameterName, 'Resolve the plan and report it without acquiring or replacing bytes')
            [CompletionResult]::new('--no-progress', '--no-progress', [CompletionResultType]::ParameterName, 'Disable transient progress UI (useful for benchmarks and debugging)')
            [CompletionResult]::new('--stats', '--stats', [CompletionResultType]::ParameterName, 'Print detailed scan/cleanup counters and phase timings to stderr. Never changes deterministic stdout. `--no-progress --stats` is the canonical benchmark/debug combination')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            break
        }
        'cargo;help' {
            [CompletionResult]::new('cargo-cleanme', 'cargo-cleanme', [CompletionResultType]::ParameterValue, 'Find inactive Cargo artifacts and safely preview their cleanup')
            [CompletionResult]::new('help', 'help', [CompletionResultType]::ParameterValue, 'Print this message or the help of the given subcommand(s)')
            break
        }
        'cargo;help;cargo-cleanme' {
            [CompletionResult]::new('scan', 'scan', [CompletionResultType]::ParameterValue, 'scan')
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
