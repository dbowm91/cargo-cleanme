# C024 — Windows glob canonicalization is a no-op

Status: **open — ready**. Opened by
[`c023-status.md`](../../closure/distribution-release-update/c023-status.md) §11 as
an unresolved finding from the 0.2.1 patch-release corrective. It is **not**
part of C023 and must not be folded into it.

## 1. Why this is a separate plan

C023's rule for a requalification that turns up a new defect is explicit: *stop
patch staging and either expand C023 explicitly or open a separate corrective
before release.* C023 chose the second branch for this finding, for three
reasons that are properties of the defect rather than of the schedule:

1. **It is not a regression.** `escape_glob_literal` has always returned `None`
   for a string containing `\`. On Windows `fs::canonicalize` always returns one.
   The rewrite has therefore never done anything on Windows, before or after
   C023. Nothing that worked on Windows stopped working.
2. **It is not destructive.** The reachable failure is a pattern that matches
   less than its author intended, so a directory the user tried to `exclude` is
   cleaned as though no `exclude` had been written. That is the default
   behaviour, not a new hazard — but it is also exactly the direction that
   matters when the user's intent was "never touch this".
3. **The fix is a design decision, not a patch.** Deciding whether Windows glob
   patterns use `/` or `\` as separators, and whether `backslash_escape` is
   enabled, changes what every existing `include`/`exclude` means on Windows.
   §17 of C023 puts changes to matching and authorization semantics out of scope
   for a patch release, and this is precisely that.

## 2. The defect

`canonical_pattern_prefix` (`src/config.rs`) splits a glob at its first
metacharacter, canonicalizes the glob-free prefix, and splices the canonical
spelling back in — so that a directory whose real name contains a glob
metacharacter is matched literally instead of being reinterpreted as a pattern.

The splice is gated on `escape_glob_literal` returning `Some`:

```rust
PatternKind::Glob => match escape_glob_literal(&canonical) {
    Some(literal) => format!("{literal}{separators}{tail}"),
    None => pattern.to_owned(),
},
```

`escape_glob_literal` returns `None` for `{`, `}`, **and `\`. Its own doc
comment says why: `\` is the escape character on Unix but a path separator on
Windows, and a separator cannot be expressed in the escaped form.

Every canonical Windows path contains `\`. `fs::canonicalize` on Windows returns
`C:\Users\…`. So the guard is false for every input on Windows, the `None` arm
is always taken, and the user's pattern is returned unmodified.

**Consequence.** For an `include` or `exclude` pattern whose canonical prefix
names a directory containing a glob metacharacter — `[` and `]` are legal in
Windows filenames — the metacharacters reach `globset` unescaped. `real[abc]`
becomes a character class matching `a`, `b`, or `c`, not the directory named
`real[abc]`. The user's `exclude` then fails to match the tree it names.

`ignore` and `unignore` are unaffected: they are `PatternKind::Literal`, spliced
raw, and were never subject to this.

## 3. What was checked, and what was not

- **Checked.** `a_canonical_spelling_cannot_inject_glob_characters` was gating
  its Unix-only premise behind `#[cfg(not(unix))]`, which bound the link to the
  *real* name so the rewrite never ran — the test could only ever fail, and
  never ran on Windows. It is now `#[cfg(unix)]` with that reason recorded. That
  repair makes the Unix premise honest; it does **not** give Windows coverage,
  because there is no Windows fixture to write until this defect is fixed.
- **Not checked.** No test on any lane exercises the Windows path, because the
  path is unreachable and the behaviour on the other side of it is undefined
  until the separator decision in §4 is made.

## 4. What the corrective must decide

The core question is how a canonical Windows path is spelled inside a
`globset` pattern. Three options, in the order they should be considered:

1. **Normalize the canonical spelling to `/` before splicing**, on every
   platform, and splice the user's own `separators` run verbatim as today. This
   is the smallest change and matches how `globset` treats `/` as a plain
   character. It requires establishing that candidates are matched against
   `/`-spelled paths too — which is a separate, currently unverified premise.
2. **Enable `backslash_escape(false)` on the `Glob` builders** and escape
   `\` as a literal. This makes `\` mean separator everywhere and matches
   Windows user expectation, at the cost of changing what an existing Windows
   pattern means.
3. **Document `\` as unavailable in Windows glob patterns** and require `/`,
   leaving the rewrite disabled there. This is a documentation-only remedy and
   leaves `exclude` unreliable on Windows; it is the weakest and is listed only
   to be rejected on the record.

Whatever is chosen, the corrective must:

- state which platform each option applies to, since a Unix-only fix would
  leave the same defect in any platform whose canonical spelling contains a
  rejected character;
- add a Windows-lane test with a fixture directory whose name contains `[`, `]`,
  `*`, or `?`, proving the pattern matches that directory and does not match a
  sibling that the unescaped reading would match;
- run the fixture-portability guard's `--self-test`, since this is precisely
  the class of defect that guard exists for and the new Windows fixture must be
  allowed to run where it is claimed;
- re-check `docs/USAGE.md` §`include`/`exclude`, which currently describes the
  canonical-root matching without stating the escaping rule or its platform
  limits.

## 5. Acceptance criteria

1. A Windows-lane test proves a bracketed directory name is matched literally
   and its unescaped reading is not.
2. The Unix behaviour is unchanged; the existing Unix test still passes and is
   still discriminating.
3. `escape_glob_literal`'s `None` arm is either unreachable on every supported
   platform, or its remaining inputs are documented and tested individually.
4. `docs/USAGE.md` states the rule and its platform scope.
5. All three hosted lanes green, with the Windows lane observed rather than
   cancelled — the failure that opened this finding was only visible because a
   Windows lane ran at all.

## 6. Relationship to C023

C023 publishes 0.2.1 with this finding **recorded, not fixed**. That is a
deliberate, bounded decision: the release under review is a *destructive*
safety patch, this finding is not destructive, and widening a patch release into
a matching-semantics change is the failure mode C023 exists to avoid. The
finding is recorded in `CHANGELOG.md`, `docs/TROUBLESHOOTING.md`, and the
`c023-status.md` §11 register so that it is visible rather than buried in a
closure record.