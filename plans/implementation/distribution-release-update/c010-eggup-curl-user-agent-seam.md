# C010 — Upstream: a User-Agent seam on the Eggup curl transport

Status: proposed

Repository baseline: `95bb38e2bf2b335211bff4498089016f6d4239eb` (v0.1.1)

Closure: not applicable. This is a bounded **upstream** request. It cannot be
closed by work in this repository, and its status must not be reported as
`blocked` on this project — nothing here is waiting on it.

Source roadmap: Phase 10 — Distribution, release, and update

Subsystem roadmap: `plans/subsystems/distribution-release-update-roadmap.md`

Related: `plans/closure/distribution-release-update/010c-status.md` (records
the finding and the transport reversal),
`plans/closure/distribution-release-update/010d-status.md`.

## 1. Objective

Ask upstream `eggup-curl` for the one configuration seam cargo-cleanme needs
and cannot add itself: a caller-supplied `User-Agent` for the HTTP requests the
adapter issues.

This plan deliberately does **not** ask upstream to solve cargo-cleanme's
problem for it. It asks for a missing capability in the adapter, and it states
the minimum surface that would be sufficient.

## 2. Why this cannot be worked around in this repository

The requirement is "the update transport must identify itself to the version
authority". The current curl adapter cannot express that. Three escape hatches
were considered and all three are closed by the adapter's own behavior:

1. **Pass `--user-agent` / `-A`.** The adapter builds the curl argument vector
   itself and exposes no way to append arguments. Verified in
   `eggup-curl-0.1.2/src/lib.rs`: the vector is built from a fixed set of
   `--location`, `--max-redirs`, `--connect-timeout`, `--max-time`,
   `--max-filesize`, `--proto`, `--proto-redir`, `--noproxy`, `--output`,
   `--write-out`, and `--`, with no argument seam.

2. **Set a `CURL_*` environment variable.** The adapter calls
   `cmd.env_clear()` and then restores only the proxy variables it manages
   (`eggup-curl-0.1.2/src/lib.rs`, `env_clear()` followed by a loop over
   `proxy_env`). curl has no `CURL_USER_AGENT` variable in any case, so this
   hatch is doubly closed.

3. **Use a `~/.curlrc`.** `env_clear()` also removes `HOME`, so curl cannot
   locate a default rc file. Supplying a rc file explicitly would require
   hatch 1.

The only remaining options are ones the planning process explicitly forbids
here: writing a second HTTP client, or hand-rolling a private protocol around
the curl executable. Both would duplicate transport policy that Eggup already
owns, and both would be worse than the change actually being requested.

## 3. The evidence that makes this a correctness issue, not a preference

crates.io — the version authority for a registry-backed updater — answers
**HTTP 403** to requests whose `User-Agent` is not descriptive. Measured
directly against the live registry during the 0.1.0 release smoke:

| Request `User-Agent` | Response |
|---|---|
| absent | 403 |
| `curl/8.5.0` (the adapter's actual output) | 403 |
| a descriptive agent naming product and version | 200 |

A transport that cannot read the version authority cannot perform the
transaction it exists to perform. Its smaller footprint does not qualify it.

## 4. Requested upstream change

Add a `user_agent` builder to `CurlConfig`, consistent with the existing
builders in that type (`timeouts`, `max_redirects`, `follow_redirects`,
`allowed_protocols`, `proxy`), and emit it as `--user-agent <value>` in the
argument vector.

Minimum surface, so the request stays bounded and reviewable:

~~~rust
impl CurlConfig {
    /// Set the `User-Agent` sent with every request.
    ///
    /// A descriptive agent is required by some registries; the curl default
    /// (`curl/<version>`) is rejected by crates.io with HTTP 403.
    pub fn user_agent(mut self, agent: impl Into<String>) -> Self;
}
~~~

Points an upstream reviewer should settle, offered as questions rather than
demands:

- **Default or unset?** If the default is a descriptive library-identifying
  agent rather than curl's own, that alone fixes the problem for every
  consumer. If the default stays curl's, the seam must be used explicitly.
- **Composition.** Should the adapter accept a full agent string verbatim, or
  compose a base (`eggfetch-curl/<version>`) with a caller-supplied product
  token, the way `EggfetchConfig::user_agent` is already used? The two
  transports should not disagree about shape.
- **Empty-value handling.** An empty or whitespace-only agent should be
  rejected rather than passed through, so a caller cannot silently re-create
  the anonymous request this change exists to prevent.
- **Redaction.** Request URLs are redacted in adapter error messages. An agent
  string supplied by a caller should be treated the same way if it is ever
  interpolated into an error.

## 5. In scope for upstream

- `CurlConfig::user_agent` and the corresponding argument emission.
- A test asserting the agent reaches the child process, and a test asserting
  the empty-value refusal.
- Documentation of the new default, if a default is introduced.

## 6. Out of scope

- Any change to cargo-cleanme. This repository has already resolved its own
  side of this: see the 0.1.1 transport reversal in
  `plans/closure/distribution-release-update/010c-status.md`.
- A second transport in cargo-cleanme, or a curl fallback kept alongside
  eggfetch. A fallback that cannot reach the version authority is a second way
  to fail, and keeping it would invite a future contributor to reintroduce the
  403.
- Changing the `env_clear()` hardening. It is sound; the gap is the missing
  argument seam, not the environment handling.

## 7. What upstream acceptance would mean here

If `eggup-curl` gains the seam, the follow-up question for this repository is
narrow and should be decided on evidence, not assumed: the embedded
HTTP/TLS stack in 0.1.1 costs 6,704,960 release bytes and 95 normal
dependencies. Whether that is worth paying again to reclaim them is a real
question, and it is deliberately left open here. It is **not** a reason to hold
this request: the correct production transport today is `eggup-eggfetch`,
which works, and it remains the correct one until upstream ships and the
alternative is measured.

## 8. Verification

For upstream acceptance, from `eggup-curl`:

~~~sh
cargo test -p eggup-curl
~~~

The new test must fail before the change and pass after it, and must assert the
literal `User-Agent` header observed by a local test server rather than
inspecting the constructed argument vector. An argument-vector assertion would
pass while the value never reached the wire, which is the class of bug this
whole plan exists because of: a property that was implemented in the config but
never proven in the request.
