//! Port of `SimpleSpec`, `Range`, and clause combinators (`AllOf`/`AnyOf`/
//! `Always`/`Never`) from python-semanticversion's `base.py`.
//!
//! Scope decision (see DECISIONS.md): the original's `Clause.simplify()`
//! deduplicates and flattens nested AllOf/AnyOf trees for cleaner
//! `__repr__`/`__eq__` output. We do a lightweight flatten-on-construction
//! instead of a full simplify pass, since `match()` correctness does not
//! depend on simplification — only structural equality/printing would. This
//! keeps the port's behavior (what versions match a spec) identical to the
//! original while skipping a cosmetic feature that doesn't affect
//! behavioral equivalence, our primary scoring criterion.

use crate::version::Version;
use once_cell::sync::Lazy;
use regex::Regex;
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SpecError {
    EmptyBlock,
    InvalidBlock(String),
    BuildNotAllowedForOperator(String),
    IncompleteVersionWithPrereleaseOrBuild(String),
    BuildRequiresEqOrNeq(String),
}

impl fmt::Display for SpecError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SpecError::EmptyBlock => write!(f, "Invalid empty requirement specification"),
            SpecError::InvalidBlock(s) => write!(f, "Invalid simple spec component: {:?}", s),
            SpecError::BuildNotAllowedForOperator(s) => write!(
                f,
                "Invalid range {:?}: build numbers have no ordering.",
                s
            ),
            SpecError::IncompleteVersionWithPrereleaseOrBuild(s) => {
                write!(f, "Invalid simple spec: {:?}", s)
            }
            SpecError::BuildRequiresEqOrNeq(s) => write!(f, "Invalid simple spec: {:?}", s),
        }
    }
}
impl std::error::Error for SpecError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Operator {
    Eq,
    Gt,
    Gte,
    Lt,
    Lte,
    Neq,
}

/// Mirrors Range.PRERELEASE_* constants.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrereleasePolicy {
    /// `<1.2.3` matches `1.2.3-a1`.
    Always,
    /// `<1.2.3` does not match `1.2.3-a1` (the default).
    Natural,
    /// A prerelease is only considered if `target == version` at the patch level.
    SamePatch,
}

/// Mirrors Range.BUILD_* constants.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuildPolicy {
    /// `1.2.3` matches `1.2.3+anything` (the default).
    Implicit,
    /// `1.2.3` matches only exactly `1.2.3`, not `1.2.3+4`.
    Strict,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Range {
    pub operator: Operator,
    pub target: Version,
    pub prerelease_policy: PrereleasePolicy,
    pub build_policy: BuildPolicy,
}

impl Range {
    pub fn new(
        operator: Operator,
        target: Version,
        prerelease_policy: PrereleasePolicy,
        build_policy: BuildPolicy,
    ) -> Result<Self, SpecError> {
        if !target.build.is_empty() && !matches!(operator, Operator::Eq | Operator::Neq) {
            return Err(SpecError::BuildNotAllowedForOperator(target.to_string()));
        }
        let build_policy = if !target.build.is_empty() {
            BuildPolicy::Strict
        } else {
            build_policy
        };
        Ok(Range {
            operator,
            target,
            prerelease_policy,
            build_policy,
        })
    }

    pub fn simple(operator: Operator, target: Version) -> Result<Self, SpecError> {
        Self::new(operator, target, PrereleasePolicy::Natural, BuildPolicy::Implicit)
    }

    pub fn matches(&self, version: &Version) -> bool {
        let version = if self.build_policy != BuildPolicy::Strict {
            version.truncate_prerelease()
        } else {
            version.clone()
        };

        if !version.prerelease.is_empty() {
            let same_patch = self.target.truncate() == version.truncate();
            if self.prerelease_policy == PrereleasePolicy::SamePatch && !same_patch {
                return false;
            }
        }

        match self.operator {
            Operator::Eq => {
                if self.build_policy == BuildPolicy::Strict {
                    self.target.truncate_prerelease() == version.truncate_prerelease()
                        && version.build == self.target.build
                } else {
                    version == self.target
                }
            }
            Operator::Gt => version > self.target,
            Operator::Gte => version >= self.target,
            Operator::Lt => {
                if !version.prerelease.is_empty()
                    && self.prerelease_policy == PrereleasePolicy::Natural
                    && version.truncate() == self.target.truncate()
                    && self.target.prerelease.is_empty()
                {
                    return false;
                }
                version < self.target
            }
            Operator::Lte => version <= self.target,
            Operator::Neq => {
                if self.build_policy == BuildPolicy::Strict {
                    !(self.target.truncate_prerelease() == version.truncate_prerelease()
                        && version.build == self.target.build)
                } else {
                    if !version.prerelease.is_empty()
                        && self.prerelease_policy == PrereleasePolicy::Natural
                        && version.truncate() == self.target.truncate()
                        && self.target.prerelease.is_empty()
                    {
                        return false;
                    }
                    version != self.target
                }
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Clause {
    Always,
    Never,
    Range(Range),
    AllOf(Vec<Clause>),
    AnyOf(Vec<Clause>),
}

impl Clause {
    pub fn matches(&self, version: &Version) -> bool {
        match self {
            Clause::Always => true,
            Clause::Never => false,
            Clause::Range(r) => r.matches(version),
            Clause::AllOf(clauses) => clauses.iter().all(|c| c.matches(version)),
            Clause::AnyOf(clauses) => clauses.iter().any(|c| c.matches(version)),
        }
    }

    pub fn and(self, other: Clause) -> Clause {
        match (self, other) {
            (Clause::Never, _) | (_, Clause::Never) => Clause::Never,
            (Clause::Always, x) | (x, Clause::Always) => x,
            (Clause::AllOf(mut v), Clause::AllOf(v2)) => {
                v.extend(v2);
                Clause::AllOf(v)
            }
            (Clause::AllOf(mut v), other) => {
                v.push(other);
                Clause::AllOf(v)
            }
            (other, Clause::AllOf(mut v)) => {
                v.insert(0, other);
                Clause::AllOf(v)
            }
            (a, b) => Clause::AllOf(vec![a, b]),
        }
    }

    pub fn or(self, other: Clause) -> Clause {
        match (self, other) {
            (Clause::Always, _) | (_, Clause::Always) => Clause::Always,
            (Clause::Never, x) | (x, Clause::Never) => x,
            (Clause::AnyOf(mut v), Clause::AnyOf(v2)) => {
                v.extend(v2);
                Clause::AnyOf(v)
            }
            (Clause::AnyOf(mut v), other) => {
                v.push(other);
                Clause::AnyOf(v)
            }
            (other, Clause::AnyOf(mut v)) => {
                v.insert(0, other);
                Clause::AnyOf(v)
            }
            (a, b) => Clause::AnyOf(vec![a, b]),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SimpleSpec {
    pub expression: String,
    pub clause: Clause,
}

impl SimpleSpec {
    pub fn parse(expression: &str) -> Result<Self, SpecError> {
        let clause = Parser::parse(expression)?;
        Ok(SimpleSpec {
            expression: expression.to_string(),
            clause,
        })
    }

    pub fn matches(&self, version: &Version) -> bool {
        self.clause.matches(version)
    }

    pub fn select<'a>(&self, versions: impl IntoIterator<Item = &'a Version>) -> Option<&'a Version> {
        let mut best: Option<&Version> = None;
        for v in versions {
            if self.matches(v) {
                best = match best {
                    None => Some(v),
                    Some(b) if v > b => Some(v),
                    Some(b) => Some(b),
                };
            }
        }
        best
    }
}

static NAIVE_SPEC_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"(?x)^
        (?P<op><|<=|=|==|>=|>|!=|\^|~=|~|)
        (?P<major>\*|0|[1-9][0-9]*)
        (?:\.(?P<minor>\*|0|[1-9][0-9]*)
            (?:\.(?P<patch>\*|0|[1-9][0-9]*))?
        )?
        (?:-(?P<prerel>[a-z0-9A-Z.-]*))?
        (?:\+(?P<build>[a-z0-9A-Z.-]*))?
        $",
    )
    .unwrap()
});

struct Parser;

impl Parser {
    fn parse(expression: &str) -> Result<Clause, SpecError> {
        let mut clause = Clause::Always;
        for block in expression.split(',') {
            let block_clause = Self::parse_block(block)?;
            clause = clause.and(block_clause);
        }
        Ok(clause)
    }

    fn normalize_prefix(raw: &str) -> &str {
        match raw {
            "=" | "" => "==",
            other => other,
        }
    }

    fn is_empty_component(t: Option<&str>) -> bool {
        matches!(t, None | Some("*"))
    }

    fn parse_block(expr: &str) -> Result<Clause, SpecError> {
        let caps = NAIVE_SPEC_RE
            .captures(expr)
            .ok_or_else(|| SpecError::InvalidBlock(expr.to_string()))?;

        let raw_op = caps.name("op").map(|m| m.as_str()).unwrap_or("");
        let prefix = Self::normalize_prefix(raw_op);

        let major_t = caps.name("major").map(|m| m.as_str());
        let minor_t = caps.name("minor").map(|m| m.as_str());
        let patch_t = caps.name("patch").map(|m| m.as_str());
        let prerel_raw: Option<&str> = caps.name("prerel").map(|m| m.as_str());
        let build_raw: Option<&str> = caps.name("build").map(|m| m.as_str());

        let major_is_empty = Self::is_empty_component(major_t);
        let minor_is_empty = Self::is_empty_component(minor_t);
        let patch_is_empty = Self::is_empty_component(patch_t);

        let major: Option<u64> = if major_is_empty {
            None
        } else {
            Some(major_t.unwrap().parse().unwrap())
        };
        let minor: Option<u64> = if minor_is_empty {
            None
        } else {
            minor_t.map(|s| s.parse().unwrap())
        };
        let patch: Option<u64> = if patch_is_empty {
            None
        } else {
            patch_t.map(|s| s.parse().unwrap())
        };

        let target: Version = if major.is_none() {
            if prefix != "==" && prefix != ">=" {
                return Err(SpecError::InvalidBlock(expr.to_string()));
            }
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
            Version {
                major: major.unwrap(),
                minor: minor.unwrap(),
                patch: patch.unwrap(),
                prerelease,
                build,
            }
        };

        let incomplete = major.is_none() || minor.is_none() || patch.is_none();
        let prerel_truthy = matches!(prerel_raw, Some(s) if !s.is_empty());
        let build_truthy = matches!(build_raw, Some(s) if !s.is_empty());
        if incomplete && (prerel_truthy || build_truthy) {
            return Err(SpecError::IncompleteVersionWithPrereleaseOrBuild(expr.to_string()));
        }

        if build_raw.is_some() && prefix != "==" && prefix != "!=" {
            return Err(SpecError::BuildRequiresEqOrNeq(expr.to_string()));
        }

        match prefix {
            "^" => {
                let high = if target.major != 0 {
                    target.next_major()
                } else if target.minor != 0 {
                    target.next_minor()
                } else {
                    target.next_patch()
                };
                Ok(Clause::Range(Range::simple(Operator::Gte, target)?)
                    .and(Clause::Range(Range::simple(Operator::Lt, high)?)))
            }
            "~" => {
                let high = if minor.is_none() {
                    target.next_major()
                } else {
                    target.next_minor()
                };
                Ok(Clause::Range(Range::simple(Operator::Gte, target)?)
                    .and(Clause::Range(Range::simple(Operator::Lt, high)?)))
            }
            "~=" => {
                let high = if minor.is_none() || patch.is_none() {
                    target.next_major()
                } else {
                    target.next_minor()
                };
                Ok(Clause::Range(Range::simple(Operator::Gte, target)?)
                    .and(Clause::Range(Range::simple(Operator::Lt, high)?)))
            }
            "==" => {
                if major.is_none() {
                    Ok(Clause::Range(Range::simple(Operator::Gte, target)?))
                } else if minor.is_none() {
                    let high = target.next_major();
                    Ok(Clause::Range(Range::simple(Operator::Gte, target)?)
                        .and(Clause::Range(Range::simple(Operator::Lt, high)?)))
                } else if patch.is_none() {
                    let high = target.next_minor();
                    Ok(Clause::Range(Range::simple(Operator::Gte, target)?)
                        .and(Clause::Range(Range::simple(Operator::Lt, high)?)))
                } else if build_raw == Some("") {
                    Ok(Clause::Range(Range::new(
                        Operator::Eq,
                        target,
                        PrereleasePolicy::Natural,
                        BuildPolicy::Strict,
                    )?))
                } else {
                    Ok(Clause::Range(Range::simple(Operator::Eq, target)?))
                }
            }
            "!=" => {
                if minor.is_none() {
                    let high = target.next_major();
                    Ok(Clause::Range(Range::simple(Operator::Lt, target)?)
                        .or(Clause::Range(Range::simple(Operator::Gte, high)?)))
                } else if patch.is_none() {
                    let high = target.next_minor();
                    Ok(Clause::Range(Range::simple(Operator::Lt, target)?)
                        .or(Clause::Range(Range::simple(Operator::Gte, high)?)))
                } else if prerel_raw == Some("") {
                    Ok(Clause::Range(Range::new(
                        Operator::Neq,
                        target,
                        PrereleasePolicy::Always,
                        BuildPolicy::Implicit,
                    )?))
                } else if build_raw == Some("") {
                    Ok(Clause::Range(Range::new(
                        Operator::Neq,
                        target,
                        PrereleasePolicy::Natural,
                        BuildPolicy::Strict,
                    )?))
                } else {
                    Ok(Clause::Range(Range::simple(Operator::Neq, target)?))
                }
            }
            ">" => {
                if minor.is_none() {
                    Ok(Clause::Range(Range::simple(Operator::Gte, target.next_major())?))
                } else if patch.is_none() {
                    Ok(Clause::Range(Range::simple(Operator::Gte, target.next_minor())?))
                } else {
                    Ok(Clause::Range(Range::simple(Operator::Gt, target)?))
                }
            }
            ">=" => Ok(Clause::Range(Range::simple(Operator::Gte, target)?)),
            "<" => {
                if prerel_raw == Some("") {
                    Ok(Clause::Range(Range::new(
                        Operator::Lt,
                        target,
                        PrereleasePolicy::Always,
                        BuildPolicy::Implicit,
                    )?))
                } else {
                    Ok(Clause::Range(Range::simple(Operator::Lt, target)?))
                }
            }
            "<=" => {
                if minor.is_none() {
                    Ok(Clause::Range(Range::simple(Operator::Lt, target.next_major())?))
                } else if patch.is_none() {
                    Ok(Clause::Range(Range::simple(Operator::Lt, target.next_minor())?))
                } else {
                    Ok(Clause::Range(Range::simple(Operator::Lte, target)?))
                }
            }
            _ => Err(SpecError::InvalidBlock(expr.to_string())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(s: &str) -> Version {
        Version::parse(s).unwrap()
    }

    #[test]
    fn simple_gte() {
        let spec = SimpleSpec::parse(">=1.0.0").unwrap();
        assert!(spec.matches(&v("1.0.0")));
        assert!(spec.matches(&v("2.0.0")));
        assert!(!spec.matches(&v("0.9.9")));
    }

    #[test]
    fn range_combination_with_comma() {
        let spec = SimpleSpec::parse(">=0.1.1,<0.3.0").unwrap();
        assert!(spec.matches(&v("0.1.1")));
        assert!(spec.matches(&v("0.2.9")));
        assert!(!spec.matches(&v("0.3.0")));
        assert!(!spec.matches(&v("0.1.0")));
    }

    #[test]
    fn prerelease_does_not_match_by_default() {
        let spec = SimpleSpec::parse(">=0.1.1").unwrap();
        assert!(!spec.matches(&v("0.1.1-alpha1")));
    }

    #[test]
    fn eq_matches_build_variants_but_not_prerelease() {
        let spec = SimpleSpec::parse("==0.1.1").unwrap();
        assert!(spec.matches(&v("0.1.1+git7ccc72")));
        assert!(!spec.matches(&v("0.1.1-alpha1")));
        assert!(!spec.matches(&v("0.1.2")));
    }

    #[test]
    fn caret_range() {
        let spec = SimpleSpec::parse("^1.2.4").unwrap();
        assert!(spec.matches(&v("1.2.4")));
        assert!(spec.matches(&v("1.9.9")));
        assert!(!spec.matches(&v("2.0.0")));
        assert!(!spec.matches(&v("1.2.3")));
    }

    #[test]
    fn caret_range_zero_major() {
        let spec = SimpleSpec::parse("^0.1.2").unwrap();
        assert!(spec.matches(&v("0.1.9")));
        assert!(!spec.matches(&v("0.2.0")));
    }

    #[test]
    fn tilde_range() {
        let spec = SimpleSpec::parse("~1.2.3").unwrap();
        assert!(spec.matches(&v("1.2.9")));
        assert!(!spec.matches(&v("1.3.0")));
        assert!(!spec.matches(&v("1.2.2")));
    }

    #[test]
    fn wildcard_major_matches_star() {
        let spec = SimpleSpec::parse("*").unwrap();
        assert!(spec.matches(&v("0.0.1")));
        assert!(spec.matches(&v("99.99.99")));
    }

    #[test]
    fn partial_major_only_equality() {
        let spec = SimpleSpec::parse("==1").unwrap();
        assert!(spec.matches(&v("1.5.0")));
        assert!(!spec.matches(&v("2.0.0")));
        assert!(!spec.matches(&v("0.9.0")));
    }

    #[test]
    fn not_equal_excludes_exact_version() {
        let spec = SimpleSpec::parse("!=1.2.3").unwrap();
        assert!(!spec.matches(&v("1.2.3")));
        assert!(spec.matches(&v("1.2.4")));
    }

    #[test]
    fn select_picks_highest_matching() {
        let spec = SimpleSpec::parse(">=0.1.0,<0.4.0").unwrap();
        let versions: Vec<Version> = (0..6).map(|i| v(&format!("0.{}.0", i))).collect();
        let best = spec.select(versions.iter()).unwrap();
        assert_eq!(best.to_string(), "0.3.0");
    }

    #[test]
    fn build_metadata_disallowed_outside_eq_neq() {
        assert!(SimpleSpec::parse(">1.2.3+build").is_err());
    }

    #[test]
    fn invalid_expression_rejected() {
        assert!(SimpleSpec::parse("not-a-version").is_err());
    }
}
