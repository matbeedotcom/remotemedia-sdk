//! ManifestTester — orchestrator that runs probes and collects results

use crate::prerequisites::PrerequisiteCheck;
use crate::probes::direct::DirectProbe;
use crate::probes::{ProbeBackend, ProbeContext, ProbeSpec};
use crate::report::{
    CategorizedError, ErrorCategory, ManifestTestReport, ProbeResult, TestStatus,
};
use crate::synthetic_data::SyntheticDataFactory;
use remotemedia_core::manifest::Manifest;
use remotemedia_manifest_analyzer::{self as analyzer, AnalyzerError};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tracing::info;

/// Builder for configuring and running manifest tests
pub struct ManifestTester {
    manifest_path: PathBuf,
    probes: Vec<ProbeSpec>,
    timeout: Duration,
    skip_ml: bool,
    dry_run: bool,
}

impl ManifestTester {
    /// Create a new tester for the given manifest path
    pub fn test(path: impl Into<PathBuf>) -> Self {
        Self {
            manifest_path: path.into(),
            probes: vec![ProbeSpec::Direct],
            timeout: Duration::from_secs(30),
            skip_ml: false,
            dry_run: false,
        }
    }

    /// Set which probes to run
    pub fn with_probes(mut self, probes: &[ProbeSpec]) -> Self {
        self.probes = probes.to_vec();
        self
    }

    /// Set timeout for each probe
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Skip ML nodes (replace with passthrough stubs)
    pub fn skip_ml(mut self, skip: bool) -> Self {
        self.skip_ml = skip;
        self
    }

    /// Only show what would be tested, don't execute
    pub fn dry_run(mut self, dry: bool) -> Self {
        self.dry_run = dry;
        self
    }

    /// Run the test suite
    pub async fn run(self) -> ManifestTestReport {
        let start = Instant::now();
        let manifest_name = self
            .manifest_path
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "unknown".to_string());

        let mut report = ManifestTestReport::new(self.manifest_path.clone(), manifest_name);

        // Step 1: Analyze the manifest
        let analysis = match analyzer::analyze_file(&self.manifest_path) {
            Ok(a) => a,
            Err(e) => {
                report.errors.push(CategorizedError {
                    category: match &e {
                        AnalyzerError::Parse(_) => ErrorCategory::ManifestParse,
                        AnalyzerError::InvalidManifest(_) => ErrorCategory::ManifestValidation,
                        AnalyzerError::Io(_) => ErrorCategory::ManifestParse,
                    },
                    node_id: None,
                    message: e.to_string(),
                    source: None,
                });
                report.overall_status = TestStatus::Fail;
                report.duration = start.elapsed();
                return report;
            }
        };

        info!(
            "Analyzed manifest: type={:?}, mode={:?}, transports={:?}, ml_reqs={}",
            analysis.pipeline_type,
            analysis.execution_mode,
            analysis.applicable_transports,
            analysis.ml_requirements.len()
        );

        // Step 2: Check prerequisites
        let prereqs = PrerequisiteCheck::check(&analysis.ml_requirements);
        if !prereqs.all_met() && !self.skip_ml {
            for missing in &prereqs.missing_prerequisites {
                report.errors.push(CategorizedError {
                    category: ErrorCategory::Prerequisite,
                    node_id: Some(missing.node_id.clone()),
                    message: missing.message.clone(),
                    source: None,
                });
            }
        }

        // Step 3: Generate synthetic data
        let test_data: Vec<_> = analysis
            .source_input_types
            .iter()
            .flat_map(|t| SyntheticDataFactory::generate(t))
            .collect();

        info!("Generated {} synthetic test data items", test_data.len());

        // Step 4: If dry-run, show test plan and return
        if self.dry_run {
            info!("Dry run — would test with probes: {:?}", self.probes);
            report.overall_status = TestStatus::Skipped;
            report.duration = start.elapsed();
            return report;
        }

        // Step 5: Load manifest for probes (reuses analyzer's camelCase-aware parser)
        let manifest: Manifest = match analyzer::load_manifest(&self.manifest_path) {
            Ok(m) => m,
            Err(e) => {
                report.errors.push(CategorizedError {
                    category: ErrorCategory::ManifestParse,
                    node_id: None,
                    message: format!("Failed to load manifest: {e}"),
                    source: None,
                });
                report.overall_status = TestStatus::Fail;
                report.duration = start.elapsed();
                return report;
            }
        };

        let ctx = ProbeContext {
            manifest: Arc::new(manifest),
            analysis: Arc::new(analysis),
            test_data,
            timeout: self.timeout,
            skip_ml: self.skip_ml,
        };

        // Step 6: Run probes sequentially
        for spec in &self.probes {
            let probe: Box<dyn ProbeBackend> = match spec {
                ProbeSpec::Direct => Box::new(DirectProbe),
                ProbeSpec::Grpc { .. } => {
                    #[cfg(feature = "probe-grpc")]
                    {
                        // TODO: implement GrpcProbe
                        report.probe_results.push(ProbeResult {
                            transport: "grpc".to_string(),
                            status: TestStatus::Skipped,
                            latency_ms: None,
                            first_output_ms: None,
                            errors: vec![CategorizedError {
                                category: ErrorCategory::Transport,
                                node_id: None,
                                message: "gRPC probe not yet implemented".to_string(),
                                source: None,
                            }],
                        });
                        continue;
                    }
                    #[cfg(not(feature = "probe-grpc"))]
                    {
                        report.probe_results.push(ProbeResult {
                            transport: "grpc".to_string(),
                            status: TestStatus::Skipped,
                            latency_ms: None,
                            first_output_ms: None,
                            errors: vec![CategorizedError {
                                category: ErrorCategory::Transport,
                                node_id: None,
                                message: "gRPC probe not compiled (enable probe-grpc feature)"
                                    .to_string(),
                                source: None,
                            }],
                        });
                        continue;
                    }
                }
                ProbeSpec::WebRtc { .. } => {
                    report.probe_results.push(ProbeResult {
                        transport: "webrtc".to_string(),
                        status: TestStatus::Skipped,
                        latency_ms: None,
                        first_output_ms: None,
                        errors: vec![CategorizedError {
                            category: ErrorCategory::Transport,
                            node_id: None,
                            message: "WebRTC probe not yet implemented".to_string(),
                            source: None,
                        }],
                    });
                    continue;
                }
                ProbeSpec::Http { .. } => {
                    report.probe_results.push(ProbeResult {
                        transport: "http".to_string(),
                        status: TestStatus::Skipped,
                        latency_ms: None,
                        first_output_ms: None,
                        errors: vec![CategorizedError {
                            category: ErrorCategory::Transport,
                            node_id: None,
                            message: "HTTP probe not yet implemented".to_string(),
                            source: None,
                        }],
                    });
                    continue;
                }
            };

            let result = probe.probe(&ctx).await;
            report.probe_results.push(result);
        }

        // Step 7: Finalize report
        report.duration = start.elapsed();
        report.finalize();
        report
    }
}
