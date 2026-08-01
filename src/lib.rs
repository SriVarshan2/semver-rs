//! semver-rs: a Rust port of rbarrois/python-semanticversion
//!
//! Port_Mortem 2026 — Track D (Python -> Rust)
//! Source: https://github.com/rbarrois/python-semanticversion
//!
//! Port order (see DECISIONS.md):
//! 1. Version (parsing + comparison)
//! 2. Version::coerce()
//! 3. SimpleSpec
//! 4. NpmSpec

// TODO: pub mod version;
// TODO: pub mod spec;
// TODO: pub mod npm_spec;

#[cfg(test)]
mod tests {
    #[test]
    fn scaffold_builds() {
        assert!(true);
    }
}
