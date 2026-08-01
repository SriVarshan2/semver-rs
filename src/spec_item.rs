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
        (?P<op><=|>=|==|!=|<|>|=|\^|~|)
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
            other => return Err(format!("Unknown operator {:?} in {:?}", other, expr)),
        };

        let major: u64 = caps["major"].parse().map_err(|_| format!("Bad major in {:?}", expr))?;
        let minor: Option<u64> = caps.name("minor").map(|m| m.as_str().parse().unwrap());
        let patch: Option<u64> = caps.name("patch").map(|m| m.as_str().parse().unwrap());

        let prerelease: Option<Vec<String>> = caps.name("prerel").map(|m| {
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
            Kind::Equal => version == &target,
            Kind::Neq => version != &target,
            Kind::Lt => version < &target,
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
        }
    }

    pub fn to_display_string(&self) -> String {
        let mut s = format!("{}{}", self.kind.as_str(), self.major);
        if let Some(m) = self.minor {
            s.push_str(&format!(".{}", m));
        }
        if let Some(p) = self.patch {
            s.push_str(&format!(".{}", p));
        }
        if let Some(pre) = &self.prerelease {
            s.push('-');
            s.push_str(&pre.join("."));
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
