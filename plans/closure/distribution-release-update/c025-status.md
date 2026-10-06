# C025 — Post-v0.2.1 planning and status reconciliation — status

Status: **closed.**

Plan: `plans/implementation/distribution-release-update/c025-post-v0.2.1-planning-status-reconciliation.md`

Repository baseline: `35905c5841effadcdb32e3ad4b5f396e4e9096ef`

## 1. What this record is, and what it is not

C025 was a **control-surface corrective**. It changed no Rust source, no release
bytes, no glob semantics, no updater behaviour, no yank state, no workflow
behaviour, no version, and no acceptance-matrix cell. `src/` is byte-identical
to the baseline; `Cargo.toml` reads `0.2.1` as it did before.

What it changed is the **agreement between the planning record and the
evidence already in it**. At the baseline, current-status sentences in the
registry, both subsystem roadmaps, and the canonical roadmap contradicted the
accepted closure records they summarise. That drift is the defect. C025 removed
it without touching a single run id, digest, conclusion, date, or historical
evidence item.

Because the change is documentation-only, this is closed rather than
conditionally closed: there is no acceptance criterion that depends on a future
event, and no product qualification left outstanding by this plan.

## 2. Implementation commits

| Commit | Work |
|---|---|
| `6afd279` | the C025 plan itself |
| `2853e7e` | register C025 in `plans/registry.md`; correct the active release-status row |
| `9f05268` | register C025 in the distribution roadmap; correct its status header |
| `a834f41` | cross-register C025 in the cleanup roadmap |
| `288a878` | register C025 in the canonical long-term roadmap |
| `966f60ade410bfc41b68e37e9aa0160ecfc85ec2` | **the reconciliation itself** — work packages A–F |

`966f60a` is the substantive commit. The four commits before it registered the
plan; they are cited here so the record is complete rather than because they
carried work.

## 3. Requirements mapped to evidence

Each acceptance criterion in §13 of the plan, in the plan's order, and where it
is satisfied.

| Plan §13 criterion | Where satisfied | How it was checked |
|---|---|---|
| C023's closure header says conditionally closed and agrees with its disposition | `c023-status.md:3`; disposition `:589` | `grep -n '^Status:'` and `grep -n 'Conditionally closed'`. Both use the same normative vocabulary; §14 still marks criterion 11 not met |
| C023's finding count agrees with §11 | `c023-status.md:3` and §15 | `grep -c '^### 11\.'` returns **5** (§11.1–§11.5). The header count was derived from that heading count, not remembered. The three-finding claim at `artifact-discovery-cleanup-roadmap.md:744` is **C021's** count, a different record, and was left alone |
| C023 §13 no longer falsely upgrades M011C | `c023-status.md` §13 | Now reads "M011C remains conditionally closed … it is not upgraded to confirmed-green" |
| M011C contains the v0.2.1 automatic-failure/manual-green addendum and remains conditionally closed | `m011c-status.md` v0.2.1 addendum; disposition `:5` | The addendum appends evidence and keeps the disposition **conditionally closed**. It states that no green automatic run exists and none can be recreated |
| Registry subsystem rows, open-work text, next-release note, and published state agree with the evidence | `plans/registry.md`: subsystem row 38, open work, next-release note, published state | Read in full after editing; each states C023 conditional, M011C conditional, C024 the handoff, and a dated yank fact (§7) |
| Distribution roadmap no longer claims observed-green automatic v0.2.1 smoke | §12 header, §11.4, §12.4, §12.5 | §11.4 now separates trigger-fired (done) from trigger-green (outstanding) |
| Artifact roadmap no longer marks C023 ready/current | §10J header, §14, §10J status line | §10J is conditionally closed and points at the closure record; §14 no longer calls C023 the active corrective |
| Long-term roadmap treats v0.2.1 publication and v0.2.0 yank as completed | `002-long-term-roadmap.md`: Phase 11, 12, 13 headers, exit condition, corrective section, C025 section | Future-tense publication/yank converted to past tense; the corrective path now carries a per-step outcome |
| C024 remains the next substantive product corrective | `registry.md` open work; both roadmaps | C024's plan and status rows were re-read and are unchanged; it is named the next implementation handoff |
| Current release/version prose reflects `Cargo.toml = 0.2.1` | `registry.md`, `002-long-term-roadmap.md` | `Cargo.toml` verified at `0.2.1`. The `Cargo.toml reads 0.2.0` claim was restated as "has read 0.2.0 or later ever since (it is 0.2.1)" rather than left to rot |
| All historical failed run IDs/results remain intact | everywhere | Run `37507738147` is recorded as **failed** at every occurrence. §4 below lists the deliberate non-edits |
| Doc/release/post-smoke contract guards pass | §6 | All green |
| No contradictory active status remains in the audited surfaces | §5 | Cross-surface sweep; every surface listed in plan §10 was audited, including `Cargo.toml` and `CHANGELOG.md` |

## 4. Version and current-release prose audit

Work package F (plan §9) required each version/current-release statement to be
one of: **historical statement whose date/context should remain**, **stale
current-state statement to correct**, or **deliberate future acceptance criterion
that should remain future tense**. This is that audit, and it is why the table
below has three groups rather than a list of edits.

### Corrected

| Location | Was | Now |
|---|---|---|
| `c023-status.md:3` | `Status: **closed**` | `Status: **conditionally closed**`, with the unmet criterion named |
| `c023-status.md` §13 | "M011C upgraded to a confirmed-green automatic trigger" | "M011C remains conditionally closed … it is not upgraded to confirmed-green" |
| `c023-status.md` §15 | "Two findings are handed on" with no count of the rest | the five-finding classification, derived from §11 |
| `m011c-status.md` | closed at the v0.2.0 cycle; no v0.2.1 evidence | v0.2.1 addendum; disposition unchanged |
| `registry.md` distribution row | "C025 ready", C024 described in parallel | "no active release corrective; M011C conditional" |
| `registry.md` open work | C025 named the immediate handoff | C024 named the next implementation handoff |
| `registry.md` published state | "every other version is not" yanked — unbounded | rechecked live; see §7 |
| `registry.md` M012A row | sentence fragment (`Execute. the destructive-default break …`) | sentence repaired |
| distribution roadmap header, §11.4, §12 header, §12.4, §12.5 | "C025 ready"; M011C "first real release-event smoke evidence outstanding" | per-step outcomes stated |
| cleanup roadmap §10J and §14 | "C023 is now the active safety corrective"; §14 called C023 active | conditionally closed; C024 named the handoff |
| `002-long-term-roadmap.md` | Phase 13 "C023 ready"; future-tense publication/yank; ordered corrective path as pre-implementation intent | published-and-corrected status; past tense; per-step outcome sequence |

### Still accurate, deliberately left alone

- Every historical run result in every closure record. Run `37507738147`
  **failed**; it is recorded as failed everywhere it appears.
- `013-status.md:371` — the appended C023 corrective note says "C023 owns the
  repair and the disclosure". That is an ownership statement made under the
  explicit heading "Corrective note added by C023", it is true (C023 did own
  and deliver the repair), and it does not assert that C023 is open. Rewriting a
  timestamped note in a closed record would be the failure mode this repository
  keeps re-learning, not an improvement.
- The pre-implementation body text of both roadmaps' C023 sections, now labelled
  in place as history with a pointer to the closure record.

### Historical narrative that must stay

- Everything describing v0.2.0 publication, and the C023 pre-implementation
  intent ("stop patch staging", "C023 has priority over ordinary roadmap work").
  These were true when written and are the reason C023 exists.

## 5. Consistency audit

Post-edit cross-surface sweep, `grep` over `plans/`:

| Check | Result |
|---|---|
| C023 closure header says conditionally closed | ✅ `c023-status.md:3` |
| C023 disposition uses the same word | ✅ `c023-status.md:589` |
| C023 §14 still marks criterion 11 not met | ✅ unchanged |
| C023 §11 has 5 numbered findings; header says five | ✅ `grep -c '^### 11\.'` = 5 |
| C023 plan header no longer claims ready | ✅ conditionally closed, points at the closure record |
| M011C disposition still conditionally closed | ✅ `m011c-status.md:5` |
| M011C addendum records automatic **failed** + manual **green** | ✅ runs `37507738147` / `37508262225` |
| No active surface says C023 "ready / immediate" | ✅ only inside the C025 plan describing the defect |
| No active surface says M011C/automatic smoke green | ✅ none |
| No active surface says "main is at 0.2.0" | ✅ none |
| No active surface says the next release is 0.2.1 | ✅ none |
| Registry, roadmaps, canonical roadmap agree on C023/M011C/C024 | ✅ |
| `Cargo.toml` version | ✅ `0.2.1` |
| C024 left untouched and still open/ready | ✅ unchanged |

## 6. Verification actually run

Reported as run, in the order run. Nothing here is claimed that was not
executed.

```text
python3 scripts/check-doc-citations.py --self-test          → self test passed
python3 scripts/check-doc-citations.py                      → 16 documents clean
python3 scripts/check-fixture-portability.py --self-test    → self test passed
python3 scripts/check-fixture-portability.py                → 35 fixture files clean
python3 scripts/check-release-contract.py --self-test       → pass
python3 scripts/check-release-contract.py                   → pass
python3 scripts/check-post-release-smoke-contract.py --self-test → pass
python3 scripts/check-post-release-smoke-contract.py       → pass
python3 scripts/check-staged-validation-contract.py --self-test  → pass
python3 scripts/check-staged-validation-contract.py         → pass
python3 scripts/check-installer-contract.py --self-test     → pass
python3 scripts/check-installer-contract.py                 → pass
cargo run --quiet --features dev-tools --bin generate-docs -- --check
                                                           → 13 artifacts match
```

**Deliberately not run, and why:**

- `cargo fmt` / `cargo clippy` / `cargo test`. C025 changed no Rust;
  `git diff --stat` against the baseline touches `plans/` only. Running the
  build ladder would have been theatre, and this repository treats an
  unearned green as worse than no record.
- `scripts/release-check.sh`. Per `AGENTS.md` §3 it requires a clean tree, the
  `eggpack` binary, and toolchain 1.89, and is explicitly not routine
  verification. Every documentation gate it invokes was run individually above
  instead, and each passed.

## 7. External state rechecked rather than asserted

Work package C (plan §6) forbade repeating "every other version is not
yanked" unless the evidence was actually rechecked during implementation. It
was, on 2026-10-06:

- **crates.io API** `cargo-cleanme/versions` → `total: 9`; `yanked: true` for
  `0.2.0` alone; `0.2.1` through `0.1.0` all `yanked: false`. The 0.2.0 yank
  appears as an explicit audit action at `2026-10-06T16:21:49Z`.
- **GitHub releases API** → nine releases; `v0.2.1` (id 405000990) and `v0.2.0`
  (id 404345035) both `immutable: true`.

The registry now states these as dated, sourced facts instead of an unbounded
claim about every version. If a future yank occurs, this line is dated and
knows how to be rechecked.

## 8. Platform and fixture evidence

**Not applicable, and here is why rather than merely omitted.** C025 produced no
binary, ran nothing on any platform, and touched no fixture. The evidence C025
deals with is documentary — a set of mutually inconsistent status statements —
and the check that a status statement is true is a comparison against the record
it summarises. That is §3 and §5.

This is also the reason C025 closes rather than closing conditionally. Its
verification is not deferred to a future run or a future platform; there is
nothing left that a later run could settle.

## 9. Discrepancies found and how they were classified

| # | Discrepancy | Classification | Resolution |
|---|---|---|---|
| 9.1 | C023 header said `closed` while its own §12.6 recorded the automatic run failing and §14 marked criterion 11 not met | **summary contradicted accepted evidence** | corrected; recorded in C023 §13A rather than silently edited, because a closure record whose summary changed should say why |
| 9.2 | C023 §13 claimed M011C was upgraded to a confirmed-green automatic trigger — a claim no evidence supports, and which §12.6 contradicts | **false automatic-green claim** | removed; M011C stays conditional |
| 9.3 | C023 header said "three findings" against five numbered headings | **count drift** | count re-derived from the headings |
| 9.4 | Registry asserted "every other version is not" yanked without a recheck | **unbounded claim** | rechecked live (§7) and restated as a dated fact |
| 9.5 | C023's **plan** header still read `Status: ready` | **status field stale** | set to conditionally closed with a pointer to the closure record, matching how C019/C021/C022 plans were marked. The status field is live metadata; the body is untouched history |
| 9.6 | Registry M012A row contained a sentence fragment introduced by `2853e7e` | **editorial defect introduced by C025 registration** | repaired; found by reading the file, not by any checker |
| 9.7 | Canonical roadmap described completed publication and yank in future tense | **stale status prose** | converted to past tense with per-step outcomes |
| 9.8 | `013-status.md` "C023 owns the repair" | **historical, deliberately unchanged** | left as written; see §4 |

**No new implementation plan was opened by this reconciliation**, and none
needed to be. 9.1–9.3 were summary drift over existing evidence; 9.4 was an
evidence gap that a live check closed; 9.5–9.7 were editorial; 9.8 was correct
history. The one open product finding remains **C024**, which was already open
before C025 and which C025 deliberately did not touch.

## 10. Effect on other plans

| Plan | Effect |
|---|---|
| **C024** | **Unblocked, and now the next implementation handoff.** Its scope, status, and semantics are untouched — it is not made more urgent by this reconciliation. Its registry and roadmap rows were re-read to confirm they still say open/ready. |
| **C023** | **Closed, conditionally.** The qualification is unchanged: acceptance criterion 11. C025 did not upgrade it and did not weaken it. |
| **M011C** | **Still conditionally closed.** C025 added the v0.2.1 evidence it was missing and did not upgrade it. |
| **M013** | Unaffected. Not reopened, nothing retracted. |
| **C019/C020/C021/C022** | Unaffected. |

**Nothing is unblocked by C025 that was blocked by a qualification.** What C025
did is remove the ambiguity that made it unclear what the current state *was*.
The one qualification that mattered — a green automatic M011C run — still has
no plan attached to it, and that is correct: it is an operational dependency on
the next publication, not a body of work waiting to be written down. Writing a
plan for it would be a way of appearing to make progress.

## 11. Disposition

**Closed.**

Every acceptance criterion in §13 of the plan is met and traced to a specific
location in §3 above. The consistency audit in §5 passes on every surface
listed. Every documentation gate was run and is green, with the two exceptions
in §6 stated with their reasons rather than omitted.

No product code changed, no release was performed, no yank state altered, no
failed run was rewritten as passing, and C024's semantics were left alone.