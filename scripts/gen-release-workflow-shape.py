#!/usr/bin/env python3
"""Derive the Eggpack reusable-workflow shape from the other Eggpack inputs.

`release/eggpack/workflow-shape.json` is the static, identity-free seam that
`eggpack ci generate --workflow-shape` and `eggpack ci check` consume. Keeping
it hand-maintained would make it a second copy of the producer facts already
held in `pack.toml`, `build-bindings.toml`, `qualification-bindings.toml`,
`consumer-validators.json`, and `distribution.toml`, which is exactly the drift
this subsystem is meant to eliminate.

This script therefore *derives* the shape from those inputs. Run it with
`--check` in CI to prove the checked-in shape is exactly what the inputs
produce; run it with no arguments after an intentional input change to rewrite
the shape.

The script adds no producer semantics: it copies validated input documents
verbatim and only chooses canonical ordering and the staging intent.
"""
from __future__ import annotations

import argparse
import json
import sys
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
EGG = ROOT / "release/eggpack"
SHAPE = EGG / "workflow-shape.json"

# Staging intent is a fixed property of this consumer's release contract:
# Eggpack stages a GitHub draft from the dispatch-supplied exact tag and never
# publishes. It is expressed here (rather than duplicated per input file)
# because it is workflow shape, not build/qualification/binding policy.
STAGING_INTENT = {
    "provider": "git_hub_draft",
    "tag_source": "dispatch_input",
    "required": True,
}


def load_toml(path: Path) -> dict:
    with path.open("rb") as handle:
        return tomllib.load(handle)


def build_shape() -> dict:
    pack = load_toml(EGG / "pack.toml")
    build_bindings = load_toml(EGG / "build-bindings.toml")
    qualification_bindings = load_toml(EGG / "qualification-bindings.toml")
    contract = load_toml(EGG / "distribution.toml")
    with (EGG / "consumer-validators.json").open(encoding="utf-8") as handle:
        consumer_validators = json.load(handle)

    if pack.get("schema_version") != 1:
        raise SystemExit("pack.toml must declare schema_version = 1")
    if build_bindings.get("schema_version") != 1:
        raise SystemExit("build-bindings.toml must declare schema_version = 1")
    if qualification_bindings.get("schema_version") != 1:
        raise SystemExit("qualification-bindings.toml must declare schema_version = 1")

    targets = sorted(pack["targets"], key=lambda policy: policy["target"])
    if not targets:
        raise SystemExit("pack.toml declares no targets")
    triples = [policy["target"] for policy in targets]
    if len(set(triples)) != len(triples):
        raise SystemExit("pack.toml declares a duplicate target")

    for label, document in (
        ("build-bindings.toml", build_bindings.get("targets", {})),
        ("qualification-bindings.toml", qualification_bindings.get("targets", {})),
    ):
        missing = sorted(set(triples) - set(document))
        extra = sorted(set(document) - set(triples))
        if missing or extra:
            raise SystemExit(
                f"{label} does not match pack.toml targets "
                f"(missing={missing}, extra={extra})"
            )

    validator_targets = set(consumer_validators)
    missing = sorted(set(triples) - validator_targets)
    extra = sorted(validator_targets - set(triples))
    if missing or extra:
        raise SystemExit(
            "consumer-validators.json does not match pack.toml targets "
            f"(missing={missing}, extra={extra})"
        )

    # Selected aliases come from the contract in canonical target order so the
    # rendered workflow's job order is a projection of producer authority
    # rather than a separately maintained list.
    aliases: dict[str, list[str]] = {}
    for entry in contract["targets"]:
        aliases[entry["triple"]] = list(entry.get("aliases", []))
    contract_triples = sorted(aliases)
    if contract_triples != triples:
        raise SystemExit(
            "distribution.toml targets do not match pack.toml targets "
            f"(contract={contract_triples}, pack={triples})"
        )
    selected_aliases = [alias for triple in triples for alias in aliases[triple]]
    if not selected_aliases:
        raise SystemExit("distribution.toml declares no target aliases")

    return {
        "schema_version": 1,
        "targets": targets,
        "selected_aliases": selected_aliases,
        "build_bindings": {
            "schema_version": 1,
            "targets": {
                triple: build_bindings["targets"][triple] for triple in triples
            },
        },
        "qualification_bindings": {
            "schema_version": 1,
            "targets": {
                triple: qualification_bindings["targets"][triple]
                for triple in triples
            },
        },
        "consumer_validators": {
            triple: consumer_validators[triple] for triple in triples
        },
        "staging": STAGING_INTENT,
    }


def render(shape: dict) -> str:
    return json.dumps(shape, indent=2, sort_keys=True) + "\n"


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--check",
        action="store_true",
        help="verify the checked-in shape matches the derived shape; never writes",
    )
    args = parser.parse_args()

    rendered = render(build_shape())
    if not args.check:
        SHAPE.write_text(rendered, encoding="utf-8")
        print(f"wrote {len(rendered.encode())} bytes to {SHAPE.relative_to(ROOT)}")
        return 0

    if not SHAPE.is_file():
        print("workflow-shape.json is missing; run scripts/gen-release-workflow-shape.py", file=sys.stderr)
        return 1
    existing = SHAPE.read_text(encoding="utf-8")
    if existing != rendered:
        print(
            "release/eggpack/workflow-shape.json is not derived from the current "
            "Eggpack inputs; run scripts/gen-release-workflow-shape.py",
            file=sys.stderr,
        )
        return 1
    print(f"workflow shape matches derived inputs ({len(rendered.encode())} bytes)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
