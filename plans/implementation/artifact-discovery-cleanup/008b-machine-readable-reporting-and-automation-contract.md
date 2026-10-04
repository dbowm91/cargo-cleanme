# M008B — Machine-Readable Reporting and Automation Contract

Status: blocked on M008A public policy/disposition interface

Repository baseline: `3ee9699a0b0d987287d08e427195284e08d079f7`

Source roadmap: Phase 9 — Selective cleanup and policy

Primary class: stable output contract / unattended-operation safety

Hard dependency:

- M008A must close, or at minimum its typed cleanup-policy and disposition interface must be explicitly frozen before M008B implementation begins.

Soft dependencies:

- none from Phase 10 distribution work.

## 1. Objective

Add a versioned, deterministic machine-readable output contract for scan and cleanup operations, then define the process/exit-code semantics required for safe non-interactive orchestration.

M008B does not add a scheduler, daemon, cron implementation, background service, or new destructive authority. It makes the existing read-only and Cargo-mediated operations consumable by external automation without requiring parsers to scrape human text.

The central principle is:

> automation may depend on stable typed results, never on rendered prose, transient progress, or debug counters.

## 2. Why this milestone follows M008A

The existing report structures contain useful data, but several cleanup decisions are represented primarily by human `detail` strings. M008A introduces typed policy dispositions and fresh pre-spawn policy decisions. M008B freezes the externally visible semantic vocabulary only after those distinctions exist.

The repository already depends on `serde` and `serde_json`; no new serialization dependency is expected.

## 3. Public CLI contract

Preferred direction:

~~~text
cargo-cleanme [GLOBAL OPTIONS] scan [ROOT|--full] [--format human|json]
cargo-cleanme [GLOBAL OPTIONS] clean ROOT|--known|--full [POLICY] [--dry-run|--dryrun|--yes] [--format human|json]
~~~

Human remains the default.

`--format json` must:

- emit exactly one complete JSON document to stdout;
- never interleave progress rendering with stdout;
- keep diagnostics/progress/debug information out of stdout unless represented as schema fields;
- remain deterministic for identical typed input;
- work for success, partial read-only results, scope-blocked cleanup, per-unit skips/failures, and fatal startup errors where a report can be formed.

A newline-terminated JSON document is preferred for shell ergonomics. NDJSON is out of scope unless separately justified.

## 4. Versioning contract

Do not derive `Serialize` directly on internal operational structs and call that the public API.

Create dedicated output DTOs, for example:

~~~text
OutputEnvelopeV1
ScanOutputV1
CleanupOutputV1
DiagnosticV1
CleanupUnitResultV1
PolicyDispositionV1
SafetyDispositionV1
SummaryV1
~~~

The top-level object must contain at least:

- `schema_version`;
- `cargo_cleanme_version`;
- operation kind;
- operation mode/scope;
- effective policy relevant to the result;
- result payload;
- summary;
- diagnostics.

Schema versioning is independent of the package version. Backward-incompatible field/enum changes require a schema-version decision rather than silently changing meanings.

Prefer string enum values over numeric codes.

## 5. Stable cleanup disposition model

Machine output must distinguish selection policy from safety/operation outcome.

Required policy disposition vocabulary should cover at least:

- `selected`;
- `below_minimum_size`;
- `too_recent_for_policy`;
- `not_included`;
- `excluded`.

Required cleanup/safety outcome vocabulary should cover at least:

- `previewed`;
- `simulated`;
- `cleaned`;
- `skipped_active`;
- `skipped_shared`;
- `skipped_uncertain`;
- `skipped_unauthorized`;
- `skipped_marker_invalid`;
- `skipped_changed_before_cleanup`;
- `scope_blocked_incomplete_ownership`;
- `cargo_failed`;
- `measurement_failed`.

Exact enum factoring may differ. The important requirement is that callers never need to substring-match `detail`.

Human details may remain for explanation, but public automation decisions must derive from typed fields.

## 6. Scan JSON contract

Scan output should include:

- effective scan scope kind: explicit/routine/full;
- selected roots where disclosure is already part of the human/debug contract;
- counts: discovered manifests/workspaces, reportable groups, diagnostics;
- each eligible physical output group:
  - display path;
  - canonical workspace roots;
  - physical covering roots where appropriate;
  - bytes;
  - size metric;
  - newest output activity timestamp when available;
  - ownership classification;
- bounded diagnostics with severity/category/path/message;
- summary reclaimable bytes.

Do not expose machine-local learned state as cleanup authorization evidence. If state-generation metadata is reported, label it as discovery metadata only.

JSON scan semantics must match human scan semantics exactly; format selection cannot change discovery or eligibility.

## 7. Cleanup JSON contract

Cleanup output should include:

- scope selector: explicit/known/full;
- selected canonical roots;
- cleanup mode: preview/simulate/execute;
- effective cleanup policy;
- discovered manifest count;
- resolved workspace count;
- unresolved ownership participants;
- scope-blocked state;
- units considered/selected/policy-skipped/safety-skipped/failed;
- per-unit:
  - canonical workspace identity/root;
  - affected covering output roots;
  - ownership class;
  - policy disposition;
  - cleanup/safety outcome;
  - initial measured bytes if retained;
  - fresh pre-spawn bytes when available;
  - post-clean bytes for Execute;
  - observed decrease for Execute;
  - newest trusted activity timestamp if exposed by M008A;
  - stable reason code;
  - optional human detail;
- deterministic summary totals.

Preview and Simulate must not label estimated bytes as recovered bytes.

Execute must keep pre-clean estimate, post-clean measured bytes, and observed decrease distinct.

## 8. Timestamp/path/number encoding

Define before implementation:

- timestamps: RFC 3339 UTC strings or integer Unix seconds; choose one and test boundary behavior;
- byte counts: JSON integers;
- missing observations: `null`, not sentinel values;
- filesystem paths: valid UTF-8 paths may be emitted as strings.

Non-UTF-8 paths require an explicit cross-platform representation. Do not silently lossy-convert and present the result as canonical identity.

Preferred direction is a path DTO that has:

- a display string;
- and, where needed, a platform/native escaped representation sufficient to distinguish non-UTF-8 Unix paths.

If a portable reversible representation materially expands the milestone, preserve a stable escaped display field and document that machine identity is not guaranteed for non-UTF-8 paths; do not silently drop such candidates.

## 9. Work package A — Output DTO layer

Add a reporting module boundary that converts internal domain/report types into schema-versioned DTOs.

Requirements:

- no filesystem/Cargo side effects during conversion;
- conversion is deterministic and testable with pure fixtures;
- stable enum spelling is centralized;
- schema construction is separate from human rendering;
- internal fields can evolve without automatically changing the public schema.

The existing human report should remain independently rendered from typed results.

## 10. Work package B — Format routing

Add a global or command-compatible format selector.

Requirements:

- human remains default;
- JSON disables transient TTY progress by default or routes all transient state strictly to stderr without ANSI leakage into stdout;
- `--no-progress` remains accepted;
- `--stats` remains stderr-only and never changes the JSON document;
- pipe/non-TTY behavior remains deterministic;
- Cargo external-subcommand invocation behaves identically.

Do not make JSON contingent on a TTY.

## 11. Work package C — Stable reason codes

Refactor cleanup paths that currently return or store free-form strings so the externally relevant decision is represented by a typed code plus optional detail.

This includes:

- whole-scope ownership block;
- M008A policy skips;
- unit-wide ownership/authorization blocks;
- final-proof change/race failures;
- Cargo spawn/exit failure;
- post-clean measurement failure.

Do not overfit every internal error string into the public schema. Group only decisions callers need to act on while retaining `detail` for diagnosis.

A new human message must not require a schema version bump if its stable code is unchanged.

## 12. Work package D — Exit status contract

Define and test process exit semantics for automation.

Preferred direction:

- exit 0: command completed according to its requested mode even if some candidates were safely skipped by policy/safety;
- nonzero: fatal invocation/configuration/scope failure or operational failure that prevents trustworthy completion;
- Execute with one or more Cargo/process failures should be nonzero even if other independent units succeeded;
- Preview/Simulate with scope-blocked incomplete ownership should use a nonzero status because the requested cleanup assessment could not be completed;
- read-only scan with bounded nonfatal diagnostics may remain exit 0 if the existing partial-result contract is preserved.

If finer-grained exit codes are introduced, keep the set small and documented. Do not encode every disposition into process status.

JSON output should still be emitted for nonzero operational outcomes whenever a trustworthy report exists.

## 13. Work package E — Non-interactive automation readiness

Once JSON and exit semantics are stable, document supported unattended invocation.

Requirements:

- no prompt is introduced in this milestone;
- `--yes` remains the explicit destructive opt-in;
- no implicit execute mode;
- no environment variable silently enables mutation;
- no broadening from known to full;
- `clean --full --yes` must still complete Full reconciliation first and then run bounded fresh proofs;
- interruption/cancellation semantics remain those of existing cleanup execution;
- external schedulers are expected to enforce their own cadence/load/dependency policy.

Provide examples suitable for cron/systemd timers/other schedulers, but cargo-cleanme itself remains stateless between invocations except for the existing discovery state.

## 14. Failure and compatibility semantics

- unknown future schema versions are a consumer concern; cargo-cleanme emits only its current schema;
- adding optional fields MAY be backward-compatible, but changing enum meaning is not;
- human output compatibility remains separate from JSON schema compatibility;
- `--stats` never alters schema;
- serialization failure after destructive work is a serious operational error and must be made extremely unlikely by constructing serializable typed results before rendering; tests must cover it;
- JSON generation must not panic on non-UTF-8 paths or large byte counts.

## 15. Required tests

### Schema fixtures

Maintain golden or structural fixtures for:

- empty scan;
- scan with eligible groups and diagnostics;
- cleanup Preview;
- cleanup Simulate;
- cleanup Execute with observed decrease;
- M008A policy skips;
- shared/uncertain/unauthorized skips;
- scope-blocked unresolved ownership;
- mixed success/Cargo failure;
- Full reconciliation metadata where reported.

Golden tests should normalize inherently variable fields such as timestamps/version only through explicit fixture construction, not ad-hoc string replacement of production output.

### Determinism

- stable item ordering;
- stable enum spellings;
- same typed report -> byte-identical JSON;
- `--stats` on/off -> byte-identical stdout;
- progress enabled/disabled -> byte-identical JSON stdout.

### Exit status

Integration-test every documented status class.

### Cross-format parity

For fixture operations, assert human and JSON summaries derive from the same typed report and agree on counts/bytes/outcomes.

### Platform/MSRV

- Linux/macOS/Windows hosted CI;
- Rust 1.89 check/test;
- Unix non-UTF-8 path fixture;
- Windows path escaping fixture.

## 16. Verification commands

At minimum:

~~~text
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets --all-features
rustup run 1.89 cargo check --locked --all-targets
rustup run 1.89 cargo test --locked --all-targets
git diff --check
~~~

Add CLI integration invocations that parse emitted JSON with `serde_json` or a small test harness.

Hosted Linux/macOS/Windows CI must pass.

## 17. Documentation updates

Update:

- README format examples;
- documented schema version;
- disposition/reason-code table;
- exit-code semantics;
- unattended invocation guidance;
- explicit statement that cargo-cleanme is not a scheduler/daemon;
- compatibility policy for machine-readable output.

If practical, add a checked-in `docs/` or `plans/` schema reference generated from the stable DTO contract. Do not add a heavyweight schema-generation dependency solely for documentation.

## 18. Acceptance criteria

M008B may close only when:

- scan and cleanup can emit one deterministic versioned JSON document;
- machine consumers never need to parse human `detail` to distinguish policy/safety/action outcomes;
- JSON and human output derive from the same typed operational results;
- `--stats` and progress never contaminate JSON stdout;
- process exit semantics are documented and integration-tested;
- non-interactive `--yes` usage is documented without adding background execution;
- no destructive safety boundary changes;
- hosted Linux/macOS/Windows and Rust 1.89 gates pass.

## 19. Non-goals

- scheduler/daemon/cron driver;
- shell installer;
- network service/API;
- telemetry;
- streaming/NDJSON;
- direct filesystem deletion;
- package/profile-selective Cargo cleanup (M008C);
- policy implementation already owned by M008A.

## 20. Stop conditions

Stop and require architecture review if:

- public JSON would directly serialize unstable internal structs;
- a consumer must parse rendered strings to determine safety;
- JSON mode changes discovery/cleanup behavior;
- non-interactive operation weakens explicit `--yes` or Full-reconciliation gates;
- supporting non-UTF-8 paths would require silently lossy identity semantics;
- an implementation introduces a background scheduler/service.

## 21. Closure evidence

The closure record must include:

- schema-versioned example fixtures;
- stable disposition/reason-code inventory;
- cross-format parity evidence;
- exit-status matrix;
- `--stats`/progress stdout-isolation evidence;
- unattended `--yes` integration fixture with fake Cargo;
- hosted Linux/macOS/Windows CI;
- Rust 1.89 verification.
