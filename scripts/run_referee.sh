#!/usr/bin/env bash
set -euo pipefail

echo "== ferric-semver referee =="
echo "1. Verifying original test files are unmodified..."
cd tests/original
sha256sum -c HASHES.txt
cd ../..

echo "2. Building Rust port (release)..."
cargo build --release

echo "3. TODO: run original pytest suite against Rust port via PyO3 adapter"
echo "4. TODO: run differential fuzz harness (random version strings/ranges)"
echo "5. TODO: emit pass-rate report"
