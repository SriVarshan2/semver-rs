# semver-rs

A Rust port of [`rbarrois/python-semanticversion`](https://github.com/rbarrois/python-semanticversion),
built for **Port_Mortem 2026 · Code Resurrection** (Track D: Python → Rust).

Original license: BSD 2-Clause. See `THIRD_PARTY_LICENSE` (TODO: add original
LICENSE file copy for attribution).

## What this ports

- `Version` — parsing, comparison, `coerce()`, `next_major/minor/patch()`
- `SimpleSpec` — the library's own range-matching syntax
- `NpmSpec` — full npm-style range grammar

**Not ported:** `django_fields.py` (Django integration glue — out of scope
for a standalone Rust crate; see DECISIONS.md).

## Build & run (single command)

```bash
docker compose up
```

This builds the Rust port, verifies the original test files are unmodified
(hash check), and runs the referee/fuzz pipeline in an isolated Alpine
container.

## Proving equivalence

- `tests/original/` — the original Python test suite, copied verbatim, with
  SHA256 hashes recorded at kickoff in `tests/original/HASHES.txt`.
- (TODO) A PyO3/maturin adapter lets these original tests run unmodified
  against this Rust implementation.
- (TODO) A differential fuzz harness generates version strings and range
  specs, runs them through both the Python original and this Rust port, and
  diffs the results.

## Status

Scaffold only — port in progress. See `DECISIONS.md` for the running
decision log.

## Pass rate

TODO — updated honestly as the port progresses, including where it fails.
