pub mod version;
pub mod spec;
pub use version::{Version, VersionError};
pub use spec::{SimpleSpec, SpecError};
pub mod npm_spec;
pub use npm_spec::NpmSpec;
