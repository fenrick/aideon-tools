use std::path::PathBuf;

use oxigraph::model::{BlankNodeIdParseError, IriParseError};
use thiserror::Error;

/// Convenient alias for fallible results returned throughout the crate.
pub type Result<T> = std::result::Result<T, ToolError>;

/// Error type covering the different failure cases that can occur when the
/// tool ingests, transforms, or emits data.
#[derive(Debug, Error)]
pub enum ToolError {
    /// Wrapper for IO failures such as reading or writing files.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// Raised when JSON parsing or serialization fails.
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    /// Errors bubbled up from the Excel writer implementation.
    #[error("Excel write error: {0}")]
    ExcelWrite(#[from] rust_xlsxwriter::XlsxError),

    /// Errors bubbled up from the Excel reader implementation.
    #[error("Excel read error: {0}")]
    ExcelRead(#[from] calamine::XlsxError),

    /// Raised when a sheet does not follow the expected conventions.
    #[error("invalid workbook structure: {0}")]
    InvalidWorkbook(String),

    /// Raised when JSON-LD could not be normalized into the internal model.
    #[error("JSON-LD normalization error: {0}")]
    JsonLd(String),

    /// Raised when RDF parsing or serialization fails.
    #[error("RDF error: {0}")]
    Rdf(String),

    /// Raised when an invalid IRI is encountered while building RDF nodes.
    #[error("invalid IRI: {0}")]
    InvalidIri(#[from] IriParseError),

    /// Raised when a blank node identifier fails validation.
    #[error("invalid blank node identifier: {0}")]
    InvalidBlankNode(#[from] BlankNodeIdParseError),

    /// Raised when the CLI receives an unsupported conversion request.
    #[error("unsupported conversion from {from:?} to {to:?}")]
    UnsupportedConversion { from: String, to: String },

    /// Raised when a required sheet or mapping entry is missing.
    #[error("missing metadata entry for sheet {0}")]
    MissingMetadata(String),

    /// Raised when numeric parsing fails when rebuilding typed values.
    #[error("invalid literal value '{value}' in column {column}")]
    InvalidLiteral { column: String, value: String },

    /// Raised when the user provides a path that does not exist.
    #[error("input file not found: {0}")]
    MissingInput(PathBuf),

    /// Raised when the tracing subscriber fails to initialise.
    #[error("failed to initialise logging: {0}")]
    Logging(String),
}
