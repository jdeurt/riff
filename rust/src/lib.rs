//! Shared schema compilation, generation, mutation, and minimization for Riff.
mod cases;
mod cli;
mod contracts;
mod documents;
mod engine;
mod error;
mod factory;
mod limits;
mod location;
mod metadata;
mod minimize;
mod mutations;
mod node;
mod openapi;
mod providers;
mod references;
mod scalars;
mod schema;
mod session;

#[doc(inline)]
pub use cases::{Case, Violation};
#[doc(inline)]
pub use cli::{CliOutput, run_cli};
#[doc(inline)]
pub use documents::{Document, NumericMode, parse_document, read_document};
#[doc(inline)]
pub use engine::{GenerationOptions, Profile, ProviderContext, ProviderKind};
#[doc(inline)]
pub use error::{Error, Result};
#[doc(inline)]
pub use factory::Factory;
#[doc(inline)]
pub use limits::{MAX_INPUT_DEPTH, MAX_INTEGER_BITS};
#[doc(inline)]
pub use metadata::{ConstraintMode, Metadata, Source, Strategy};
#[doc(inline)]
pub use minimize::MinimizationResult;
#[doc(inline)]
pub use schema::{Body, Dialect, Operation, prepare, strip_metadata};
#[doc(inline)]
pub use session::{Session, validate_options};
