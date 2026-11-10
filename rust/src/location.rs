//! Keep schema pointers distinct from encoded URI references.
use percent_encoding::{AsciiSet, NON_ALPHANUMERIC, percent_decode_str, utf8_percent_encode};
use serde_json::Value;

use crate::{Error, Result};

const FRAGMENT: &AsciiSet = &NON_ALPHANUMERIC.remove(b'/').remove(b'~');

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum SchemaLocation {
    Document(String),
    Resource { uri: String, pointer: String },
}

impl SchemaLocation {
    pub const fn root() -> Self {
        Self::Document(String::new())
    }

    pub fn from_reference(reference: &str) -> Result<Self> {
        let (uri, fragment) = reference.split_once('#').unwrap_or((reference, ""));
        let pointer = percent_decode_str(fragment)
            .decode_utf8()
            .map_err(|error| Error::Schema(error.to_string()))?
            .into_owned();
        Ok(if uri.is_empty() {
            Self::Document(pointer)
        } else {
            Self::Resource { uri: uri.to_owned(), pointer }
        })
    }

    pub fn child(&self, location: &str) -> Self {
        match self {
            Self::Document(pointer) => Self::Document(format!("{pointer}{location}")),
            Self::Resource { uri, pointer } => {
                Self::Resource { uri: uri.clone(), pointer: format!("{pointer}{location}") }
            }
        }
    }

    pub fn resolve<'a>(&self, document: &'a Value) -> Result<&'a Value> {
        let (resource, pointer) = match self {
            Self::Document(pointer) => (document, pointer),
            Self::Resource { uri, pointer } => {
                (crate::schema::resolve_reference(document, uri)?, pointer)
            }
        };
        resource
            .pointer(pointer)
            .ok_or_else(|| Error::Schema(format!("Unresolved schema pointer: {pointer}")))
    }

    pub fn reference(&self, document_uri: &str) -> String {
        let (uri, pointer) = match self {
            Self::Document(pointer) => (document_uri, pointer),
            Self::Resource { uri, pointer } => (uri.as_str(), pointer),
        };
        format!("{uri}#{}", utf8_percent_encode(pointer, FRAGMENT))
    }
}
