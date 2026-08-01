//! Port of `NpmSpec` from python-semanticversion's `npm_spec.py`.
//! Reuses Clause/Range from spec.rs; only the parsing grammar differs from SimpleSpec.

use crate::spec::{Clause, Operator, Range, SpecError};
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
        // Top-level OR on '||'
        let mut clause = Clause::Never;
        for block in expression.split("||") {
            let block = block.trim();
            let block_clause = parse_range_set(block)?;
            clause = clause.or(block_clause);
        }
        Ok(NpmSpec { expression: expression.to_string(), clause })
    }

    pub fn matches(&self, version: &Version) -> bool {
        self.clause.matches(version)
    }

    pub fn select<'a>(&self, versions: impl IntoIterator<Item = &'a Version>) -> Option<&'a Version> {
        versions.into_iter().filter(|v| self.matches(v))
            .fold(None, |best, v| match best {
                None => Some(v),
                Some(b) if v > b => Some(v),
                Some(b) => Some(b),
            })
    }
}

static COMPARATOR_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^(?P<op><=|>=|<|>|=|)\s*(?P<ver>.+)$").unwrap()
});

static PART_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)^(?P<maj>[xX*]|\d+)(?:\.(?P<min>[xX*]|\d+))?(?:\.(?P<pat>[xX*]|\d+))?(?:-(?P<pre>[0-9A-Za-z.-]+))?(?:\+(?P<build>[0-9A-Za-z.-]+))?$").unwrap()
});

fn is_wild(s: Option<&str>) -> bool {
    matches!(s, None | Some("x") | Some("X") | Some("*"))
}

fn parse_partial(s: &str) -> Result<(Option<u64>, Option<u64>, Option<u64>, Vec<String>, Vec<String>), SpecError> {
    let caps = PART_RE.captures(s.trim()).ok_or_else(|| SpecError::InvalidBlock(s.to_string()))?;
    let maj = caps.name("maj").map(|m| m.as_str());
    let min = caps.name("min").map(|m| m.as_str());
    let pat = caps.name("pat").map(|m| m.as_str());
    let major = if is_wild(maj) { None } else { Some(maj.unwrap().parse().unwrap()) };
    let minor = if is_wild(min) { None } else { min.map(|x| x.parse().unwrap()) };
    let patch = if is_wild(pat) { None } else { pat.map(|x| x.parse().unwrap()) };
    let pre = caps.name("pre").map(|m| m.as_str().split('.').map(String::from).collect()).unwrap_or_default();
    let build = caps.name("build").map(|m| m.as_str().split('.').map(String::from).collect()).unwrap_or_default();
    Ok((major, minor, patch, pre, build))
}

fn full_version(maj: u64, min: u64, pat: u64, pre: Vec<String>, build: Vec<String>) -> Version {
    Version { major: maj, minor: min, patch: pat, prerelease: pre, build }
}

/// Handles a hyphen range "1.2.3 - 2.3.4" or a space-separated set of comparators.
fn parse_range_set(block: &str) -> Result<Clause, SpecError> {
    if block.is_empty() { return Ok(Clause::Always); }

    if let Some(idx) = block.find(" - ") {
        let (low, high) = block.split_at(idx);
        let high = &high[3..];
        let (lmaj, lmin, lpat, _, _) = parse_partial(low.trim())?;
        let low_v = full_version(lmaj.unwrap_or(0), lmin.unwrap_or(0), lpat.unwrap_or(0), vec![], vec![]);
        let (hmaj, hmin, hpat, _, _) = parse_partial(high.trim())?;
        let low_clause = Clause::Range(Range::simple(Operator::Gte, low_v)?);
        let high_clause = if hmin.is_none() {
            Clause::Range(Range::simple(Operator::Lt, full_version(hmaj.unwrap() + 1, 0, 0, vec![], vec![]))?)
        } else if hpat.is_none() {
            Clause::Range(Range::simple(Operator::Lt, full_version(hmaj.unwrap(), hmin.unwrap() + 1, 0, vec![], vec![]))?)
        } else {
            Clause::Range(Range::simple(Operator::Lte, full_version(hmaj.unwrap(), hmin.unwrap(), hpat.unwrap(), vec![], vec![]))?)
        };
        return Ok(low_clause.and(high_clause));
    }

    let mut clause = Clause::Always;
    for comp in block.split_whitespace() {
        clause = clause.and(parse_comparator(comp)?);
    }
    Ok(clause)
}

fn parse_comparator(comp: &str) -> Result<Clause, SpecError> {
    let caps = COMPARATOR_RE.captures(comp).ok_or_else(|| SpecError::InvalidBlock(comp.to_string()))?;
    let op = caps.name("op").map(|m| m.as_str()).unwrap_or("");
    let ver_str = caps.name("ver").unwrap().as_str();

    if let Some(rest) = ver_str.strip_prefix('^') {
        let (maj, min, pat, pre, build) = parse_partial(rest)?;
        let target = full_version(maj.unwrap_or(0), min.unwrap_or(0), pat.unwrap_or(0), pre, build);
        let high = if target.major != 0 { target.next_major() }
                   else if min.is_some() && target.minor != 0 { target.next_minor() }
                   else if min.is_none() { target.next_major() }
                   else { target.next_patch() };
        return Ok(Clause::Range(Range::simple(Operator::Gte, target)?)
            .and(Clause::Range(Range::simple(Operator::Lt, high)?)));
    }
    if let Some(rest) = ver_str.strip_prefix('~') {
        let (maj, min, pat, pre, build) = parse_partial(rest)?;
        let target = full_version(maj.unwrap_or(0), min.unwrap_or(0), pat.unwrap_or(0), pre, build);
        let high = if min.is_none() { target.next_major() } else { target.next_minor() };
        return Ok(Clause::Range(Range::simple(Operator::Gte, target)?)
            .and(Clause::Range(Range::simple(Operator::Lt, high)?)));
    }

    let (maj, min, pat, pre, build) = parse_partial(ver_str)?;
    match (op, maj, min, pat) {
        ("", None, _, _) => Ok(Clause::Always), // bare "*"
        ("=" | "", Some(ma), None, _) => {
            let lo = full_version(ma, 0, 0, vec![], vec![]);
            Ok(Clause::Range(Range::simple(Operator::Gte, lo)?)
                .and(Clause::Range(Range::simple(Operator::Lt, lo.next_major())?)))
        }
        ("=" | "", Some(ma), Some(mi), None) => {
            let lo = full_version(ma, mi, 0, vec![], vec![]);
            Ok(Clause::Range(Range::simple(Operator::Gte, lo)?)
                .and(Clause::Range(Range::simple(Operator::Lt, lo.next_minor())?)))
        }
        ("=" | "", Some(ma), Some(mi), Some(pa)) => {
            Ok(Clause::Range(Range::simple(Operator::Eq, full_version(ma, mi, pa, pre, build))?))
        }
        (">", Some(ma), min, pat) => {
            let base = full_version(ma, min.unwrap_or(0), pat.unwrap_or(0), vec![], vec![]);
            let floor = if min.is_none() { base.next_major() } else if pat.is_none() { base.next_minor() } else { base.next_patch() };
            Ok(Clause::Range(Range::simple(Operator::Gte, floor)?))
        }
        (">=", Some(ma), min, pat) => {
            Ok(Clause::Range(Range::simple(Operator::Gte, full_version(ma, min.unwrap_or(0), pat.unwrap_or(0), pre, build))?))
        }
        ("<", Some(ma), min, pat) => {
            Ok(Clause::Range(Range::simple(Operator::Lt, full_version(ma, min.unwrap_or(0), pat.unwrap_or(0), pre, build))?))
        }
        ("<=", Some(ma), min, pat) => {
            let base = full_version(ma, min.unwrap_or(0), pat.unwrap_or(0), vec![], vec![]);
            let ceil = if min.is_none() { base.next_major() } else if pat.is_none() { base.next_minor() } else { return Ok(Clause::Range(Range::simple(Operator::Lte, base)?)); };
            Ok(Clause::Range(Range::simple(Operator::Lt, ceil)?))
        }
        _ => Err(SpecError::InvalidBlock(comp.to_string())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn v(s: &str) -> Version { Version::parse(s).unwrap() }

    #[test]
    fn caret_range() {
        let s = NpmSpec::parse("^1.2.3").unwrap();
        assert!(s.matches(&v("1.9.0")));
        assert!(!s.matches(&v("2.0.0")));
    }

    #[test]
    fn hyphen_range() {
        let s = NpmSpec::parse("1.2.3 - 2.3.4").unwrap();
        assert!(s.matches(&v("2.3.4")));
        assert!(!s.matches(&v("2.3.5")));
    }

    #[test]
    fn or_ranges() {
        let s = NpmSpec::parse("1.2.7 || >=1.2.9 <2.0.0").unwrap();
        assert!(s.matches(&v("1.2.7")));
        assert!(s.matches(&v("1.2.9")));
        assert!(!s.matches(&v("1.2.8")));
    }

    #[test]
    fn x_range_minor() {
        let s = NpmSpec::parse("1.x").unwrap();
        assert!(s.matches(&v("1.5.9")));
        assert!(!s.matches(&v("2.0.0")));
    }
}
