# C045 — The `clean` alias's surface is declared twice, and its compatibility guarantee is guarded in one direction only

Status: **closing** — implemented and ladder-green. **Not published**: no tag,
no release, no crates.io publication; publication is owner-gated. The
behavioural contract this corrective preserves was settled before the work
began (ADR 008 §4b is accepted; M019's resolved intent is frozen), and no owner
decision was outstanding. Closure record:
`plans/closure/artifact-discovery-cleanup/c045-status.md`.
Date: 2026-10-22.
Repository baseline: `main` at `22bbfd2` (0.2.10 published; latest published tag
`v0.2.10`).
Source roadmap: `plans/subsystems/artifact-discovery-cleanup-roadmap.md`.
Corrects: **M019** — `plans/implementation/artifact-discovery-cleanup/m019-canonical-full-cleanup-spelling-and-root-front-door.md`;
closure record `plans/closure/artifact-discovery-cleanup/m019-status.md`
(status: **closing**, unpublished).
Related: ADR 008 §4b (the `clean` alias survives as a hidden compatibility
alias) and ADR 008 §§1–4; ADR 003 §4 (canonical invocation, amended twice on
2026-10-21); ADR 006 §5 (the `scan` selectors); ADR 009 / **C040** (the
working directory joins the maintenance scope); ADR 007 §3 (a discarded flag is
the worst failure mode this tool has); **M012A** (the origin of the hidden
legacy aliases); **M014** (the CWD-anchored default and `-i` selection);
**M017**; **C036** (CLI-surface truthfulness and documentation drift).

## Objective

Make the cleanup invocation surface have **exactly one declaration**, and make
the `clean` alias's compatibility guarantee **mechanically enforced** rather
than hand-listed, so that adding a root-level cleanup flag can never silently
break the published `cargo cleanme clean …` spelling.

Concretely:

- the twelve cleanup flags the root and the alias share are declared **once**;
- the scope rule — which scope a given argv selects — is implemented **once**;
- a guard asserts the **converse** of today's: not only that the alias carries
  the flags it carries, but that it accepts **every** cleanup flag the root
  declares;
- the prose that describes the surface says what the surface actually is.

## Non-goals

- **Not** a change to any resolved intent. The scope, the selection, the mode,
  the policy evaluation, the JSON envelope, the reason-code vocabulary and the
  exit codes are untouched. Every published spelling resolves to exactly what
  it resolves to today.
- **Not** a removal, deprecation, or narrowing of any published spelling.
  `clean ROOT`, `clean --known`, `clean --full`, `clean --dry-run`,
  `clean --dryrun`, `clean --yes`, `clean --cargo-preview` and every root-level
  flag continue to work. `clean` stays **hidden**: absent from
  `cargo cleanme --help`.
- **Not** a change to the bare invocation's semantics. The working directory
  joining the maintenance scope is a separate, shipped, canonical feature
  (ADR 009 / C040) and is not re-derived through the alias.
- **Not** a change to the cleanup engine, the discovery pipeline, ownership
  proof, or the scan surface.
- **Not** a new ADR. No durable decision is being made here; the decision
  (ADR 008 §4b) is already accepted and this corrective only implements it
  faithfully.
- **Not** a rename or a deprecation notice for `--yes`. `--yes` is deliberately
  unbound (`src/cli.rs:424-427`) because Execute is now the default; that
  decision stands and is recorded in the surface documentation.

## The premise this corrective corrects

The hypothesis that motivated this line of work was that the `clean`
subcommand's duplication existed so that the base scan could be run in a target
directory simply as `cargo cleanme`. **That need is already met, and it is met
without the alias.**

A bare `cargo cleanme` in a target directory *is* the base scan in that
directory. The working directory joins the maintenance scope as an additional
root (ADR 009, accepted 2026-10-20; **C040**, shipped in v0.2.10), and the
operator's roots carry `RootProvenance::Operator` so the maintainability rule
does not refuse a root the operator is standing in. The scope only ever grows
from the known roots, so a target shared with a project outside the scope still
stays `shared`. It is documented at `docs/USAGE.md:25`, `:113-120`, `:226` and
`:378`.

Verified on 2026-10-22 against the built binary, in a fixture directory:

~~~text
$ cd /tmp/cli-equivalence/proj && cargo-cleanme --dry-run --format json
{"schema_version":1,…,"scope":"routine","mode":"simulate","result":{…
 "selected_roots":["/home/sugarwookie/m013-fixture","/home/sugarwookie/projects",
 "/tmp/cli-equivalence/proj"],…}}
~~~

The fixture directory is in `selected_roots`. The same run as
`cargo cleanme clean --dry-run --format json` is **byte-identical**, as is
`cargo cleanme <ROOT>` against `cargo cleanme clean <ROOT>`.

So the alias's remaining purpose is narrow and specific: **published-spelling
compatibility.** `clean` and `clean --known` have been published since v0.2.0
(`CHANGELOG.md`'s v0.2.0 mode table lists `cargo cleanme clean R --dry-run`,
`… --cargo-preview`, `… --dryrun` and `… --yes`), and ADR 008 §4b settled that
they are retained, hidden, and never deleted.

That distinction is the thing this corrective records. It is not currently
written down anywhere as a single sentence, and the consequence of leaving it
unwritten is concrete: an implementer who believes the alias is what makes
"run in the target directory" work will preserve it for the wrong reason, and
may one day "simplify" by making the alias the canonical path — undoing ADR
008 §4b and deprecating the bare invocation that the ADR 009 / C040 work
actually made canonical.

## Current implementation evidence

> **These citations are a baseline, not a current map.** They describe the
> tree this plan was written against, before the corrective changed it. The
> plan is a historical record of a decision and of the evidence it was decided
> on; the *current* map is `architecture/02-cli.md`, and the current evidence
> is `plans/closure/artifact-discovery-cleanup/c045-status.md`. This is the
> convention `architecture/overview.md` §10 item 64 records for historical
> records.


### D1 — the compatibility guarantee is guarded in one direction only

The only guard on the alias's surface is
`compatibility_aliases_are_hidden_but_accepted`
(`src/cli.rs:1854-1896`). It enumerates the alias's ids — twelve hidden
(`src/cli.rs:1870-1882`) and two visible (`src/cli.rs:1889-1895`) — and asserts
that the alias **carries** them and that their hide states are right.

It asserts nothing about the **converse**: that the alias accepts every
cleanup flag the root declares.

That converse does not even hold *structurally* today. `interactive` is
declared only on the root (`src/cli.rs:87-88`) and reaches the alias solely
because the flag is declared `global = true` — not because the alias declares
it. The guard's list correctly omits `interactive`, which means the list is
already a hand-maintained approximation of the real surface, not a derivation
of it.

The failure mode is specific and reachable. M019's design makes the root the
canonical home for every future policy override. An override added to the root
and not mirrored onto the alias yields `cargo cleanme clean --new-override` →
clap *"unexpected argument"* → exit `2`. A spelling published since v0.2.0
stops working. The guard does not mention the new flag, so the suite stays
green.

M019's own acceptance criteria say: *"No published spelling is removed; `clean`
still accepts everything it accepts today."* That criterion is satisfied today,
but it is discharged by reading a diff — **no test enumerates "everything it
accepts today"**, so the guarantee cannot be broken loudly. It can only be
broken quietly.

Recorded as `architecture/overview.md` §10 finding **68**.

### D2 — the cleanup surface is declared twice, and the scope rule is implemented twice

**The flags.** The root declares twelve cleanup flags — `dry_run`
(`src/cli.rs:71`), `dryrun_legacy` (`:74`), `interactive` (`:88`), `full`
(`:94`), `root` (`:102`), `min_reclaimable_bytes` (`:105`), `older_than`
(`:108`), `include` (`:111`), `exclude` (`:114`), `profile` (`:117`),
`package` (`:120`), `cargo_preview` (`:124`). The alias declares the same
surface again — `root` (`:176`), `known` (`:179`), `full` (`:182`),
`min_reclaimable_bytes` (`:185`), `older_than` (`:188`), `include` (`:191`),
`exclude` (`:194`), `profile` (`:197`), `package` (`:200`), `dry_run` (`:204`),
`cargo_preview` (`:208`), `yes_legacy` (`:212`), `dryrun_legacy` (`:215`) —
eleven of them `hide = true`, and two (`dry_run`, `known`) advertised.

**The scope rule.** `root_cleanup_request` (`src/cli.rs:469-487`) derives the
scope from three sources into a three-arm match. `merged_cleanup_request`
(`src/cli.rs:497-567`) derives it **again**, from the merged root+alias view,
with six conflict checks and its own three-arm match. The two are not shared,
and neither calls the other.

**The merge layer.** `merge_overrides` (`src/cli.rs:721-745`), `merge_one`
(`:748-755`), `merge_many` (`:758-765`), `merged_clean_mode` (`:655-665`),
`selection_for` (`:642-647`) and the `CleanAlias` struct (`:771-779`) are a
bespoke layer whose whole job is to reconcile two copies of one declaration.

Four places to touch for one new flag: the root declaration, the alias
declaration, the merge layer, and the guard's hand-list. **D1 is the
consequence of D2.**

Recorded as `architecture/overview.md` §10 finding **69**.

### D3 — the type-level prose denies the shipped surface

`CleanupOverrides`'s own doc comment (`src/cli.rs:278-282`) still says:

> These live only on the `clean` subcommand by design (M012A work package D):
> bare maintenance intentionally uses the configured/default policy rather than
> cloning the whole advanced option surface onto the root command.

M019 put them on the root — that is the whole point of ADR 008 §4b — and the
type's fields are now read from the root's flags
(`src/main.rs:354-380`). The comment is not a doc nit: it states the *reason* a
reviewer would use to reject the change that was actually made, and it is the
first thing a reader of the consolidated surface will meet.

This is `architecture/overview.md` §10 finding **65**, which was recorded as
*"Open — not yet planned"* and is the right home for a doc fix that belongs to
whoever owns `src/`. This corrective is that owner.

A wider sweep is adjacent and **not** in scope here: 48 source comments across
`src/cleanup.rs` (13), `src/cli.rs` (22), `src/main.rs` (8) and
`src/policy.rs` (5) still spell pre-M019 invocations (`clean ROOT`,
`clean --full`, `clean --known`) where the root spelling is now canonical.
Those are stale but not false — the alias still works — so they are recorded as
a related open item rather than swept here.

### D4 — what was verified before this plan was written

- The two spellings are **byte-identical** in the JSON envelope, for both the
  bare/`clean` pair and the `ROOT`/`clean ROOT` pair (2026-10-22, fixture at
  `/tmp/cli-equivalence/proj`). There is **no behavioural defect** in the alias
  layer today.
- M019's fifteen named tests all exist in `src/cli.rs` and pass. The suite
  covers the equivalence it names.
- `cargo cleanme --known` is a usage error (exit `2`) — the root has no
  `--known`; `cargo cleanme clean --known` resolves to the maintenance scope.
- `cargo cleanme --yes` is a usage error; `cargo cleanme clean --yes` is
  accepted and deliberately unbound (`src/cli.rs:424-427`).
- `cargo cleanme clean -i` is accepted, because `interactive` is `global = true`
  (`src/cli.rs:87`) — the case that shows the converse guard is not merely
  missing but *already untrue* of the declarations.

## Why previous verification missed it

This is the field that decides whether the corrective is ready. Four reasons,
each verified:

1. **Every equivalence test names a hand-picked pair.**
   `the_root_and_the_alias_resolve_to_the_same_intent`
   (`src/cli.rs:1105-1127`) asserts equality for four *specific* pairs: bare
   vs `clean`, `/tmp/p` vs `clean /tmp/p`, `--full` vs `clean --full`, and
   `--dry-run` vs `clean --dry-run`. A hand-written pair list is green for the
   pairs it names and silent about every pair it does not. The space of
   spellings is a **cross product** — scope selectors × overrides × levels —
   and it is open-ended, so no such enumeration can be complete by
   construction.

2. **The alias-flag guard points the wrong way.** It answers "does the alias
   carry flag F?", which is the question that matters when a flag is being
   *removed*. Compatibility needs the opposite question: "does the alias accept
   every flag the root offers?" A guard cannot detect "the root gained flag G
   and the alias did not" when it is written as a list of the alias's own ids.

3. **The compatibility criterion was discharged by reading, not by checking.**
   M019's acceptance criteria contain the sentence *"No published spelling is
   removed; `clean` still accepts everything it accepts today."* Nothing
   enumerates "everything it accepts today", so the sentence is a review
   instruction. This is `AGENTS.md` §5's first failure mode in its purest form:
   the test proves only that its own premises hold, and the premise *is* the
   hand-written list that would need updating when the surface changes.

4. **The duplication is invisible at runtime.** Both spellings resolve to the
   same `Invocation`, so no behavioural test can observe it. The duplication
   is only visible in the *source*, and nothing in the plan or the suite was
   looking there — M019's subject was the spelling, and the spelling was
   right.

## Invariants

These must hold before, during and after the change.

- **I1.** The resolved intent of every published spelling is unchanged: the
  same `CleanupScope`, the same `CleanupSelection`, the same `CleanMode`, the
  same `CleanupOverrides`.
- **I2.** `clean` remains **hidden** — absent from `cargo cleanme --help` —
  and remains **accepted**.
- **I3.** No published spelling is removed, narrowed, or made to fail: every
  flag the alias accepts today it still accepts, and every flag the root
  accepts today the alias also accepts.
- **I4.** The one-override-at-one-level rule is unchanged: the same override
  at both levels is still a usage error (exit `2`), and a scope selector at
  both levels is still a usage error naming the flags that actually collide.
- **I5.** The alias's hide states are unchanged: `dry_run` and `known` stay
  advertised on `clean`; the other twelve stay hidden.
- **I6.** The bare invocation's CWD-anchored behaviour is unchanged, and is
  **not** re-implemented through the alias.
- **I7.** No change to the JSON envelope, the reason-code vocabulary, the exit
  codes, the cleanup engine, the discovery pipeline, ownership proof, or the
  scope resolution.
- **I8.** The alias carries no intent the root does not. `--yes` remains
  deliberately unbound, and the root continues to reject `--yes`.

## Production changes

Two implementation shapes are available for the declaration, and §4 of the
planning process preserves the choice: *"Plans may name likely modules but
SHOULD preserve implementation freedom where several mechanics satisfy the
contract."*

- **Shape A — a shared builder.** One helper applies the same `#[arg]`
  attributes to both the root struct and the alias variant, so the two cannot
  diverge by omission.
- **Shape B — one flattened struct.** A single `CleanupFlags` struct is
  `#[command(flatten)]`ed into both `Cli` and `Command::Clean`. This is the
  idiomatic clap answer and removes the duplication outright rather than
  making two copies that agree.

Either shape is acceptable. What is **not** acceptable is leaving two
independent hand-maintained lists.

The consolidation must preserve, exactly:

- the hide states (I5) — `dry_run` and `known` advertised on the alias, the
  rest hidden;
- the alias's own `root` positional and `known` flag, which the root does not
  have;
- the root's `yes_legacy` absence — `--yes` stays `clean`-only (I8);
- the six conflict checks and their exact messages;
- `selection_for`'s three arms and `merged_clean_mode`'s precedence
  (simulation over Cargo preview, at either level).

The scope rule collapses to **one** derivation that takes the merged
root+alias view. The merge layer (`merge_overrides`, `merge_one`,
`merge_many`) may be retained, simplified, or replaced — it is correct today
and its behaviour is frozen; only its duplication with the declaration is in
scope.

## Work packages

- **WP-1 — Freeze the current surface before touching it.** Add a
  characterization test that enumerates the **whole** published spelling set
  from a table — scope selectors × overrides × levels — and asserts each
  root/alias pair resolves to the same `Invocation`. It must be written
  **before** any production change, and it must pass against the current
  code. This is the regression evidence for I1–I3.
- **WP-2 — One declaration.** Consolidate the cleanup flag surface into one
  declaration (shape A or B). I5 and I8 hold; no rendered help text changes.
- **WP-3 — One scope rule.** Collapse `root_cleanup_request` and
  `merged_cleanup_request` onto a single derivation. The six conflict checks
  and their messages are byte-identical afterwards.
- **WP-4 — The converse guard.** Add a test that enumerates the **root's**
  cleanup flag ids from the clap command model and asserts the alias accepts
  each one — either by carrying it or by inheriting it as `global`. This is
  the guard that was missing (D1). It is a *structural* guard: it reads the
  clap model, so a new root flag cannot be added without the guard noticing.
- **WP-5 — Correct the prose.** Fix `CleanupOverrides`'s doc comment (D3,
  finding 65) and rewrite the alias's rationale comment to state the alias's
  actual purpose — published-spelling compatibility — and to point at ADR 009
  / C040 for the bare invocation's CWD-anchored behaviour, so the two are not
  confused again.
- **WP-6 — Regenerate the artifacts** if the rendered help changed at all,
  and confirm the drift gate is green.

## Failure, cancellation and contention semantics

Not applicable in the usual sense: this corrective changes no I/O, no
process-spawning, no traversal, no state and no concurrency. The only
behavioural surface it touches is argv parsing and the two usage-error
messages, both of which are frozen. The one hazard that *is* in scope is
**silent narrowing** — a consolidation that drops a spelling would fail
closed for the operator (exit `2`, "unexpected argument"), which is safe but
is still a broken published spelling, and is exactly what WP-1 and WP-4 exist
to catch.

## Compatibility and configuration effects

- **No configuration effect.** `config.toml` is untouched; no config key is
  added, removed, or reinterpreted.
- **No published spelling is removed or narrowed** (I3). The alias remains
  accepted and hidden.
- The rendered `cargo cleanme --help` and `cargo cleanme clean --help` must be
  **byte-identical** before and after, except for wording WP-5 explicitly
  authorises in a *source comment* (not in user-facing help).
- The generated artifacts (`completions/`, `man/`) must be unchanged; the
  alias's man page continues to ship, per `AGENTS.md` §4.

> **Amendment (2026-10-22, after implementation).** The completions **cannot**
> be byte-identical, and the reason is structural rather than accidental: a
> completion script lists a command's options in the order the command
> *declares* them, and the root and the alias declared the same flags in
> **two different orders** before this corrective. One shared declaration
> has one order, so one of the two orderings had to move. The flag *sets* are
> unchanged — the same flags are offered, with the same values and the same
> help — and `man/` **is** byte-identical, because a man page renders the
> *help text*, which is unchanged. What moved is the order of the option
> lists in the five completion files, and the order of `--interactive` in the
> root's list.
>
> **A second, non-ordering change, found on review and recorded here rather
> than papered over.** In the zsh completion only, the `clean` alias's
> *rendered* conflict hints are narrower on three options: `--dry-run` reads
> `(--dryrun)` where it read `(--cargo-preview --yes --dryrun)`, `--dryrun`
> reads `(--dry-run)` where it read `(--dry-run --cargo-preview --yes)`, and
> `--full` reads `()` where it read `(--known)`. The **effective** conflict
> relation is unchanged — clap's is symmetric, so a pair is refused whichever
> side declares it, and every one of those pairs is still refused with the
> same message and the same exit `2`. What moved is which *side* carries the
> declaration, which is inherent to the shared shape: the shared declaration
> cannot declare `conflicts_with = "known"`, because the root has no `known`
> argument and clap rejects the unknown id. The other four completion files
> carry no conflict hints and changed in order only.
>
> This is recorded rather than papered over: the criterion as written was not
> satisfiable together with the corrective's primary goal, and the plan is
> amended rather than the criterion quietly dropped. The alternative —
> keeping two declarations so that two orderings survive — is the defect
> this corrective exists to remove.

## Required tests

Each test names the input that makes it fail.

1. **The whole published spelling set agrees.** A table of
   (root argv, alias argv) pairs covering the cross product of scope
   selectors (`none`, `ROOT`, `--full`) × overrides (`none`, and each of
   `--min-reclaimable-bytes`, `--older-than`, `--include`, `--exclude`,
   `--profile`, `--package`) × the two levels. Each pair must resolve to the
   same `Invocation`. *Fails if:* any pair diverges — which is the shape of a
   consolidation that drops or mis-merges one override.
2. **The converse compatibility guard.** The root's cleanup flag ids are read
   from the clap command model; the alias must accept every one of them, by
   carrying it or by inheriting it as `global`. *Fails if:* a root cleanup
   flag exists that `cargo cleanme clean <flag>` would reject. **This is the
   test that did not exist.**
3. **The alias is still hidden and still accepted.** `clean` is absent from
   `cargo cleanme --help`; `cargo cleanme clean --known`, `clean --full` and
   `clean ROOT` all still parse and resolve. *Fails if:* the alias is dropped,
   or the hidden subcommand reappears in the help text.
4. **The hide states are unchanged.** `dry_run` and `known` are advertised on
   the alias; the other twelve are hidden. *Fails if:* a consolidation
   flattens the alias and loses or gains a hide state.
5. **The duplicate-override rule survives.** `cargo cleanme --older-than 3600
   clean --older-than 60` exits `2` with a usage message and cleans nothing.
   *Fails if:* one of the two values is silently preferred — the discarded-flag
   failure mode ADR 008 §3 exists to prevent.
6. **The scope-conflict rules survive.** The four scope-conflict shapes
   (`--full` with a ROOT at either level; `clean --known` with a root-level
   `--full` or a root-level ROOT) each exit `2` and name the flags that
   actually collide. *Fails if:* a conflict check is lost in the collapse, or
   the message names a flag the operator never typed.
7. **The bare invocation is untouched.** In a fixture directory, `cargo
   cleanme` resolves to the maintenance scope with the working directory
   joining it; `cargo cleanme clean` resolves to the same `Invocation`. *Fails
   if:* the consolidation changes what the bare invocation selects — which is
   the behaviour ADR 009 / C040 shipped.
8. **`--yes` stays `clean`-only and stays unbound.** `cargo cleanme --yes`
   exits `2`; `cargo cleanme clean --yes` parses, resolves to Execute, and
   does not alter the mode. *Fails if:* a consolidation adds `--yes` to the
   root, or binds it.
9. **The generated artifacts are current.** `generate-docs -- --check` is
   green and no artifact in `man/` or `completions/` is stale. *Fails if:* the
   rendered help changed without regeneration.

## Verification commands

The ladder, in order; stop at the first failure.

~~~sh
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets --all-features
python3 scripts/check-fixture-portability.py --self-test && python3 scripts/check-fixture-portability.py
python3 scripts/check-doc-citations.py --self-test && python3 scripts/check-doc-citations.py
cargo run --quiet --features dev-tools --bin generate-docs -- --check
~~~

Focused runs while iterating:

~~~sh
cargo test --all-features --lib cli::tests::
cargo test --all-features --test cli_contract
~~~

The **MSRV lane** (`cargo +1.89 check --locked --all-targets` and
`cargo +1.89 test --locked --all-targets`) and the hosted
ubuntu/macos/windows lanes are required before closure, because this change
touches argv parsing — the surface where the platform-specific fixtures
differ.

## Documentation updates

- `architecture/02-cli.md` — the alias's role: a **compatibility surface for
  published spellings**, not the mechanism by which the bare invocation is
  CWD-anchored. The two must not be conflated again.
- `architecture/overview.md` §10 — findings **68** and **69** recorded, and
  finding **65**'s status updated to "planned by C045".
- `src/cli.rs` — the `CleanupOverrides` doc comment (D3) and the alias's
  rationale comment (WP-5).
- `docs/USAGE.md` — **no change required.** The bare invocation's semantics
  are already documented (`:25`, `:113-120`, `:226`, `:378`) and the alias is
  already documented (`:33`, `:42-44`, `:69-70`, `:77`, `:82`, `:91`). This was
  checked before the plan was written; the documentation is not the defect.
- `docs/RELEASING.md`, `README.md` — no change required.

## Acceptance criteria

- The cleanup flag surface has **one** declaration. A reader of `src/cli.rs`
  can find every cleanup flag in one place, and adding a flag requires one
  edit, not four.
- The scope rule is implemented **once**.
- The converse compatibility guard (test 2) exists, reads the clap model, and
  is green. A root cleanup flag that the alias would reject **cannot** be
  added without a failing test.
- The whole published spelling set (test 1) agrees.
  > **Amendment (2026-10-22).** The criterion asked for the test to be
  > written *before* the production change and to pass unchanged after it.
  > The permanent test was written after the change; the *property* it
  > asserts was pinned before it, by a temporary harness that rendered the
  > parse outcome of **394** argv shapes — every scope selector × override ×
  > mode × level, plus every conflict pair and every read-only-subcommand
  > shape — and diffed the two renders byte for byte. The diff was empty,
  > and the harness was removed once the permanent test replaced it. The
  > evidence is stronger than the criterion asked for; the criterion's
  > *sequencing* was not met, and is recorded as such.
- Every one of the nine required tests exists, passes, and names its failing
  input.
- `CleanupOverrides`'s doc comment no longer denies the shipped surface.
- The alias's rationale comment states its actual purpose and points at ADR
  009 / C040 for the bare invocation's behaviour.
- `cargo cleanme --help` and `cargo cleanme clean --help` are byte-identical
  before and after, and `man/` and `completions/` are unchanged — the last
  clause amended as recorded above: `man/` is unchanged, `completions/`
  changed in the *order* of two option lists and of nothing else, with the
  second amendment above: three of the zsh file's *rendered* conflict hints
  are also narrower, with the effective conflict relation unchanged.
- The ladder is green.
  > **Amendment (2026-10-22).** The criterion also asked for the MSRV and
  > hosted lanes to be green. The **MSRV lane is red at `main`** for a reason
  > this corrective did not cause: the failure reproduces with its changes
  > stashed, and it is a test that asserts the row renderer's label where a
  > qualified host renders the summary's branch. It is recorded as
  > `architecture/overview.md` §10 finding **70** and owned by **C046**; a plan
  > that finds a defect writes a corrective, it does not fix it
  > (`AGENTS.md` §5). The **hosted lanes were not run** — they are not
  > available here, and "not run" is the honest entry.
- `architecture/overview.md` §10's finding count is re-derived from the table,
  not copied forward.

## Stop conditions

Stop and write a corrective rather than continuing if:

- the consolidation **cannot** preserve every published spelling exactly — in
  particular, if any flag the alias accepts today cannot be carried by a
  single shared declaration without changing its conflict behaviour or its
  hide state;
- the scope rule cannot be collapsed without changing a resolved intent (the
  scope, the selection, the mode, or the policy evaluation);
- the converse guard turns out to require the alias to accept a flag it
  legitimately must not — that would mean the *design* is wrong, not the
  implementation, and would need an owner decision rather than an edit;
- a rendered help text or a generated artifact would have to change and the
  change cannot be regenerated cleanly;
- any test would have to assert an unrelated failure to pass;
- the alias's hide states cannot be preserved, i.e. `clean` would reappear in
  the root help or lose `--dry-run` / `--known`.

## Closure evidence requirements

A closure record is written under `plans/closure/artifact-discovery-cleanup/`
as `c045-status.md`, and it must carry:

- the implementation commit or PR;
- a requirement-to-evidence matrix: each invariant I1–I8 and each acceptance
  criterion mapped to the test that proves it;
- the **verification commands actually run**, with their results — a command
  that was skipped must be named as skipped, not omitted;
- the MSRV lane result and the hosted ubuntu/macos/windows lane results,
  because this change touches argv parsing;
- the before/after rendered `cargo cleanme --help` and
  `cargo cleanme clean --help`, with a diff;
- the `man/` and `completions/` diff, which must be empty — **amended**: the
  `man/` diff is empty; the `completions/` diff is the two option-list
  reorderings and the three narrowed zsh conflict hints recorded above, and
  it is reported in full rather than as empty;
- evidence that the alias's published spellings were exercised end-to-end
  through the built binary, not only through unit tests — at minimum
  `clean --known`, `clean --full`, `clean ROOT`, `clean --dry-run`,
  `clean --dryrun` and `clean --yes`, each against the root spelling;
- known limitations;
- unresolved findings classified by severity;
- a disposition: `closed`, `conditionally closed`, `corrective required`, or
  `blocked`.

**Publication is not closure.** This corrective authorises no release. If it
lands before M019 and M018, it ships with them or after them; the canonical
spelling must never point at an operation that refuses, and M019 may not be
published before M018 has shipped.

## Planning review (`plans/003-planning-process.md` §12)

1. **Durable behavior is unambiguous** — yes: the resolved intent is frozen
   (I1–I3) and the alias's purpose is now written down.
2. **Destructive behavior is not introduced ahead of its dependency** — yes:
   this changes no destructive behaviour; it changes how the surface is
   declared.
3. **Scope/filter precedence is explicit** — unchanged and untouched.
4. **Filesystem failure semantics are explicit** — unchanged; no filesystem
   behaviour is touched.
5. **Concurrency/parallelism does not weaken correctness** — no concurrency is
   touched.
6. **Cross-platform behavior is testable** — argv parsing is platform-neutral,
   but the MSRV and hosted lanes are required before closure.
7. **Configuration compatibility is defined** — no configuration effect.
8. **Verification is specific** — nine tests, each naming its failing input,
   plus the ladder and the hosted lanes.
9. **Closure evidence is measurable** — the matrix and the command list above.
10. **Stop conditions prevent silent scope expansion** — five stop
    conditions, including the one that would send this back to an owner
    decision rather than an edit.

## Related open items this corrective does not close

- **The 48 pre-M019 spelling comments** across `src/cleanup.rs` (13),
  `src/cli.rs` (22), `src/main.rs` (8) and `src/policy.rs` (5). Stale but not
  false — the alias still works. Recorded for a later sweep; not swept here.
- **The safety-critical prose coupling and copy-paste findings** surfaced by the
  same 2026-10-22 investigation — `is_authorization_infrastructure_error`
  deciding the exit code by substring-matching prose
  (`src/cleanup.rs:1350-1352`), and `path_is_symlink` existing four times
  byte-identically (`src/cleanup.rs:3077`, `src/workspace.rs:234`,
  `src/discovery.rs:475`, `src/policy.rs:228`). These are **not** part of the
  cleanup invocation surface and are **not** in this corrective's scope. They
  are safety-path defects and need their own correctives; this plan names them
  so they are not lost, and this plan's authority does not extend to them.
- **The `update` surface's hand-rolled JSON producers** (`src/main.rs:118-132`
  and `:159-173`, duplicating the tested `output::update_json`). Outside this
  line.
