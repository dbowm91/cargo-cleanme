# M012B — Bounded Unattended Log Output and greggd Integration

Status: closed — see `plans/closure/artifact-discovery-cleanup/m012b-status.md`

Repository baseline: `e4e9d92673e5f10548248e23db788c0906760916`

Source roadmap: Phase 12 — canonical maintenance UX and unattended operation

Subsystem roadmap: `plans/subsystems/artifact-discovery-cleanup-roadmap.md`

Decision: `plans/adr/003-canonical-maintenance-invocation-and-unattended-output.md`

Primary class: operational output / documentation integration

Dependencies: interface dependency on M012A's final operation/scope/mode model. Rendering/DTO work may proceed in parallel if it does not assume the old invocation semantics.

## 1. Objective

Add an explicit, compact output mode for unattended maintenance and document a first-class greggd scheduling pattern.

Target usage:

~~~text
cargo cleanme --format log
cargo cleanme scan --format log
~~~

The output must be useful when retained as a short scheduler-history tail, while complete machine consumers continue using `--format json`.

## 2. Non-goals

- no daemon or scheduler inside cargo-cleanme;
- no cron-expression parser;
- no load sampling or retry/backoff policy;
- no persistent cargo-cleanme log database;
- no shell wrapper generation;
- no replacement of JSON as the complete automation contract;
- no remote Gregg API integration from cargo-cleanme;
- no dependency on Gregg at runtime.

## 3. Current ecosystem evidence

cargo-cleanme today has `human|json` output. Human cleanup can emit per-unit rows and diagnostic detail; progress is stderr-only and suppressed for non-TTY/JSON. JSON is complete and deterministic but verbose for a retained operator-history pane.

Current greggd scheduled maintenance:

- executes an argv array directly without a shell;
- captures stdout and stderr independently into bounded in-memory tails;
- defaults to five retained terminal records per job;
- bounds each raw stream tail to 1024 bytes and publishes at most 512 JSON-escaped bytes;
- serializes scheduled commands globally;
- can defer on 1m/5m/15m cached load averages;
- defaults to five-minute retry and 24-hour max wait;
- runs jobs as greggd's OS principal;
- the normal Linux system service uses the `greggd` service account and `ProtectHome=true`, so developer-home maintenance generally requires a user-owned/rootless daemon.

Gregg's current documentation also contains a stale cargo-cleanme example using `scan --deep`; cargo-cleanme's documentation should not reproduce it. A downstream Gregg documentation correction is required after the new cargo-cleanme contract lands, but this plan does not mutate another repository.

## 4. Output contract

Extend the public format enum to:

~~~text
human
json
log
~~~

For scan and cleanup, `log` must emit exactly one ASCII line to stdout when a report is available.

Recommended shape:

~~~text
cargo-cleanme op=clean status=ok scope=routine mode=execute cleaned=7 skipped=3 failed=0 reclaimed_bytes=19778387968 diagnostics=0
cargo-cleanme op=scan status=ok scope=full manifests=184 groups=31 bytes=51335569408 diagnostics=2
cargo-cleanme op=clean status=blocked scope=routine mode=execute cleaned=0 skipped=0 failed=0 reason=ownership_unproven diagnostics=1
~~~

Exact field names may differ if existing DTO vocabulary provides a better stable mapping, but the renderer must obey these rules:

- prefix identifies cargo-cleanme;
- `op`, `status`, and `scope` are always present for report-producing operations;
- cleanup includes `mode`;
- counts/bytes are base-10 ASCII integers, not locale-dependent strings;
- values contain no whitespace requiring quoting;
- no path, arbitrary Cargo stderr, or unbounded free text appears on stdout;
- one newline terminates the line;
- deterministic field order;
- hard maximum 384 bytes including prefix but excluding the final newline;
- if optional detail would exceed the bound, omit optional fields before truncating anything structural;
- never split UTF-8 because output is ASCII by contract.

The 384-byte cap intentionally leaves headroom under Gregg's 512-byte published escaped tail.

## 5. Status and reason semantics

Define bounded statuses from typed report state rather than prose:

- `ok` — operation completed and no unit/report failure requires exit 1;
- `blocked` — cleanup scope is safety-blocked before mutation;
- `failed` — one or more cleanup operations failed or Full scan is incomplete under existing exit semantics.

Where a typed blocker/reason exists, render its stable code. If current scope-blocking state stores only prose, introduce the minimum typed reason needed for log/JSON consistency rather than parsing English strings.

Do not create a parallel reason taxonomy solely for log mode.

## 6. Stdout/stderr discipline

In log mode:

- progress is disabled regardless of TTY;
- successful scan/cleanup writes the one-line summary to stdout and normally nothing to stderr;
- scope-blocked/operation-failed reports still write one summary line to stdout and preserve the existing nonzero exit status;
- normal per-diagnostic stderr fan-out is suppressed; the summary carries `diagnostics=N`;
- `--stats` remains an explicit debugging override and may write its existing stderr detail, but scheduler examples must not use it;
- pre-report fatal errors write no normal summary and emit exactly one bounded single-line stderr diagnostic after CLI parsing has established log mode;
- Clap usage errors may retain Clap's standard behavior; do not reimplement the parser error stack merely to force the log renderer.

Ensure Cargo child output cannot escape into unattended stdout/stderr unboundedly. Existing capture/report boundaries should remain authoritative.

## 7. Reuse report data; do not add expensive work

The log renderer must project information already produced by ScanReport/CleanReport or state reconciliation.

Do not:

- re-walk the filesystem;
- re-run Cargo;
- re-read discovery state solely to decorate a line;
- compute selector-specific reclaimable bytes that the product deliberately treats as unknown.

If learned/pruned-root counts are not already cheaply available, omit them from the initial log line rather than widening state APIs only for cosmetics.

## 8. JSON and human compatibility

- Human output remains the interactive/detail surface.
- JSON remains the complete versioned machine contract.
- Log is a bounded operational summary and must be documented as unsuitable for software that needs complete per-unit detail.
- Non-TTY stdout must not automatically select log mode.
- Existing `--format human` and `--format json` output must remain byte/semantic compatible except where M012A intentionally changes resolved operation/mode.

If adding `log` changes the generated completions/manpage, regenerate them from Clap.

## 9. greggd integration documentation

Add a dedicated unattended-maintenance section, preferably `docs/AUTOMATION.md` linked from README and USAGE.

Document an example equivalent to:

~~~toml
# User-owned/rootless greggd recommended for developer-home maintenance.
scheduler_history_limit = 5

[[jobs]]
name = "cargo-cleanme-daily"
schedule = "0 2 * * *"
command = ["/home/user/.cargo/bin/cargo-cleanme", "--format", "log"]
max_load = 8.0
load_window = "15m"
retry_interval_ms = 300000
max_wait_ms = 86400000

[[jobs]]
name = "cargo-cleanme-weekly-scan"
schedule = "0 3 * * 0"
command = ["/home/user/.cargo/bin/cargo-cleanme", "scan", "--format", "log"]
max_load = 8.0
load_window = "15m"
retry_interval_ms = 300000
max_wait_ms = 86400000
~~~

Requirements for the prose:

- use an absolute cargo-cleanme executable path; do not assume a service PATH;
- explain that commands are argv arrays, not shell strings;
- explain daily Routine cleanup versus weekly Full reconciliation;
- explain that Full scan is read-only but may be expensive;
- explain that load gating/retry/max-wait belong to Gregg, not cargo-cleanme;
- warn that system greggd normally cannot access a developer's home and root/euid job restrictions also apply;
- recommend a user-owned/rootless greggd instance for this workload;
- note that scheduler history is bounded/in-memory and the compact log format is designed to fit it;
- do not place credentials in argv or cargo-cleanme output;
- do not claim Gregg authentication/TLS that it does not provide;
- keep generic cron/systemd examples scheduler-neutral where useful.

## 10. Downstream Gregg reconciliation

Record a non-blocking downstream handoff note: Gregg's `docs/daemon.md` currently shows `cargo-cleanme scan --deep`, which is stale.

After M012A/B lands, Gregg should replace that example with the canonical daily/weekly commands and link to cargo-cleanme's automation documentation if appropriate.

This cargo-cleanme plan must not make its closure depend on a commit in another repository unless maintainers explicitly convert that note into a cross-repo dependency.

## 11. Required tests

At minimum:

1. `--format log` parses globally;
2. Routine Execute summary is one line, ASCII, deterministic, <=384 bytes;
3. Routine Simulate summary reports mode=simulate and zero Cargo-clean spawns;
4. CargoPreview reports mode=preview without claiming reclaimed bytes;
5. Full scan summary reports scope=full;
6. Explicit scan reports scope=explicit;
7. Routine read-only `scan --known` reports scope=routine;
8. scope-blocked cleanup returns exit 1 with one stdout summary containing a typed reason;
9. failed cleanup returns exit 1 with status=failed;
10. successful zero-result maintenance remains status=ok;
11. progress/control sequences never appear in log mode even on an attended TTY renderer path;
12. normal diagnostic fan-out is absent in log mode;
13. `--stats` remains opt-in stderr and does not alter the one-line stdout;
14. JSON output is unchanged by the existence of log mode;
15. human output is unchanged apart from M012A's intentional invocation/mode wording;
16. a synthetic worst-case summary proves the hard bound without cutting structural fields;
17. a premise-negative test feeds an overlong optional diagnostic/reason source and proves the renderer stays bounded rather than leaking text.

## 12. Verification

Run:

~~~text
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets --all-features
python3 scripts/check-fixture-portability.py --self-test
python3 scripts/check-fixture-portability.py
python3 scripts/check-doc-citations.py --self-test
python3 scripts/check-doc-citations.py
cargo run --quiet --features dev-tools --bin generate-docs
cargo run --quiet --features dev-tools --bin generate-docs -- --check
~~~

Also exercise the examples manually with a temporary config/fixture and capture exact stdout/stderr byte counts.

If Gregg is locally available, an optional integration rehearsal may schedule a harmless cargo-cleanme simulation job and confirm its retained history contains the complete summary line. Do not make closure depend on an unavailable external binary.

## 13. Documentation updates

Update:

- README quickstart/automation pointer;
- docs/USAGE.md output-format section;
- new docs/AUTOMATION.md (recommended);
- docs/TROUBLESHOOTING.md for scheduler PATH/permissions and interpreting nonzero exit;
- architecture/10-reporting.md;
- architecture/11-progress.md;
- architecture/13-orchestration.md;
- architecture/14-testing-and-verification.md;
- plans/output-schema-v1.md only to clarify that log is separate from JSON, not to redefine JSON;
- generated manpage/completions.

Do not copy volatile Gregg implementation internals beyond what is necessary to explain the integration; cite/version the integration assumptions in prose if the repository's doc conventions require it.

## 14. Acceptance criteria

M012B closes when:

- `--format log` exists and produces one bounded deterministic ASCII summary line for scan/cleanup reports;
- log mode disables progress and normal diagnostic fan-out;
- existing exit semantics are preserved;
- JSON remains the complete machine-readable contract;
- no expensive work is added merely for log decoration;
- cargo-cleanme documents daily Routine cleanup + weekly Full scan under greggd load gating;
- the system-daemon/home-access caveat is explicit;
- a downstream Gregg stale-example handoff is recorded;
- all repository verification gates pass.

## 15. Stop conditions

Stop and write a follow-up if:

- a useful log summary cannot be derived from existing reports without another filesystem/Cargo pass;
- stable status/reason output requires incompatible JSON schema-v1 changes;
- child-process output can bypass existing capture bounds and needs a larger process-boundary redesign;
- Gregg integration would require cargo-cleanme to talk to Gregg's HTTP API;
- an internal scheduler/daemon begins to emerge from the implementation.

## 16. Closure evidence

Record:

- implementation commit/PR;
- exact log grammar and byte bound;
- representative clean/simulate/preview/scan/blocked/failed output samples with byte lengths;
- proof that progress and diagnostic fan-out are absent;
- JSON/human regression results;
- verification ladder results;
- generated-doc drift result;
- final automation documentation excerpt;
- downstream Gregg handoff reference;
- unresolved findings and disposition.
