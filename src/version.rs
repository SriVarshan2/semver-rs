//! Port of `Version` from python-semanticversion's `base.py`.
//!
//! Scope decision (see DECISIONS.md): the original supports a `partial=True`
//! mode for incomplete versions. We do not port partial-version support —
//! the original library itself deprecates it.

use once_cell::sync::Lazy;
use regex::Regex;
use std::cmp::Ordering;
use std::fmt;

static VERSION_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^(\d+)\.(\d+)\.(\d+)(?:-([0-9a-zA-Z.-]+))?(?:\+([0-9a-zA-Z.-]+))?$").unwrap()
});

static COERCE_BASE_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^\d+(?:\.\d+(?:\.\d+)?)?").unwrap());

static COERCE_CLEANUP_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"[^a-zA-Z0-9+.\-]").unwrap());

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VersionError {
    Empty,
    Invalid(String),
    LeadingZero(String),
    EmptyIdentifier,
    LeadingZeroIdentifier(String),
    MissingNumericComponent(String),
}

impl fmt::Display for VersionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            VersionError::Empty => write!(f, "Invalid empty version string"),
            VersionError::Invalid(s) => write!(f, "Invalid version string: {:?}", s),
            VersionError::LeadingZero(s) => write!(f, "Invalid leading zero in version: {:?}", s),
            VersionError::EmptyIdentifier => write!(f, "Invalid empty identifier"),
            VersionError::LeadingZeroIdentifier(s) => {
                write!(f, "Invalid leading zero in identifier {:?}", s)
            }
            VersionError::MissingNumericComponent(s) => write!(
                f,
                "Version string lacks a numerical component: {:?}",
                s
            ),
        }
    }
}

impl std::error::Error for VersionError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Identifier {
    Numeric(u64),
    Alpha(String),
}

impl Identifier {
    fn parse(s: &str) -> Self {
        if !s.is_empty() && s.chars().all(|c| c.is_ascii_digit()) {
            Identifier::Numeric(s.parse().unwrap())
        } else {
            Identifier::Alpha(s.to_string())
        }
    }
}

impl Ord for Identifier {
    fn cmp(&self, other: &Self) -> Ordering {
        use Identifier::*;
        match (self, other) {
            (Numeric(a), Numeric(b)) => a.cmp(b),
            (Alpha(a), Alpha(b)) => a.as_bytes().cmp(b.as_bytes()),
            (Numeric(_), Alpha(_)) => Ordering::Less,
            (Alpha(_), Numeric(_)) => Ordering::Greater,
        }
    }
}
impl PartialOrd for Identifier {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PrereleaseKey {
    Max,
    Identifiers(Vec<Identifier>),
}

impl Ord for PrereleaseKey {
    fn cmp(&self, other: &Self) -> Ordering {
        use PrereleaseKey::*;
        match (self, other) {
            (Max, Max) => Ordering::Equal,
            (Max, Identifiers(_)) => Ordering::Greater,
            (Identifiers(_), Max) => Ordering::Less,
            (Identifiers(a), Identifiers(b)) => a.cmp(b),
        }
    }
}
impl PartialOrd for PrereleaseKey {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

#[derive(Debug, Clone)]
pub struct Version {
    pub major: u64,
    pub minor: u64,
    pub patch: u64,
    pub prerelease: Vec<String>,
    pub build: Vec<String>,
}

fn has_leading_zero(s: &str) -> bool {
    !s.is_empty()
        && s.starts_with('0')
        && s.chars().all(|c| c.is_ascii_digit())
        && s != "0"
}

fn validate_identifiers(identifiers: &[String], allow_leading_zeroes: bool) -> Result<(), VersionError> {
    for item in identifiers {
        if item.is_empty() {
            return Err(VersionError::EmptyIdentifier);
        }
        let is_all_digit = item.chars().all(|c| c.is_ascii_digit());
        if item.starts_with('0') && is_all_digit && item != "0" && !allow_leading_zeroes {
            return Err(VersionError::LeadingZeroIdentifier(item.clone()));
        }
    }
    Ok(())
}

impl Version {
    pub fn parse(version_string: &str) -> Result<Self, VersionError> {
        if version_string.is_empty() {
            return Err(VersionError::Empty);
        }

        let caps = VERSION_RE
            .captures(version_string)
            .ok_or_else(|| VersionError::Invalid(version_string.to_string()))?;

        let major_s = &caps[1];
        let minor_s = &caps[2];
        let patch_s = &caps[3];

        if has_leading_zero(major_s) || has_leading_zero(minor_s) || has_leading_zero(patch_s) {
            return Err(VersionError::LeadingZero(version_string.to_string()));
        }

        let major: u64 = major_s.parse().unwrap();
        let minor: u64 = minor_s.parse().unwrap();
        let patch: u64 = patch_s.parse().unwrap();

        let prerelease: Vec<String> = match caps.get(4) {
            None => vec![],
            Some(m) if m.as_str().is_empty() => vec![],
            Some(m) => m.as_str().split('.').map(String::from).collect(),
        };
        validate_identifiers(&prerelease, false)?;

        let build: Vec<String> = match caps.get(5) {
            None => vec![],
            Some(m) if m.as_str().is_empty() => vec![],
            Some(m) => m.as_str().split('.').map(String::from).collect(),
        };
        validate_identifiers(&build, true)?;

        Ok(Version {
            major,
            minor,
            patch,
            prerelease,
            build,
        })
    }

    pub fn coerce(version_string: &str) -> Result<Self, VersionError> {
        let m = COERCE_BASE_RE
            .find(version_string)
            .ok_or_else(|| VersionError::MissingNumericComponent(version_string.to_string()))?;

        let mut version = version_string[..m.end()].to_string();

        while version.matches('.').count() < 2 {
            version.push_str(".0");
        }

        version = version
            .split('.')
            .map(|part| {
                let stripped = part.trim_start_matches('0');
                if stripped.is_empty() {
                    "0".to_string()
                } else {
                    stripped.to_string()
                }
            })
            .collect::<Vec<_>>()
            .join(".");

        if m.end() == version_string.len() {
            return Version::parse(&version);
        }

        let rest_raw = &version_string[m.end()..];
        let rest_cleaned = COERCE_CLEANUP_RE.replace_all(rest_raw, "-").to_string();

        let (prerelease, build): (String, String) = {
            let mut rest = rest_cleaned.as_str();
            if rest.starts_with('+') {
                (String::new(), rest[1..].to_string())
            } else if rest.starts_with('.') {
                (String::new(), rest[1..].to_string())
            } else if rest.starts_with('-') {
                rest = &rest[1..];
                if let Some(idx) = rest.find('+') {
                    (rest[..idx].to_string(), rest[idx + 1..].to_string())
                } else {
                    (rest.to_string(), String::new())
                }
            } else if let Some(idx) = rest.find('+') {
                (rest[..idx].to_string(), rest[idx + 1..].to_string())
            } else {
                (rest.to_string(), String::new())
            }
        };

        let build = build.replace('+', ".");

        if !prerelease.is_empty() {
            version = format!("{}-{}", version, prerelease);
        }
        if !build.is_empty() {
            version = format!("{}+{}", version, build);
        }

        Version::parse(&version)
    }

    pub fn next_major(&self) -> Version {
        if !self.prerelease.is_empty() && self.minor == 0 && self.patch == 0 {
            Version { major: self.major, minor: 0, patch: 0, prerelease: vec![], build: vec![] }
        } else {
            Version { major: self.major + 1, minor: 0, patch: 0, prerelease: vec![], build: vec![] }
        }
    }

    pub fn next_minor(&self) -> Version {
        if !self.prerelease.is_empty() && self.patch == 0 {
            Version { major: self.major, minor: self.minor, patch: 0, prerelease: vec![], build: vec![] }
        } else {
            Version { major: self.major, minor: self.minor + 1, patch: 0, prerelease: vec![], build: vec![] }
        }
    }

    pub fn next_patch(&self) -> Version {
        if !self.prerelease.is_empty() {
            Version { major: self.major, minor: self.minor, patch: self.patch, prerelease: vec![], build: vec![] }
        } else {
            Version { major: self.major, minor: self.minor, patch: self.patch + 1, prerelease: vec![], build: vec![] }
        }
    }

    pub fn truncate(&self) -> Version {
        Version { major: self.major, minor: self.minor, patch: self.patch, prerelease: vec![], build: vec![] }
    }

    fn prerelease_key(&self) -> PrereleaseKey {
        if self.prerelease.is_empty() {
            PrereleaseKey::Max
        } else {
            PrereleaseKey::Identifiers(self.prerelease.iter().map(|s| Identifier::parse(s)).collect())
        }
    }

    fn build_key(&self) -> Vec<Identifier> {
        self.build.iter().map(|s| Identifier::parse(s)).collect()
    }

    fn cmp_key(&self) -> (u64, u64, u64, PrereleaseKey) {
        (self.major, self.minor, self.patch, self.prerelease_key())
    }

    pub fn sort_key(&self) -> (u64, u64, u64, PrereleaseKey, Vec<Identifier>) {
        (self.major, self.minor, self.patch, self.prerelease_key(), self.build_key())
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)?;
        if !self.prerelease.is_empty() {
            write!(f, "-{}", self.prerelease.join("."))?;
        }
        if !self.build.is_empty() {
            write!(f, "+{}", self.build.join("."))?;
        }
        Ok(())
    }
}

impl PartialEq for Version {
    fn eq(&self, other: &Self) -> bool {
        self.major == other.major
            && self.minor == other.minor
            && self.patch == other.patch
            && self.prerelease == other.prerelease
            && self.build == other.build
    }
}
impl Eq for Version {}

impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp_key().cmp(&other.cmp_key()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_basic_version() {
        let v = Version::parse("1.2.3").unwrap();
        assert_eq!((v.major, v.minor, v.patch), (1, 2, 3));
        assert!(v.prerelease.is_empty());
        assert!(v.build.is_empty());
    }

    #[test]
    fn rejects_incomplete_version() {
        assert!(Version::parse("0.1").is_err());
    }

    #[test]
    fn rejects_leading_zero() {
        assert!(Version::parse("01.2.3").is_err());
    }

    #[test]
    fn parses_prerelease_and_build() {
        let v = Version::parse("0.1.1-alpha+build.2012-05-15").unwrap();
        assert_eq!(v.prerelease, vec!["alpha"]);
        assert_eq!(v.build, vec!["build", "2012-05-15"]);
    }

    #[test]
    fn release_outranks_prerelease() {
        let release = Version::parse("0.1.1").unwrap();
        let pre = Version::parse("0.1.1-alpha").unwrap();
        assert!(release > pre);
    }

    #[test]
    fn prerelease_lt_or_eq_ignores_build() {
        let a = Version::parse("0.1.1+build1").unwrap();
        let b = Version::parse("0.1.1+build2").unwrap();
        assert_ne!(a, b);
        assert!(!(a < b) && !(a > b));
    }

    #[test]
    fn numeric_prerelease_identifiers_compare_numerically() {
        let a = Version::parse("1.0.0-alpha.2").unwrap();
        let b = Version::parse("1.0.0-alpha.10").unwrap();
        assert!(a < b);
    }

    #[test]
    fn numeric_identifiers_sort_before_alpha() {
        let a = Version::parse("1.0.0-1").unwrap();
        let b = Version::parse("1.0.0-alpha").unwrap();
        assert!(a < b);
    }

    #[test]
    fn coerce_pads_missing_components() {
        let v = Version::coerce("0.1").unwrap();
        assert_eq!((v.major, v.minor, v.patch), (0, 1, 0));
    }

    #[test]
    fn coerce_extra_components_become_build() {
        let v = Version::coerce("0.1.2.3").unwrap();
        assert_eq!((v.major, v.minor, v.patch), (0, 1, 2));
        assert_eq!(v.build, vec!["3"]);
    }

    #[test]
    fn next_major_from_prerelease_zero_zero() {
        let v = Version::parse("1.0.0-alpha").unwrap();
        let n = v.next_major();
        assert_eq!((n.major, n.minor, n.patch), (1, 0, 0));
    }

    #[test]
    fn display_round_trips() {
        let s = "1.2.3-alpha.1+build.5";
        let v = Version::parse(s).unwrap();
        assert_eq!(v.to_string(), s);
    }
}
