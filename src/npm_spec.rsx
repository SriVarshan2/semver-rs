//! Port of `NpmSpec` from python-semanticversion's `base.py`.
//!
//! The prerelease-matching algorithm here is genuinely subtle and easy to
//! get wrong by guessing — see DECISIONS.md for the full writeup of what
//! we initially got wrong and how we corrected it against the real source.
//!
//! Ground truth (from base.py's `NpmSpec.Parser`):
//! 1. `parse_simple` builds all Ranges using a single `range()` helper that
//!    ALWAYS applies `PrereleasePolicy::SamePatch` — there is no per-operator
//!    Natural/SamePatch conditional. This is uniform, not selective.
//! 2. The actual prerelease-vs-release split happens one level up, in
//!    `parse()`, per space-separated block within a `||`-joined group:
//!    - For each subclause whose target carries a prerelease, synthesize an
//!      extra bound clause with `PrereleasePolicy::Always`:
//!      - operator ∈ {Gt, Gte} → extra `Lt` bound at `major.minor.(patch+1)`
//!      - operator ∈ {Lt, Lte} → extra `Gte` bound at `major.minor.0` (no prerelease)
//!      The original subclause itself is also collected into this group.
//!      All of these go into `prerelease_clauses`.
//!    - A truncated-target duplicate (prerelease stripped, same SamePatch
//!      policy) of the ORIGINAL clause goes into `non_prerel_clauses` —
//!      always, whether or not the target had a prerelease.
//!    - The block's contribution is `AllOf(prerelease_clauses) OR
//!      AllOf(non_prerel_clauses)` (only the AllOf(prerelease_clauses) part
//!      is added if prerelease_clauses is non-empty).
//! 3. Groups (split on `||`) are OR'd together into the final result.

use crate::spec::{BuildPolicy, Clause, Operator, PrereleasePolicy, Range, SpecError};
use crate::version::Version;
use once_cell::sync::Lazy;
use regex::Regex;

#[derive(Debug, Clone, PartialEq)]
pub struct NpmSpec {
    pub expression: String,
    pub clause: Clause,
}

impl NpmSpec {
    pub fn parse(expression: &str) -> Result<Self, SpecError> {
        let clause = Parser::parse(expression)?;
        Ok(NpmSpec {
            expression: expression.to_string(),
            clause,
        })
    }

    pub fn matches(&self, version: &Version) -> bool {
        self.clause.matches(version)
    }

    /// Highest matching version among `versions`, or `None` if none match.
    /// Linear scan via `>` since `Version` only implements `PartialOrd`.
    pub fn select<'a>(&self, versions: impl IntoIterator<Item = &'a Version>) -> Option<&'a Version> {
        versions
            .into_iter()
            .filter(|v| self.matches(v))
            .fold(None, |best, v| match best {
                None => Some(v),
                Some(b) if v > b => Some(v),
                Some(_) => best,
            })
    }
}

static NPM_SPEC_BLOCK_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"(?x)^
        v?
        (?P<op><|<=|>=|>|=|\^|~|)
        (?P<major>x|X|\*|0|[1-9][0-9]*)
        (?:\.(?P<minor>x|X|\*|0|[1-9][0-9]*)
            (?:\.(?P<patch>x|X|\*|0|[1-9][0-9]*))?
        )?
        (?:-(?P<prerel>[a-zA-Z0-9.-]*))?
        (?:\+(?P<build>[a-zA-Z0-9.-]*))?
        $",
    )
    .unwrap()
});

/// Always uses SamePatch policy — mirrors `Parser.range()`.
fn range_helper(operator: Operator, target: Version) -> Result<Range, SpecError> {
    Range::new(operator, target, PrereleasePolicy::SamePatch, BuildPolicy::Implicit)
        .map_err(|e| SpecError::InvalidBlock(e.to_string()))
}

struct Parser;

impl Parser {
    fn parse(expression: &str) -> Result<Clause, SpecError> {
        let mut result = Clause::Never;
        for group in expression.split("||") {
            let group = group.trim();
            let group_owned;
            let group: &str = if group.is_empty() {
                ">=0.0.0"
            } else {
                group_owned = group.to_string();
                &group_owned
            };

            let subclauses: Vec<Range> = if group.contains(" - ") {
                let mut parts = group.splitn(2, " - ");
                let low = parts.next().unwrap();
                let high = parts.next().unwrap();
                let mut v = Self::parse_simple(&format!(">={}", low))?;
                v.extend(Self::parse_simple(&format!("<={}", high))?);
                v
            } else {
                let mut v = Vec::new();
                for block in group.split(' ') {
                    if block.is_empty() {
                        continue;
                    }
                    if !NPM_SPEC_BLOCK_RE.is_match(block) {
                        return Err(SpecError::InvalidBlock(block.to_string()));
                    }
                    v.extend(Self::parse_simple(block)?);
                }
                v
            };

            let mut prerelease_clauses: Vec<Clause> = Vec::new();
            let mut non_prerel_clauses: Vec<Clause> = Vec::new();

            for clause in subclauses {
                if !clause.target.prerelease.is_empty() {
                    match clause.operator {
                        Operator::Gt | Operator::Gte => {
                            let bump = Version {
                                major: clause.target.major,
                                minor: clause.target.minor,
                                patch: clause.target.patch + 1,
                                prerelease: vec![],
                                build: vec![],
                            };
                            prerelease_clauses.push(Clause::Range(
                                Range::new(Operator::Lt, bump, PrereleasePolicy::Always, BuildPolicy::Implicit)
                                    .map_err(|e| SpecError::InvalidBlock(e.to_string()))?,
                            ));
                        }
                        Operator::Lt | Operator::Lte => {
                            let floor = Version {
                                major: clause.target.major,
                                minor: clause.target.minor,
                                patch: 0,
                                prerelease: vec![],
                                build: vec![],
                            };
                            prerelease_clauses.push(Clause::Range(
                                Range::new(Operator::Gte, floor, PrereleasePolicy::Always, BuildPolicy::Implicit)
                                    .map_err(|e| SpecError::InvalidBlock(e.to_string()))?,
                            ));
                        }
                        _ => {}
                    }
                    prerelease_clauses.push(Clause::Range(clause.clone()));
                    non_prerel_clauses.push(Clause::Range(
                        range_helper(clause.operator, clause.target.truncate())?,
                    ));
                } else {
                    non_prerel_clauses.push(Clause::Range(clause));
                }
            }

            let mut group_clause = Clause::AllOf(non_prerel_clauses);
            if !prerelease_clauses.is_empty() {
                group_clause = Clause::AllOf(prerelease_clauses).or(group_clause);
            }
            result = result.or(group_clause);
        }
        Ok(result)
    }

    fn is_empty_component(t: Option<&str>) -> bool {
        matches!(t, None | Some("*") | Some("x") | Some("X"))
    }

    /// Port of `parse_simple` — returns a Vec because caret/tilde/eq-partial
    /// expand into two Ranges (lower + upper bound).
    fn parse_simple(simple: &str) -> Result<Vec<Range>, SpecError> {
        let caps = NPM_SPEC_BLOCK_RE
            .captures(simple)
            .ok_or_else(|| SpecError::InvalidBlock(simple.to_string()))?;

        let raw_op = caps.name("op").map(|m| m.as_str()).unwrap_or("");
        let prefix = if raw_op.is_empty() { "=" } else { raw_op };

        let major_t = caps.name("major").map(|m| m.as_str());
        let minor_t = caps.name("minor").map(|m| m.as_str());
        let patch_t = caps.name("patch").map(|m| m.as_str());
        let prerel_raw: Option<&str> = caps.name("prerel").map(|m| m.as_str());
        let mut build_raw: Option<&str> = caps.name("build").map(|m| m.as_str());

        if build_raw.is_some() && prefix != "=" {
            build_raw = None;
        }

        let major_is_empty = Self::is_empty_component(major_t);
        let minor_is_empty = Self::is_empty_component(minor_t);
        let patch_is_empty = Self::is_empty_component(patch_t);

        let major: Option<u64> = if major_is_empty { None } else { Some(major_t.unwrap().parse().unwrap()) };
        let minor: Option<u64> = if minor_is_empty { None } else { minor_t.map(|s| s.parse().unwrap()) };
        let patch: Option<u64> = if patch_is_empty { None } else { patch_t.map(|s| s.parse().unwrap()) };

        let mut prefix = prefix;
        let target: Version = if major.is_none() {
            if prefix != "=" && prefix != ">=" {
                return Err(SpecError::InvalidBlock(simple.to_string()));
            }
            prefix = ">=";
            Version { major: 0, minor: 0, patch: 0, prerelease: vec![], build: vec![] }
        } else if minor.is_none() {
            Version { major: major.unwrap(), minor: 0, patch: 0, prerelease: vec![], build: vec![] }
        } else if patch.is_none() {
            Version { major: major.unwrap(), minor: minor.unwrap(), patch: 0, prerelease: vec![], build: vec![] }
        } else {
            let prerelease = match prerel_raw {
                None | Some("") => vec![],
                Some(s) => s.split('.').map(String::from).collect(),
            };
            let build = match build_raw {
                None | Some("") => vec![],
                Some(s) => s.split('.').map(String::from).collect(),
            };
            Version { major: major.unwrap(), minor: minor.unwrap(), patch: patch.unwrap(), prerelease, build }
        };

        let incomplete = major.is_none() || minor.is_none() || patch.is_none();
        let prerel_truthy = matches!(prerel_raw, Some(s) if !s.is_empty());
        let build_truthy = matches!(build_raw, Some(s) if !s.is_empty());
        if incomplete && (prerel_truthy || build_truthy) {
            return Err(SpecError::InvalidBlock(simple.to_string()));
        }

        let to_range_err = |e: SpecError| e;

        match prefix {
            "^" => {
                let high = if target.major != 0 {
                    target.truncate().next_major()
                } else if target.minor != 0 {
                    target.truncate().next_minor()
                } else if minor.is_none() {
                    target.truncate().next_major()
                } else if patch.is_none() {
                    target.truncate().next_minor()
                } else {
                    target.truncate().next_patch()
                };
                Ok(vec![
                    range_helper(Operator::Gte, target).map_err(to_range_err)?,
                    range_helper(Operator::Lt, high).map_err(to_range_err)?,
                ])
            }
            "~" => {
                let high = if minor.is_none() {
                    target.next_major()
                } else {
                    target.next_minor()
                };
                Ok(vec![
                    range_helper(Operator::Gte, target).map_err(to_range_err)?,
                    range_helper(Operator::Lt, high).map_err(to_range_err)?,
                ])
            }
            "=" => {
                if major.is_none() {
                    Ok(vec![range_helper(Operator::Gte, target).map_err(to_range_err)?])
                } else if minor.is_none() {
                    Ok(vec![
                        range_helper(Operator::Gte, target.clone()).map_err(to_range_err)?,
                        range_helper(Operator::Lt, target.next_major()).map_err(to_range_err)?,
                    ])
                } else if patch.is_none() {
                    Ok(vec![
                        range_helper(Operator::Gte, target.clone()).map_err(to_range_err)?,
                        range_helper(Operator::Lt, target.next_minor()).map_err(to_range_err)?,
                    ])
                } else {
                    Ok(vec![range_helper(Operator::Eq, target).map_err(to_range_err)?])
                }
            }
            ">" => {
                if minor.is_none() {
                    Ok(vec![range_helper(Operator::Gte, target.next_major()).map_err(to_range_err)?])
                } else if patch.is_none() {
                    Ok(vec![range_helper(Operator::Gte, target.next_minor()).map_err(to_range_err)?])
                } else {
                    Ok(vec![range_helper(Operator::Gt, target).map_err(to_range_err)?])
                }
            }
            ">=" => Ok(vec![range_helper(Operator::Gte, target).map_err(to_range_err)?]),
            "<" => Ok(vec![range_helper(Operator::Lt, target).map_err(to_range_err)?]),
            "<=" => {
                if minor.is_none() {
                    Ok(vec![range_helper(Operator::Lt, target.next_major()).map_err(to_range_err)?])
                } else if patch.is_none() {
                    Ok(vec![range_helper(Operator::Lt, target.next_minor()).map_err(to_range_err)?])
                } else {
                    Ok(vec![range_helper(Operator::Lte, target).map_err(to_range_err)?])
                }
            }
            _ => Err(SpecError::InvalidBlock(simple.to_string())),
        }
    }
}
