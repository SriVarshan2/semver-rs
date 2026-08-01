# DECISIONS.md

Running log of real decisions made during the 72-hour build. Entries are
timestamped (UTC) and added as they happen — not reconstructed afterward.

---

## [Pre-kickoff] Repo selection

**Decision:** Port `rbarrois/python-semanticversion` (Python -> Rust, Track D),
not the originally-scouted `karpathy/micrograd`.

**Why:** micrograd's core (`engine.py` + `nn.py`) is ~150-250 lines, well
under the hackathon's 1,000-source-line floor for a bring-your-own repo, and
it isn't in the official repo pool either. Rather than pad an artificial repo
to clear the floor, we pivoted to a repo that's honestly sized and pre-vetted
for the track.

**Verification performed pre-kickoff:**
- `cloc semantic_version/base.py semantic_version/__init__.py` -> 1,112 lines
  of code (excludes `django_fields.py`, which is Django-specific glue we are
  intentionally not porting — see below).
- License confirmed: BSD 2-Clause (OSS, permissive).
- Searched for an existing Rust port of this specific repo — none found.
  Only Python-side mirrors/forks exist (e.g. `alvistack/rbarrois-python-semanticversion`).

**What we're NOT porting:** `semantic_version/django_fields.py`. It's a Django
model field integration, not core semver logic, and porting it would pull in
a web-framework dependency with no equivalent purpose in a standalone Rust
crate. This is a scope decision, not an oversight.

---

## [Pre-kickoff] Equivalence-proof strategy

**Decision:** Prioritize a PyO3/maturin adapter so the *original* pytest
suite (`tests/original/`) can run unmodified against the Rust port, over
hand-reimplementing test logic natively in Rust.

**Why:** The hackathon rules explicitly rank "original tests execute
unmodified against your port" above "reimplementing test logic 1:1" —
the former proves more. Test parity & proof is 25% of the score.

**Test suite inventory (kickoff, verbatim, hashed):**
- `test_base.py` — 28,716 bytes
- `test_match.py` — 5,051 bytes
- `test_npm.py` — 3,424 bytes
- `test_parsing.py` — 4,579 bytes
- `test_spec.py` — 5,688 bytes
- SHA256 hashes recorded in `tests/original/HASHES.txt`

---

## [Pre-kickoff] Port order

**Decision:** Port in this order: `Version` parsing/comparison ->
`Version.coerce()` -> `SimpleSpec` -> `NpmSpec`.

**Why:** `Version` is the foundation everything else depends on — get it
provably correct first. `NpmSpec` is the most complex grammar (`||`,
hyphen ranges, `x`-ranges) and the likeliest source of subtle bugs, so it's
last, when we have the most context and the most test coverage to lean on.

**Known tricky spot flagged in advance:** the library's own docs note that
`SimpleSpec('<1.3.4')` deliberately does NOT match `1.3.4-rc2`, which
diverges from strict SemVer 2.0.0 precedence rules (where a pre-release
version like `1.3.4-rc2` sorts *before* `1.3.4`, and so technically satisfies
`<1.3.4` under strict comparison). This is an intentional design choice in
the original library, not a bug — we need to replicate it exactly, not
"fix" it, or we'll silently break behavioral equivalence.

---

## [Template for entries during the build]

## [YYYY-MM-DD HH:MM UTC] <short title>

**What happened:**

**Decision:**

**Why:**

**Trade-off / what we'd reconsider:**

---

## [Version module complete] Ord vs PartialOrd, and partial-version scope

**What happened:** While porting `Version`'s comparison operators, found that
the original Python's ordering (`__lt__`/`__gt__`) deliberately excludes
build metadata, while `__eq__` includes it. This means two versions like
`0.1.1+build1` and `0.1.1+build2` are `!=` but neither `<` nor `>` the other.

**Decision:** In Rust, implemented `PartialOrd` + `PartialEq` for `Version`,
but deliberately did NOT implement `Ord`. Rust's `Ord` trait requires a total
order consistent with `Eq` — the original's semantics violate that contract,
so implementing `Ord` would either be dishonest (silently "fixing" the
original's behavior) or would panic/misbehave if used with things like
`.sort()`. `PartialOrd`-only is the accurate port of the original's actual
behavior.

**Why this matters:** A less careful port might implement `Ord` for
convenience (e.g. to support `Vec<Version>::sort()`), which would silently
change behavior versus the original on any version set containing build
metadata. This is exactly the kind of subtle equivalence gap our differential
fuzz harness is meant to catch — we caught it by reading the source closely
instead.

**Also decided:** Did not port `partial=True` mode (incomplete versions like
`Version('1.2', partial=True)`). The original library itself deprecates this
feature for removal in 3.0. Scope cut, not an oversight — verified via
12 passing unit tests covering the non-partial path, including the numeric-
vs-alphanumeric prerelease identifier ordering and the "release outranks its
own prerelease" precedence rule.

**Status:** `Version` module complete — parsing, `coerce()`, precedence
comparison, `next_major/minor/patch`, `Display`. 12/12 tests passing.
