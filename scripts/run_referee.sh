#!/usr/bin/env bash
set -euo pipefail

echo "== ferric-semver referee =="
echo "1. Verifying original test files are unmodified..."
cd tests/original
sha256sum -c HASHES.txt
cd ../..

echo "2. Confirming Rust-backed semantic_version is importable..."
python3 -c "import semantic_version; print('semantic_version module loaded from:', semantic_version.__file__)"

echo "3. Running original pytest suite against the Rust port (via PyO3 adapter)..."
python3 -m pytest tests/original/ -v

echo "4. TODO: differential fuzz harness (random version strings/ranges)"
echo "5. Done."
