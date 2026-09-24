#!/usr/bin/env python3
# created: 2026-07-23 | author: Claude Opus 4.8 (1M context), directed by @SaulBuilds
# WP-S1.5 — regenerate Rust RBAC bindings from the chain's compiled ABIs.
#
# Generates, per contract: its function selectors (4-byte, straight from foundry's
# methodIdentifiers — no keccak needed) and its event canonical signatures, as
# const Rust data. This is the deterministic primitive layer the app builds
# calldata dispatch and log filtering on; richer typed encode/decode arrives with
# alloy in a later sprint. Deterministic, network-free, crypto-free.
#
# ⚠️ These bindings are generated FRESH from citrate-chain ABIs. They must NEVER
# be copied from citrate-defense_prime-shell/gui/citrate_rbac_bindings — that repo is
# PRIVATE and customer-specific to a different account (Q2). Prior art only.
#
# Usage:
#   scripts/gen_rbac_bindings.py            # regenerate into src/generated/
#   scripts/gen_rbac_bindings.py --check    # fail if committed output is stale
#
# The chain repo location is resolved relative to this repo, or via
# CITRATE_CHAIN_DIR. No network access; if the ABIs are absent it says so and
# exits non-zero rather than inventing anything.
import json
import os
import shutil
import subprocess
import sys
from pathlib import Path

# The six BFR-02 RBAC contracts (01_RESEARCH_BASELINE.md §3.1). Order is stable
# so the generated file is byte-deterministic.
CONTRACTS = [
    "TenantHierarchy",
    "RoleEscalation",
    "ClassificationRegistry",
    "MultiSigEnvelope",
    "AgentDecisionRegistryV2",
    "ContradictionLedger",
]

HERE = Path(__file__).resolve().parent
REPO = HERE.parent
OUT_DIR = REPO / "src-tauri" / "crates" / "quorum-rbac" / "src" / "generated"


def chain_contracts_dir() -> Path:
    env = os.environ.get("CITRATE_CHAIN_DIR")
    candidates = []
    if env:
        candidates.append(Path(env) / "contracts")
    candidates.append(REPO.parent / "citrate-chain" / "contracts")
    for c in candidates:
        if (c / "out").is_dir():
            return c
    sys.stderr.write(
        "ERROR: could not find citrate-chain/contracts/out. Set CITRATE_CHAIN_DIR "
        "or place citrate-chain alongside this repo, and run `forge build` there "
        "so the ABIs exist.\n"
    )
    sys.exit(2)


def canonical_sig(entry: dict) -> str:
    """The canonical signature string used for selectors/topics."""

    def typ(io: dict) -> str:
        # Expand tuples so the signature matches solc's canonical form.
        if io["type"].startswith("tuple"):
            inner = ",".join(typ(c) for c in io.get("components", []))
            suffix = io["type"][len("tuple"):]  # handles tuple[] / tuple[2]
            return f"({inner}){suffix}"
        return io["type"]

    args = ",".join(typ(i) for i in entry.get("inputs", []))
    return f'{entry["name"]}({args})'


def rust_ident(name: str) -> str:
    out = []
    for i, ch in enumerate(name):
        if ch.isupper() and i and not name[i - 1].isupper():
            out.append("_")
        out.append(ch.upper())
    return "".join(out)


def ensure_artifacts_are_current(contracts_dir: Path) -> None:
    """Rebuild the ABIs before reading them. Refuse to guess if we cannot.

    THIS GUARD EXISTS BECAUSE THE FAILURE IT PREVENTS ALREADY HAPPENED (2026-08-01).

    The `forge build` artifacts in `contracts/out` had gone five weeks stale (Jun 21)
    against sources edited Jul 26. This script read them happily and produced bindings
    MISSING `transferGovernance`, `acceptGovernance`, `pendingGovernance` and both
    governance-transfer events for three contracts — the two-step ownership handoff,
    which is a security control.

    `scripts/check.sh` then reported "RBAC bindings are STALE. Run
    scripts/gen_rbac_bindings.py and commit the result." Following that instruction
    would have DELETED the bindings for that control and called it a drift fix. The
    committed bindings were correct the whole time; the generator's INPUT was not.

    That is the general hazard: a regenerator with stale input does not fail, it
    quietly narrows the truth — and a drift check then turns that into an
    instruction. (`emit-address-table.sh` carries the same shape; the 2026-07-27
    chain handoff §8 warns it would delete eleven manually-pinned addresses.)

    Building first is the fix rather than a freshness heuristic, because heuristics
    are wrong in both directions here: `forge` does not rewrite an artifact whose
    output is unchanged, so comparing mtimes reports false staleness for a source
    that was merely touched, while a source edited in the same second as its build
    reports false freshness. Rebuilding removes the question.
    """
    forge = shutil.which("forge")
    if not forge:
        sys.stderr.write(
            "ERROR: `forge` is not on PATH, so the ABIs in\n"
            f"  {contracts_dir / 'out'}\n"
            "cannot be proven current. Generating from a stale artifact silently\n"
            "DROPS functions and events that exist on chain — on 2026-08-01 exactly\n"
            "that deleted the two-step governance-transfer bindings from three\n"
            "contracts, and the drift check told the operator to commit the result.\n"
            "Install Foundry, or run `forge build` in the contracts directory and\n"
            "re-run with RBAC_SKIP_BUILD=1 to accept the artifacts as-is.\n"
        )
        sys.exit(2)
    proc = subprocess.run(
        [forge, "build"],
        cwd=contracts_dir,
        capture_output=True,
        text=True,
    )
    if proc.returncode != 0:
        sys.stderr.write(
            f"ERROR: `forge build` failed in {contracts_dir}; refusing to generate "
            f"from artifacts that may not describe the current sources.\n"
            f"{proc.stderr[-2000:]}\n"
        )
        sys.exit(2)


def gen_contract(name: str, contracts_dir: Path) -> str:
    abi_path = contracts_dir / "out" / f"{name}.sol" / f"{name}.json"
    if not abi_path.is_file():
        sys.stderr.write(f"ERROR: missing ABI {abi_path}\n")
        sys.exit(2)
    data = json.loads(abi_path.read_text())
    abi = data["abi"]
    method_ids = data.get("methodIdentifiers", {})

    funcs = [e for e in abi if e.get("type") == "function"]
    events = [e for e in abi if e.get("type") == "event"]

    lines = [f"/// Bindings for `{name}.sol` — generated, do not edit by hand.",
             f"pub mod {name.lower()} {{"]
    lines.append(f'    /// Solidity contract name.')
    lines.append(f'    pub const CONTRACT: &str = "{name}";')
    lines.append("")
    lines.append("    /// 4-byte function selectors, keyed to the canonical signature.")
    lines.append("    /// `(name, canonical_sig, selector_be)`")
    lines.append(f"    pub const FUNCTIONS: &[(&str, &str, [u8; 4])] = &[")
    for f in sorted(funcs, key=lambda e: e["name"]):
        sig = canonical_sig(f)
        sel_hex = method_ids.get(sig)
        if sel_hex is None:
            # foundry keys methodIdentifiers by canonical sig; if absent, skip
            # loudly rather than emit a wrong selector.
            sys.stderr.write(f"WARN: no selector for {name}.{sig}; skipping\n")
            continue
        b = bytes.fromhex(sel_hex)
        sel = ", ".join(f"0x{x:02x}" for x in b)
        lines.append(f'        ("{f["name"]}", "{sig}", [{sel}]),')
    lines.append("    ];")
    lines.append("")
    lines.append("    /// Event canonical signatures (topic0 = keccak256 of these,")
    lines.append("    /// computed by the caller with a keccak impl at runtime).")
    lines.append(f"    pub const EVENTS: &[&str] = &[")
    for e in sorted(events, key=lambda x: x["name"]):
        lines.append(f'        "{canonical_sig(e)}",')
    lines.append("    ];")
    lines.append("}")
    return "\n".join(lines)


def render() -> str:
    contracts_dir = chain_contracts_dir()
    # Prove the ABIs describe the CURRENT sources before reading a byte of them.
    if os.environ.get("RBAC_SKIP_BUILD") != "1":
        ensure_artifacts_are_current(contracts_dir)
    header = [
        "// @generated by scripts/gen_rbac_bindings.py — DO NOT EDIT.",
        "// Source: citrate-chain compiled ABIs for the six BFR-02 RBAC contracts.",
        "// Regenerate: scripts/gen_rbac_bindings.py   ·   Verify: --check (CI drift).",
        "//",
        "// Generated FRESH from chain ABIs, never copied from the private",
        "// citrate-defense_prime-shell rbac bindings (Q2 — different account).",
        "",
    ]
    body = [gen_contract(c, contracts_dir) for c in CONTRACTS]
    all_names = ", ".join(f'"{c}"' for c in CONTRACTS)
    footer = [
        "",
        "/// Every contract this module binds, for the drift/coverage test.",
        f"pub const CONTRACTS: &[&str] = &[{all_names}];",
        "",
    ]
    return "\n".join(header) + "\n\n".join(body) + "\n" + "\n".join(footer)


def main() -> int:
    check = "--check" in sys.argv[1:]
    generated = render()
    target = OUT_DIR / "rbac.rs"
    if check:
        if not target.is_file():
            sys.stderr.write("ERROR: generated bindings missing; run without --check\n")
            return 1
        current = target.read_text()
        if current != generated:
            sys.stderr.write(
                "ERROR: RBAC bindings are STALE. Run scripts/gen_rbac_bindings.py "
                "and commit the result.\n"
            )
            return 1
        print("rbac bindings up to date")
        return 0
    OUT_DIR.mkdir(parents=True, exist_ok=True)
    target.write_text(generated)
    print(f"wrote {target.relative_to(REPO)}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
