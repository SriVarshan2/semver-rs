//! Port of the legacy `SpecItem` low-level building block from
//! python-semanticversion's `base.py`. Kept separate from `spec.rs`'s
//! Clause/Range engine since SpecItem is a single-clause, non-combinator
//! API kept in the original library for backward compatibility.

use crate::version::Version;
use once_cell::sync::Lazy;
use regex::Regex;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Kind {
    Any,
    Lt,
    Lte,
    Equal,
    Gte,
    Gt,
    Neq,
    Caret,
    Tilde,
    TildeEq,
}

impl Kind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Kind::Any => "*",
            Kind::Lt => "<",
            Kind::Lte => "<=",
            Kind::Equal => "==",
            Kind::Gte => ">=",
            Kind::Gt => ">",
            Kind::Neq => "!=",
            Kind::Caret => "^",
            Kind::Tilde => "~",
            Kind::TildeEq => "~=",
        }
    }
}

#[derive(Debug, Clone)]
pub struct SpecItem {
    pub kind: Kind,
    pub major: u64,
    pub minor: Option<u64>,
    pub patch: Option<u64>,
    /// None = not mentioned in the pattern; Some(vec) = explicitly given (may be empty)
    pub prerelease: Option<Vec<String>>,
    /// None = not mentioned; Some(vec) = explicitly given (may be empty)
    pub build: Option<Vec<String>>,
    pub expression: String,
}

static ITEM_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"(?x)^
        (?P<op><=|>=|==|!=|~=|<|>|=|\^|~|)
        (?P<major>\d+)
        (?:\.(?P<minor>\d+)
            (?:\.(?P<patch>\d+))?
        )?
        (?:-(?P<prerel>[a-zA-Z0-9.-]*))?
        (?:\+(?P<build>[a-zA-Z0-9.-]*))?
        $",
    )
    .unwrap()
});

impl SpecItem {
    pub fn parse(expr: &str) -> Result<Self, String> {
        if expr.trim() == "*" {
            return Ok(SpecItem {
                kind: Kind::Any,
                major: 0,
                minor: None,
                patch: None,
                prerelease: None,
                build: None,
                expression: expr.to_string(),
            });
        }
        let caps = ITEM_RE
            .captures(expr)
            .ok_or_else(|| format!("Invalid SpecItem: {:?}", expr))?;

        let raw_op = caps.name("op").map(|m| m.as_str()).unwrap_or("");
        let kind = match raw_op {
            "" | "=" | "==" => Kind::Equal,
            "<" => Kind::Lt,
            "<=" => Kind::Lte,
            ">" => Kind::Gt,
            ">=" => Kind::Gte,
            "!=" => Kind::Neq,
            "^" => Kind::Caret,
            "~" => Kind::Tilde,
            "~=" => Kind::TildeEq,
            other => return Err(format!("Unknown operator {:?} in {:?}", other, expr)),
        };

        let major: u64 = caps["major"].parse().map_err(|_| format!("Bad major in {:?}", expr))?;
        let minor: Option<u64> = caps.name("minor").map(|m| m.as_str().parse().unwrap());
        let patch: Option<u64> = caps.name("patch").map(|m| m.as_str().parse().unwrap());

        let mut prerelease: Option<Vec<String>> = caps.name("prerel").map(|m| {
            if m.as_str().is_empty() {
                vec![]
            } else {
                m.as_str().split('.').map(String::from).collect()
            }
        });
        let build: Option<Vec<String>> = caps.name("build").map(|m| {
            if m.as_str().is_empty() {
                vec![]
            } else {
                m.as_str().split('.').map(String::from).collect()
            }
        });

        // Explicit build metadata commits the comparator to exact
        // semantics, which implicitly pins prerelease to "none" (empty)
        // rather than leaving it unconstrained.
        if build.is_some() && prerelease.is_none() {
            prerelease = Some(vec![]);
        }

        if build.is_some() && !matches!(kind, Kind::Equal | Kind::Neq) {
            return Err(format!(
                "Invalid SpecItem {:?}: build numbers have no ordering, only valid with ==/!=",
                expr
            ));
        }

        Ok(SpecItem {
            kind,
            major,
            minor,
            patch,
            prerelease,
            build,
            expression: expr.to_string(),
        })
    }

    /// True when this comparator has no explicit prerelease of its own,
    /// the version being checked does have a prerelease, and they share
    /// the same major.minor.patch tuple. Applies to Lt and Neq, where
    /// natural ordering/equality alone would otherwise incorrectly admit
    /// the version.
    fn same_tuple_no_prerelease(&self, version: &Version) -> bool {
        if self.prerelease.is_some() {
            return false;
        }
        if version.prerelease.is_empty() {
            return false;
        }
        self.major == version.major
            && self.minor.unwrap_or(0) == version.minor
            && self.patch.unwrap_or(0) == version.patch
    }

    fn target_version(&self) -> Version {
        Version {
            major: self.major,
            minor: self.minor.unwrap_or(0),
            patch: self.patch.unwrap_or(0),
            prerelease: self.prerelease.clone().unwrap_or_default(),
            build: self.build.clone().unwrap_or_default(),
        }
    }

    pub fn matches(&self, version: &Version) -> bool {
        let target = self.target_version();
        match self.kind {
            Kind::Any => true,
            Kind::Equal => {
                if self.build.is_some() {
                    version == &target
                } else {
                    let v_no_build = Version { build: vec![], ..version.clone() };
                    let t_no_build = Version { build: vec![], ..target.clone() };
                    v_no_build == t_no_build
                }
            }
            Kind::Neq => {
                if self.same_tuple_no_prerelease(version) {
                    false
                } else if self.build.is_some() {
                    version != &target
                } else {
                    let v_no_build = Version { build: vec![], ..version.clone() };
                    let t_no_build = Version { build: vec![], ..target.clone() };
                    v_no_build != t_no_build
                }
            }
            Kind::Lt => !self.same_tuple_no_prerelease(version) && version < &target,
            Kind::Lte => version <= &target,
            Kind::Gt => version > &target,
            Kind::Gte => version >= &target,
            Kind::Caret => {
                let high = if self.major != 0 {
                    target.next_major()
                } else if self.minor.unwrap_or(0) != 0 {
                    target.next_minor()
                } else {
                    target.next_patch()
                };
                *version >= target && *version < high
            }
            Kind::Tilde => {
                let high = if self.minor.is_none() {
                    target.next_major()
                } else {
                    target.next_minor()
                };
                *version >= target && *version < high
            }
            Kind::TildeEq => {
                let high = if self.patch.is_none() {
                    target.next_major()
                } else {
                    target.next_minor()
                };
                *version >= target && *version < high
            }
        }
    }

    pub fn to_display_string(&self) -> String {
        if self.kind == Kind::Any {
            return "*".to_string();
        }
        let mut s = format!("{}{}", self.kind.as_str(), self.major);
        if let Some(m) = self.minor {
            s.push_str(&format!(".{}", m));
        }
        if let Some(p) = self.patch {
            s.push_str(&format!(".{}", p));
        }
        if let Some(pre) = &self.prerelease {
            if !pre.is_empty() {
                s.push('-');
                s.push_str(&pre.join("."));
            }
        }
        if let Some(b) = &self.build {
            s.push('+');
            s.push_str(&b.join("."));
        }
        s
    }
}

impl PartialEq for SpecItem {
    fn eq(&self, other: &Self) -> bool {
        self.kind == other.kind
            && self.major == other.major
            && self.minor == other.minor
            && self.patch == other.patch
            && self.prerelease == other.prerelease
            && self.build == other.build
    }
}
impl Eq for SpecItem {}

impl std::hash::Hash for SpecItem {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.expression.hash(state);
    }
}
