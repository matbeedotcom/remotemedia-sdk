//! Universal Manifest Testing Framework for RemoteMedia SDK
//!
//! Takes a pipeline manifest, analyzes it, generates synthetic test data,
//! runs it through pluggable probe backends, and produces a structured report.

pub mod prerequisites;
pub mod probes;
pub mod report;
pub mod synthetic_data;
pub mod tester;

pub use report::{
    CategorizedError, ErrorCategory, ManifestTestReport, NodeResult, NodeStatus, ProbeResult,
    TestStatus,
};
pub use tester::ManifestTester;
