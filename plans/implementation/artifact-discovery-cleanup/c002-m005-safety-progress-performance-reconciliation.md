# Corrective C002 — M005 Ownership Safety, Simulation Parity, Progress, and Global-Scan Qualification

Status: closed

Closure: `plans/closure/artifact-discovery-cleanup/c002-status.md`

Repository baseline: 8177d82cb4d6abf313665e169ca2bf1bbbc6e0a7

Corrects:

- M005A plan: plans/implementation/artifact-discovery-cleanup/005a-workspace-output-resolution-fail-fast-progress.md
- M005A closure: plans/closure/artifact-discovery-cleanup/005a-status.md
- M005B plan: plans/implementation/artifact-discovery-cleanup/005b-authorized-redirected-cleanup-and-dryrun.md
- M005B closure: plans/closure/artifact-discovery-cleanup/005b-status.md

Authoritative decision:

- plans/adr/001-workspace-output-ownership-and-cleanup-authorization.md

Primary class: corrective / safety / performance / UX

## 1. Objective

Repair post-M005 defects found by source audit without weakening ADR 001 or rewriting the historical M005A/M005B closure records.

C002 must:

1. restore the ADR 001 destructive boundary so ExternalUnproven output is inventory-only;
2. make --dryrun exercise the same non-mutating eligibility/safety gates as Execute through the final pre-spawn boundary;
3. make the final destructive preflight prove the complete frozen output state, not only workspace root/target/source activity;
4. make the inline multi-line progress surface terminal-correct and make cleanup progress genuinely determinate;
5. remove avoidable Cargo subprocess work for multi-member workspaces and qualify the generic/global scan path;
6. keep debug performance counters available without printing them unconditionally during normal use.

No new destructive capability is introduced by this corrective.

## 2. Findings

### C002-F1 — ExternalUnproven can be promoted to destructive cleanup

Severity: high / safety contract violation.

At the baseline, workspace::build_groups correctly classifies a single-owner output outside its workspace root as OutputOwnershipClass::ExternalUnproven.

However, cleanup::is_authorized rejects only Shared and Uncertain. If every covering path is inside clean ROOT or a configured cleanup.allowed_output_roots entry, an ExternalUnproven group returns authorized.

This contradicts ADR 001:

- a single observed external owner is not exclusivity proof;
- authorization is not ownership proof;
- ExternalUnproven is inventory-only.

The current M005B tests encode the incorrect promotion, so green tests do not establish the ADR invariant.

### C002-F2 — --dryrun skips non-mutating gates that Execute applies

Severity: medium/high / simulation contract defect.

The baseline skips require_cargo_markers when mode == Simulate and does not run Execute's final preflight in Simulate mode.

Therefore --dryrun can report would-clean for a candidate that --yes would skip before spawning Cargo.

The intended simulation contract is: same cargo-cleanme decision path and safety gates, no Cargo clean subprocess.

### C002-F3 — final destructive preflight does not prove the complete output state

Severity: high / race-hardening defect.

revalidate_group checks workspace/member identity, target/build identity, source activity, output activity, and private/shared state.

Immediately before Execute spawns Cargo, final_preflight currently rechecks only:

- workspace root;
- target identity;
- source activity.

It does not re-prove:

- build-dir identity;
- physical output covering roots/type/symlink state;
- output activity;
- ownership class/group shape;
- cleanup authorization;
- Cargo cache markers.

The stronger first revalidation helps, but the last destructive boundary is narrower than the M005B plan/ADR contract.

### C002-F4 — progress composition and cleanup determinacy are not actually qualified

Severity: medium / UX correctness.

The renderer creates one main ProgressBar and five independent row bars, each writing directly to stderr. Tests primarily instantiate the hidden renderer, so they verify bookkeeping but not a coordinated six-line attended-terminal layout.

Use a coordinated multi-progress/single-draw-target composition, preferably indicatif MultiProgress or an equivalent one-owner renderer, so bars cannot independently fight for cursor state.

Separately, cleanup receives only &dyn ProgressObserver; the trait has no determinate-total event and cleanup never calls IndicatifRenderer::set_determinate_total. The closure claim that cleanup progress is determinate is therefore not established by the actual call path.

### C002-F5 — generic scan still performs avoidable Cargo locate calls and lacks global qualification

Severity: medium / performance.

resolve_workspaces invokes cargo locate-project once per discovered manifest. Metadata is cached once per workspace, but after resolving a workspace its returned member manifests are not used to seed a manifest-to-workspace cache.

For an N-member workspace, the current implementation can therefore perform N locate subprocesses even though the first metadata response identifies all N members.

M005A did not wall-clock qualify the no-argument/global scan. Its closure explicitly records only bounded-root timing. This leaves the generic-scan/fail-fast performance requirement under-qualified.

### C002-F6 — scan performance counters are unconditional normal stderr output

Severity: low / UX/polish.

Normal scan currently always prints the full debug scan counters line.

These counters are useful for qualification and --dryrun debugging, but they should not be unconditional normal UI.

## 3. Why prior verification missed these defects

- F1: tests validated path authorization behavior without rechecking ADR 001's authority ordering. A test for external authorization effectively codified the defect.
- F2: simulation tests asserted no clean subprocess and file non-mutation, but did not compare the complete pre-spawn gate/disposition set against Execute on identical state.
- F3: race tests covered target changes but did not mutate build output/group state between initial revalidation and final preflight.
- F4: progress tests used hidden/in-memory state and asserted top-five bookkeeping; they did not render a real coordinated multi-line terminal. Cleanup determinacy was inferred from design text rather than observed through the trait call path.
- F5: performance qualification used a single-workspace explicit root, so per-manifest Cargo process amplification and global traversal behavior were invisible.
- F6: debug counters were useful during implementation and were never moved behind an explicit debug/statistics surface before closure.

## 4. Invariants

- ADR 001 remains authoritative and is not weakened.
- PrivateBounded is the only ownership class C002 may destructively clean.
- ExternalUnproven, Shared, and Uncertain remain inventory-only.
- cleanup.allowed_output_roots is authorization only; it MUST NOT manufacture ownership proof.
- No direct recursive deletion of Cargo output is added.
- --yes remains the only mutating mode.
- --dryrun invokes no Cargo clean command, including Cargo dry-run.
- On unchanged filesystem/config state, --dryrun and Execute MUST reach the same pre-spawn cleanable/skipped disposition set.
- Execute revalidates the complete output/authorization proof immediately before spawning Cargo.
- Progress rendering remains stderr-only, transient, main-screen, non-raw-mode, and hidden on non-TTY/dumb terminals.
- Final stdout remains deterministic.
- Generic scan never performs a pre-scan merely to manufacture a percentage.
- Cargo configuration remains Cargo-authoritative; performance work MUST NOT add a manual Cargo config parser.
- Rust 1.89 remains the compile-time MSRV.

## 5. In scope

- ownership-class authorization repair;
- simulation gate parity;
- consolidated final cleanup proof/preflight;
- progress renderer composition;
- progress observer total/done semantics;
- multi-member locate-call elimination after authoritative metadata;
- global/generic scan performance qualification and evidence-driven optimization;
- explicit stats/debug output flag;
- README/planning reconciliation;
- focused regression and hosted qualification.

## 6. Out of scope

- a new mechanism to assert external-cache exclusivity;
- selective cargo clean -p;
- shared-cache garbage collection;
- parsing Cargo-private build-cache layout;
- persistent workspace/output index;
- daemon/watcher;
- changing the recency policy;
- machine-wide destructive cleanup;
- changing Cargo's config-resolution ownership from Cargo to cargo-cleanme.

If external exclusive-cache cleanup is desired later, write a separate ADR/milestone defining how the user can make an explicit ownership assertion. Do not reinterpret allowed_output_roots as that assertion inside C002.

## 7. Required production changes

### 7.1 Restore ownership-class safety at the cleanup boundary

Refactor is_authorized, or replace it with a clearer proof function, so:

- PrivateBounded may proceed to path authorization/revalidation;
- ExternalUnproven always returns non-cleanable;
- Shared always returns non-cleanable;
- Uncertain always returns non-cleanable.

An allowed output root may authorize location only after ownership is already PrivateBounded. It cannot convert ExternalUnproven into a private group.

Because current PrivateBounded output is normally workspace-contained, allowed_output_roots may become operationally redundant under ADR 001. Preserve schema compatibility, but document that the field does not establish exclusivity and cannot currently promote an external-unproven group.

Normal skip detail for this case should state both facts, for example: external output ownership is unproven; configured authorization does not establish exclusivity.

Update README claims that currently imply an external redirected cache becomes cleanable merely by listing its root.

### 7.2 Make simulation use the full non-mutating pre-spawn proof

Extract common pre-spawn qualification so Preview/Simulate/Execute cannot silently drift.

At minimum, Simulate MUST run every read-only gate that Execute runs before the actual Cargo clean spawn:

- ownership class;
- path authorization;
- fresh Cargo workspace/output resolution;
- member/source activity;
- target/build physical identity;
- output activity;
- group/ownership stability;
- marker validation;
- frozen environment capability validation;
- final preflight.

Only after all gates succeed may Simulate emit Simulated / would-clean.

Simulation MUST NOT call a runner with Cargo subcommand clean.

On a stable fixture, Execute may expose a helper/dry decision result before spawn so tests can compare its candidate disposition with Simulate without mutating the fixture.

### 7.3 Replace split revalidation with a single final cleanup proof

Prefer a typed proof/result such as ValidatedCleanupContext or ExecutionProof generated by the final preflight and immediately consumed to build the Cargo command.

The final proof MUST bind:

- canonical workspace root + root manifest;
- canonical member set sufficient for source-activity safety;
- canonical target path;
- canonical build path/capability;
- physical covering roots;
- output ownership class;
- authorization decision;
- cache-marker validity;
- fresh source activity;
- fresh output activity;
- frozen environment/arguments.

Immediately before Execute spawn, re-resolve and compare target AND build, rebuild physical grouping, and reject any change in group shape/classification.

The final preflight must inspect every covering root for existence, directory type, and no-symlink state.

Marker validation must be part of the same non-mutating proof used by Simulate.

Do not claim this eliminates all TOCTOU races; minimize the window and fail closed.

### 7.4 Coordinate the inline progress surface

Refactor IndicatifRenderer so the main bar and up-to-five candidate rows share one coordinated terminal renderer/draw target.

Preferred implementation: one indicatif MultiProgress owning the main bar and row bars.

Equivalent implementation is acceptable only if it provides one cursor/draw owner and can be tested as a composed multi-line frame.

Requirements remain:

- no alternate screen;
- no raw mode;
- stderr only;
- at most five candidate rows;
- largest/reportable ordering as currently intended;
- at most 10 Hz refresh;
- clear before final stdout;
- hidden/no control sequences for non-TTY, TERM=dumb, or --no-progress.

### 7.5 Put determinate totals into the observer contract

Do not require callers to know they have an IndicatifRenderer.

Extend ProgressObserver with a semantic total/progress event, for example units_total(phase, total) and unit_completed(phase), or an equivalent typed update.

Both scan analysis and cleanup MUST set a determinate total once their physical group/candidate count is known.

The renderer must reset/re-scope totals on phase changes so scan-analysis counts cannot leak into cleanup counts.

Tests must drive the real cleanup call path through the trait and assert determinate total/done behavior.

### 7.6 Seed workspace-member cache from Cargo metadata

After the first successful metadata resolution for a workspace, canonicalize its returned member manifest paths and register each as mapping to that workspace in the current invocation.

Before invoking cargo locate-project for a discovered manifest, check that authoritative member cache.

Required invariant:

- after any one member/root resolves a workspace, later discovered manifests known from that metadata MUST require neither another locate nor another metadata process.

For a fixture with N members discovered in arbitrary order, expected successful Cargo calls are one locate and one metadata, unless the first resolution itself fails.

Do not infer workspace membership from path ancestry or hand-parsed TOML.

### 7.7 Qualify and, only if justified, optimize global resolution

Add instrumentation separating:

- discovery traversal time;
- Cargo locate time/count;
- metadata time/count;
- activity-analysis time;
- output-sizing time;
- render overhead;
- total elapsed.

First apply the member-cache optimization and measure.

If Cargo subprocess time remains a material generic-scan bottleneck, an OPTIONAL second step may evaluate a small bounded Cargo resolver concurrency limit. Do not introduce concurrent Cargo processes without evidence; preserve deterministic result ordering and bound concurrency explicitly.

Do not weaken --offline --locked --no-deps merely for speed.

### 7.8 Move debug counters behind an explicit flag

Add a global flag such as --stats.

Default attended/non-attended scan:

- final report on stdout;
- concise diagnostics summary only when diagnostics exist;
- no unconditional scan counters line.

With --stats:

- print the existing semantic counters;
- include the new phase/process timing counters;
- work with scan, Preview, Simulate, and Execute where applicable;
- remain stderr/debug output, never part of deterministic report stdout.

--no-progress --stats is the canonical benchmark/debug combination.

## 8. Ordered work packages

### A — Ownership regression repair

Fix F1 first. Add negative tests before touching progress/performance.

### B — Common cleanup qualification/proof

Unify marker, ownership, output, authorization, activity, target/build, and frozen-env checks. Make Simulate and Execute consume the same proof through the spawn boundary.

### C — Progress rendering correctness

Introduce coordinated multi-line rendering and semantic total events. Prove cleanup determinate progress through the actual cleanup entry point.

### D — Resolver fail-fast optimization

Seed the authoritative member cache and remove redundant locate calls.

### E — Global performance qualification

Add timings/stats surface, run representative generic scans, and make additional optimization only when evidence supports it.

### F — Documentation/planning closure

Correct README claims, reconcile roadmap/registry, and create C002 closure.

## 9. Required tests

### Ownership safety

- ExternalUnproven inside clean ROOT => skipped.
- ExternalUnproven inside allowed_output_roots => still skipped.
- ExternalUnproven inside both => skipped.
- Shared inside any authorization => skipped.
- Uncertain inside any authorization => skipped.
- PrivateBounded conventional/local redirected-inside-workspace fixture remains preview/simulate/execute eligible.
- No test may describe an allowed root as ownership proof.

### Simulation parity

On identical immutable fixture state:

- missing marker => Simulate and Execute pre-spawn disposition both skip;
- invalid marker => both skip;
- changed target => both skip;
- changed build-dir => both skip;
- recent source => both skip;
- recent output => both skip;
- changed ownership/group shape => both skip;
- unsupported/unknown build capability => both skip;
- valid private fixture => both reach cleanable pre-spawn state;
- Simulate records zero Cargo clean calls.

### Final preflight/races

Provide a staged fake runner/hook capable of changing state between first revalidation and final preflight.

Cover:

- target replacement;
- build-dir replacement;
- build/target becomes symlink;
- covering root disappears;
- output becomes recent;
- member source becomes recent;
- marker disappears/changes;
- physical group changes classification.

Every case skips before Cargo clean.

### Progress rendering

- main + five rows are owned by one coordinated renderer;
- a test/in-memory terminal frame contains at most six progress lines;
- repeated refreshes do not append an unbounded scrolling log;
- clear removes transient frame before report output;
- hidden renderer emits no control output;
- phase total resets correctly;
- scan analysis total becomes determinate;
- cleanup Preview/Simulate/Execute totals become determinate through the actual observer path;
- refresh count remains rate-limited.

If indicatif exposes a suitable in-memory terminal abstraction, use it. Otherwise inject a small rendering sink/adapter rather than relying only on hidden-state tests.

### Resolver/performance

- N manifests from one real/fake Cargo workspace => exactly one locate + one metadata after first authoritative resolution.
- Discovery order with a member before the root still => one locate + one metadata.
- Two independent workspaces => two locate + two metadata.
- failed first locate does not poison unrelated workspaces.
- ignored/pruned manifests => zero Cargo calls remains true.
- results/counters remain deterministic.

### CLI/stats

- --stats parses in direct and cargo cleanme forms.
- default scan emits no debug counter line.
- --stats emits counters/timings to stderr.
- stdout report is byte-equivalent with and without --stats.
- --no-progress --stats produces no terminal control sequences.

## 10. Performance qualification

### Synthetic/global-like fixture

Create a broad fixture containing:

- many irrelevant directories;
- ignored/pruned Cargo-like trees;
- at least one large multi-member workspace;
- many empty/no-output workspaces;
- active workspaces;
- inactive reportable workspaces.

Record baseline vs C002:

- directories visited/pruned;
- manifests;
- unique workspaces;
- locate/metadata calls;
- phase timings;
- groups deeply measured;
- elapsed time.

The multi-member locate-call reduction is a hard acceptance criterion, not merely a timing observation.

### Real generic scan

Before C002 closure, run a release no-argument scan on at least one representative developer host using --no-progress --stats.

It is read-only.

Record:

- total elapsed;
- manifests/workspaces;
- locate/metadata counts;
- Cargo-resolution fraction of wall time;
- pruned paths;
- active/empty/deep-sized counts;
- diagnostics.

Where practical, run the same tree at the pre-C002 baseline and corrected head for comparison.

If the environment cannot safely or meaningfully run the generic scan, C002 may be only conditionally closed; do not claim the user's global-performance requirement is qualified solely from a one-workspace explicit-root fixture.

No universal wall-clock threshold is required because host/storage topology varies. A material regression must be explained.

## 11. Compatibility/configuration effects

- Rust 1.89 remains unchanged.
- cleanup.allowed_output_roots remains parse-compatible but is clarified as location authorization only and does not make ExternalUnproven cleanable.
- --stats is additive.
- --dryrun, --dry-run, and --yes spellings/mode distinction remain unchanged.
- Shared/external-unproven inventory reporting remains available.
- Some external redirected outputs that M005B incorrectly allowed will now be skipped. This is an intentional safety correction, not a backward-compatibility regression.
- No direct-deletion API is added.

## 12. Verification commands

cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets --all-features
cargo check --locked
cargo +1.89 check --locked --all-targets
cargo +1.89 test --locked --all-targets
git diff --check

Hosted stable Linux/macOS/Windows plus Rust 1.89 must pass.

Also perform static review:

- no production remove_dir_all / arbitrary recursive deletion;
- ExternalUnproven cannot reach Cargo clean;
- Simulate cannot reach Cargo clean;
- all Cargo clean spawns consume the final validated proof.

## 13. Documentation/planning updates

Update:

- README ownership/authorization semantics;
- README normal-vs---stats output;
- README progress behavior if renderer mechanics change;
- subsystem roadmap;
- registry;
- C002 closure.

Do not edit M005A/M005B closure records to imply the findings never existed. C002 is the historical correction.

ADR 001 should not need modification because C002 restores implementation compliance with it. If implementation discovers that ADR 001 itself must change, stop and write a new/superseding ADR before changing destructive semantics.

## 14. Acceptance criteria

C002 closes only when:

- ExternalUnproven cannot be cleaned under any existing authorization-root configuration;
- only PrivateBounded can reach Cargo cleanup;
- Simulate and Execute share all non-mutating pre-spawn gates and produce matching candidate dispositions on unchanged fixtures;
- Simulate performs zero Cargo clean invocations;
- final Execute preflight binds target + build + group + authorization + activity + marker state immediately before spawn;
- coordinated multi-line progress is terminal-qualified;
- cleanup progress is actually determinate through the observer interface;
- an N-member workspace produces one locate + one metadata after first authoritative resolution;
- default scan no longer emits debug counters;
- --stats exposes counters/timings without changing stdout;
- representative generic-scan evidence is recorded;
- stable Linux/macOS/Windows and Rust 1.89 are green;
- no direct recursive output deletion exists.

## 15. Stop conditions

Stop and report rather than weakening the contract if:

- external redirected cleanup requires treating authorization as ownership proof;
- Cargo cannot expose enough stable information to revalidate distinct build-dir immediately before cleanup;
- simulation parity would require invoking Cargo clean --dry-run;
- coordinated progress requires raw mode/alternate-screen behavior;
- member-cache optimization requires guessing workspace membership instead of using Cargo metadata;
- global performance can only be improved by dropping conservative Cargo resolution or activity checks;
- Rust 1.89 cannot compile the chosen renderer changes.

## 16. Closure evidence

Create:

plans/closure/artifact-discovery-cleanup/c002-status.md

The closure record MUST include:

- implementation commits/PR;
- C002-F1 through F6 requirement-to-evidence matrix;
- ownership authorization matrix;
- proof that ExternalUnproven never reaches Cargo clean;
- Simulate-vs-Execute pre-spawn parity matrix;
- zero-clean subprocess trace for Simulate;
- staged race/preflight tests including build-dir/output activity/marker changes;
- attended-terminal or in-memory composed progress evidence;
- cleanup determinate-total evidence through the real observer call path;
- N-member locate/metadata call counts before/after;
- synthetic/global-like performance counters/timings;
- at least one representative real generic scan, or explicit conditional-closure reason;
- default-vs---stats stdout/stderr evidence;
- full hosted CI/MSRV evidence;
- static no-direct-deletion review;
- unresolved findings and final disposition.
