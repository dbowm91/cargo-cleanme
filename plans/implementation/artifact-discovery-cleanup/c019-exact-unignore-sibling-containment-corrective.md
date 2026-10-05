# C019 — Exact Unignore Sibling-Containment Corrective

Status: closed — see [`plans/closure/artifact-discovery-cleanup/c019-status.md`](../../closure/artifact-discovery-cleanup/c019-status.md)

Repository baseline: `a150eda8b43e8bf2f088aec8b804e7fa557c3a18`

Corrects:

- `plans/implementation/artifact-discovery-cleanup/002-fast-project-discovery-and-scope-filters.md`
- `plans/closure/artifact-discovery-cleanup/002-status.md`

Source roadmap:

- Phase 11 — hardening, release trust, and qualification automation
- `plans/subsystems/artifact-discovery-cleanup-roadmap.md`

Primary class: corrective / discovery-scope correctness

Severity: medium

Hard dependencies: none.

Operational closure dependency: the repository's unrelated README/release-contract
drift at the planning baseline must be green or explicitly reconciled before
C019 is closed, so hosted evidence cannot be accepted from an already-red
baseline.

## 1. Objective

Restore the exact `scan.unignore` semantics that M002 specified and its closure
claimed.

A literal ignored directory must remain excluded except for the ancestry needed
to reach an explicitly re-included directory and the subtree rooted at that
exact unignore path.

Given:

~~~toml
[scan]
ignore = ["/projects/archive"]
unignore = ["/projects/archive/keep"]
~~~

and sibling projects:

~~~text
/projects/archive/keep/Cargo.toml
/projects/archive/other/Cargo.toml
~~~

Routine/Full discovery must discover `keep` and must not discover `other`.

The corrective is about search-scope selection. It must not weaken or redesign
the later Cargo ownership, recency, authorization, final-proof, or cleanup
safety gates.

## 2. Canonical contract

The durable contract is already unambiguous:

- the long-term specification requires "explicit re-included directories under
  otherwise ignored trees";
- an explicit unignore path takes precedence over an ignore pattern;
- the walker must retain enough ancestry to reach an unignored descendant even
  when its ancestor matches an ignore rule;
- the terminology defines an unignore as an **explicit absolute directory
  path**, not an inverse glob;
- M002 required an exact unignore matcher plus an ancestor-reachability index;
- M002 explicitly says that when an ignored ancestor contains an exception,
  traversal continues **only as needed to reach the exception**;
- M002 acceptance required both "ignored sibling pruning" and
  "re-included descendant reachable."

C019 therefore restores existing direction. It does not change canonical
configuration semantics.

## 3. Current defect evidence

At the baseline, `src/discovery.rs` implements filtering approximately as:

~~~text
ignored(path) =
    ignore_glob_matches(path)
    && path is not at/below an unignore path

exception_below(path) =
    some unignore path is at/below path
~~~

The traversal descends when the current path is not directly ignored or an
exception exists below it.

That works for the historical test:

~~~text
ignore   = /archive/*
unignore = /archive/keep
~~~

because `/archive/other` itself matches the glob and is pruned.

It fails for the canonical literal-directory case:

~~~text
ignore   = /archive
unignore = /archive/keep
~~~

The exception forces traversal through `/archive`, but `/archive/other`
does not itself match the literal ignore rule, so the current stateless
per-path check forgets that the child is still beneath an ignored ancestor.
The sibling is traversed and its manifest is discovered.

The documentation audit reproduced this against 0.1.6 with two projects:
`keep` and `other`. The literal-ancestor configuration discovers two
manifests rather than one.

## 4. Why M002 verification missed it

The M002 plan called for:

- exact unignore;
- ignored ancestor + unignored descendant;
- ignored sibling pruning.

The closure marked those requirements passed.

However, the surviving production fixture
`discovery_reaches_unignored_project_without_entering_ignored_sibling` uses:

~~~text
ignore = <archive>/*
unignore = <archive>/keep
~~~

and the unit test `filters_ignored_parent_keeps_exception_route` uses the same
shape.

Those tests prove that a sibling which **directly matches the ignore glob** is
pruned while an exception is reachable. They do not prove that exclusion
inherited from a **literal ignored ancestor** remains in force while the walker
passes through that ancestor to reach an exception.

The closure therefore overclaimed exact-unignore coverage. Preserve that
historical closure and correct it here; do not rewrite M002 history.

## 5. Required semantics

Implementation must satisfy this path-state model regardless of the internal
mechanism chosen.

For one effective ignore/unignore policy:

1. A directory directly matched by an ignore rule enters ignored state.
2. If that directory is an ancestor of an exact unignore path, the walker may
   traverse it as a **pass-through ancestor**.
3. Pass-through permission does not clear exclusion for sibling branches.
4. A sibling branch beneath the ignored ancestor remains excluded unless it is
   itself on an ancestry chain to another explicit unignore path.
5. At the exact unignore directory, ignored state is cleared for that
   re-included subtree. Descendants of that exact unignore path remain
   re-included under the existing product contract.
6. Multiple unignore paths under one ignored ancestor must each remain
   reachable without admitting unrelated siblings.
7. Nested ignored ancestors must compose correctly.
8. Explicit CLI/configured scan roots continue to bypass user ignore/unignore
   filters entirely.
9. Internal safety pruning (symlinks, VCS metadata, Cargo/rustup/system prunes,
   target-tree pruning) always wins and is not bypassed by unignore.
10. Path comparison must preserve the repository's current canonical/absolute
    configuration behavior and platform case/path handling; C019 must not invent
    a second normalization model.

The implementation may use inherited traversal state, a compiled policy trie,
rule attribution, or another bounded representation. A stateless
"does this exact child match a glob?" decision is insufficient unless it can
also prove inherited ignored state.

## 6. Non-goals

C019 does not:

- change ignore syntax from glob-style patterns;
- turn unignore into an inverse glob language;
- make relative ignore/unignore paths valid;
- change explicit-root filter bypass;
- change Routine or Full root selection;
- alter learned-root retention/state semantics;
- add new cleanup authorization;
- weaken complete ownership resolution or fresh final cleanup proof;
- fix unrelated README/release-contract documentation ownership drift;
- redesign dua-core traversal generally;
- optimize global traversal except where the corrective avoids unnecessary
  sibling descent.

## 7. Work package A — Encode filter disposition explicitly

Replace the current binary `ignored(path)` / `exception_below(path)`
decision with an internal disposition sufficient to distinguish at least:

- included;
- ignored/prunable;
- ignored but pass-through because an exception is below;
- exact re-included subtree.

Names are implementation detail; the state distinction is not.

The traversal decision must be derivable deterministically from the configured
ignore rules, exact unignore paths, current path, and inherited filter state.

Keep the representation bounded by configured rule count/path depth. Do not
materialize the discovered filesystem tree merely to evaluate filters.

## 8. Work package B — Preserve inherited ignored state through traversal

Teach the dua-core descent policy, or the smallest surrounding traversal
adapter necessary, to retain exclusion inherited from an ignored ancestor.

Required behavior:

~~~text
/archive                    direct ignore match; pass-through only
/archive/keep               exact unignore; include/re-enter
/archive/keep/...           included subtree
/archive/other              inherited ignore; prune
/archive/other/deeper       never visited
~~~

With two exceptions:

~~~text
unignore = [
  "/archive/keep-a",
  "/archive/nested/keep-b",
]
~~~

only the ancestry chains required to reach those two paths may be traversed
through the ignored region.

Do not solve this by expanding the user's literal `/archive` configuration
into an undocumented `/archive/**` rule. The configured pattern and exact
exception semantics must remain observable as authored.

## 9. Work package C — Reconcile instrumentation with the real prune decision

The repository reports `user_ignore_prunes` and total pruned directories.

After the fix:

- a sibling pruned because it inherits exclusion from an ignored ancestor
  counts as a user-ignore prune;
- a pass-through ancestor is not counted as pruned merely because an ignore
  rule matched it;
- exact unignore ancestry must not double-count pruning;
- `directories_pruned` accounting retains the established invariant against
  its component counters.

If dua-core does not yield a directory whose descent predicate rejects it in a
way that permits exact accounting, preserve truthful counters rather than
fabricating a count. Any counter-contract change requires explicit review.

## 10. Work package D — Correct user documentation after behavior lands

The current `docs/USAGE.md` and `docs/TROUBLESHOOTING.md` accurately
document the 0.1.6 defect and the `/**` workaround.

After implementation:

- move that text into a historical/fixed-version note rather than presenting
  the workaround as required semantics;
- document that a literal ignored ancestor plus exact unignore now admits only
  the exception subtree;
- retain the existing rule that explicit roots bypass filters;
- retain absolute-path requirements;
- add the corrective to `CHANGELOG.md` for the release that carries it.

Do not erase the fact that 0.1.6 and earlier exhibit the broader behavior.

## 11. Regression matrix

At minimum add fixture tests for:

### Literal ignored ancestor

~~~text
ignore   = archive
unignore = archive/keep
projects = archive/keep, archive/other
expected = keep only
~~~

This is the mandatory premise-negative test: it must fail against the baseline
implementation.

### Existing wildcard behavior

~~~text
ignore   = archive/*
unignore = archive/keep
expected = keep only
~~~

This must continue to pass.

### Whole-subtree ignore workaround shape

~~~text
ignore   = archive/**
unignore = archive/keep
expected = keep subtree only
~~~

Preserve currently documented behavior.

### Multiple exceptions

Two exact unignore paths under one ignored ancestor are reachable; every other
sibling stays pruned.

### Nested exception

An unignore several levels below an ignored ancestor remains reachable while
siblings at each intermediate level remain excluded.

### Re-included subtree

A Cargo manifest directly at the unignore root and a nested Cargo project
beneath that root remain discoverable according to existing subtree semantics.

### Explicit scope bypass

Scanning the ignored ancestor explicitly continues to bypass ignore/unignore
and sees the full explicit scope.

### Internal-prune precedence

An unignore path must not cause traversal into:

- `.git` / VCS metadata;
- `target` trees;
- symlink directories;
- Cargo/rustup/system prunes where those policies apply.

### Cross-platform paths

Exercise the same logical fixture on Linux, macOS, and Windows hosted CI.
Include path canonicalization/case behavior already supported by the repository;
do not add platform-specific semantics merely to satisfy a fixture.

## 12. Cleanup-safety regression evidence

The defect occurs in discovery scope, so closure must prove that the correction
does not bypass downstream cleanup safety.

Add or reuse an end-to-end fixture showing:

- the ignored sibling is absent from Routine/Full discovery after the fix;
- the exact unignored project can still become a normal resolved workspace;
- explicit cleanup-root behavior remains unchanged because explicit roots
  bypass user discovery filters;
- no change occurs to ownership class, authorization, activity, marker, or
  final-proof semantics.

C019 should remove unintended candidates earlier; it must not create a shortcut
around later proof.

## 13. Failure and cancellation semantics

No new persistence or mutation is introduced.

If filter-state evaluation cannot determine a safe traversal disposition due to
path/metadata ambiguity, preserve the existing conservative diagnostic/prune
direction rather than broadening discovery.

Cancellation/drop behavior remains owned by the traversal engine and existing
scan orchestration.

## 14. Performance requirements

The fix must not turn Routine/Full filter evaluation into a filesystem-tree-sized
memory structure or an O(number of discovered entries × unbounded filesystem
state) algorithm.

Measure the filtered-tree fixture with many ignored siblings and one/few
exceptions. The corrected implementation should prune siblings before Cargo
resolution and deep analysis.

A small rule-count/path-depth cost is acceptable; broad traversal of an ignored
subtree is not.

## 15. Compatibility

This is an intentional correction to behavior that violated the existing
configuration contract.

Compatibility consequences:

- configurations using a literal ignored ancestor plus an exact unignore become
  **narrower and correct**: unintended siblings disappear;
- configurations using `/*` or `/**` workarounds continue to behave
  correctly;
- explicit-root scans remain unchanged;
- no config migration or schema version bump is required;
- machine-readable output may contain fewer discovered/reportable workspaces
  because the search scope is corrected, not because the JSON schema changed.

## 16. Verification

At minimum:

~~~text
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets --all-features
cargo test --doc
cargo +1.89 check --locked --all-targets
cargo +1.89 test --locked --all-targets
python3 scripts/check-fixture-portability.py --self-test
python3 scripts/check-fixture-portability.py
python3 scripts/check-doc-citations.py --self-test
python3 scripts/check-doc-citations.py
bash scripts/release-check.sh
~~~

Hosted Linux, macOS, and Windows CI is required.

Because the planning baseline currently has unrelated README/release-contract
failures, closure must cite a hosted run from a green repository state rather
than treating "only the C019 tests passed" as sufficient.

## 17. Acceptance criteria

C019 closes only when:

- the literal ignored-ancestor/exact-unignore regression discovers only the
  intended exception subtree;
- the premise-negative regression demonstrably fails against the baseline;
- wildcard `/*` and recursive `/**` configurations retain their intended
  behavior;
- multiple/nested exceptions do not admit unrelated siblings;
- exact unignore does not override internal safety prunes;
- explicit-root bypass remains unchanged;
- user-ignore prune instrumentation remains truthful;
- the fix does not broaden cleanup authority or bypass ownership/freshness
  proof;
- documentation distinguishes affected historical releases from the corrected
  behavior;
- Linux/macOS/Windows hosted CI and Rust 1.89 gates are green;
- no medium-or-higher filter-scope finding remains.

## 18. Stop conditions

Stop and open a narrower design/ADR discussion if:

- fixing sibling containment requires changing the public meaning of an
  unignore path from "exact directory re-entry" to a glob/inverse-glob system;
- dua-core cannot carry enough traversal context without replacing the
  established bounded walker architecture;
- canonicalization/case semantics differ materially across supported platforms
  and would require a new public path-matching policy;
- the correction changes explicit-root precedence;
- implementation evidence shows the current `/**` workaround relies on a
  different public semantic than the canonical M002 contract.

Do not rewrite the specification to bless the 0.1.6 behavior.

## 19. Closure evidence

Record:

- implementation commit and release tag carrying the correction, if released;
- original M002 plan/closure references;
- baseline reproduction showing two manifests instead of one;
- premise-negative test result against the old implementation;
- new literal/wildcard/recursive/multiple/nested exception matrix;
- hosted Linux/macOS/Windows run IDs;
- MSRV result;
- filter/prune counter evidence;
- documentation/version disposition;
- unresolved findings by severity;
- final disposition.
