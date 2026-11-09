//! Shared schema compilation, generation, mutation, and minimization for Riff.
mod cases;
mod engine;
mod error;
mod factory;
mod limits;
mod metadata;
mod minimize;
mod mutations;
mod node;
mod scalars;
mod schema;

pub use cases::{Case, Violation};
pub use engine::{GenerationOptions, Profile, ProviderContext, ProviderKind};
pub use error::{Error, Result};
pub use factory::Factory;
pub use metadata::{ConstraintMode, Metadata, Source, Strategy};
pub use minimize::MinimizationResult;
pub use schema::{Body, Dialect, Operation, prepare, strip_metadata};
