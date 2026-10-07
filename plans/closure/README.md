# Closure Records

Closure records are written after implementation and live under:

~~~text
plans/closure/<subsystem>/<NNN>-status.md
~~~

A closure record must identify implementation commits, map requirements to evidence, list verification commands actually run, record platform/fixture evidence, classify unresolved findings, and give a disposition of closed, conditionally closed, corrective required, or blocked.

A record is named after the thing it closes. Most things are plans and use their
number. A **publication** is not a plan and has no number, so a release record is
named `r<NNN>-status.md` for the released version's last component — `r022` for
0.2.2. That form was introduced by the v0.2.2 release record and is used from
there on.

Do not create a success record merely because the crate compiles.
