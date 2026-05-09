//! Lightweight Python source summarization for docstring-oriented tooling.
//!
//! This module is not a full Python parser. It extracts the byte ranges and
//! simple function facts needed by docstring linters and autofixers while
//! keeping the Rust core dependency-free.

mod range;
mod scanner;
mod string;
mod summary;

pub use range::ByteRange;
pub use summary::{ClassItem, FileSummary, FunctionItem, Item, ParameterRecord, RaiseRecord};

/// Summarizes Python source into docstring hosts and lightweight facts.
///
/// The scanner is intentionally lenient: malformed Python source should not
/// panic. Invalid or incomplete constructs are skipped or represented with the
/// ranges that could be recovered.
pub fn summarize_python(source: &str) -> FileSummary {
    scanner::Scanner::new(source).scan()
}
