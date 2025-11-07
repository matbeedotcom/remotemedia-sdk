//! Direct test of VoiceActivityDetector node
//!
//! Since VAD is a streaming node (async generator), we test it directly
//! via PyO3 to validate the CPython executor can handle it.

use pyo3::prelude::*;
use pyo3::types::{PyAnyMethods, PyDict};
use serde_json::json;
use std::ffi::CString;

#[tokio::test]
async fn test_vad_node_direct_instantiation() {
    pyo3::prepare_freethreaded_python();

    Python::attach(|py| {
        // Add python-client to path
        let python_client_path = std::env::current_dir()
            .unwrap()
            .parent()
            .unwrap()
            .join("python-client");

        let path_str = python_client_path.to_str().unwrap();
        let add_path =
            CString::new(format!("import sys; sys.path.insert(0, r'{}')", path_str)).unwrap();
        py.run(&add_path, None, None).unwrap();

        // Import and instantiate VAD node
        let code = CString::new(
            r#"
from remotemedia.nodes.audio import VoiceActivityDetector
import numpy as np

# Create VAD instance
vad = VoiceActivityDetector(
    frame_duration_ms=30,
    energy_threshold=0.02,
    speech_threshold=0.3,
    filter_mode=False,
    include_metadata=True
)

print(f"[OK] Created VAD node: {vad}")
print(f"  Frame duration: {vad.frame_duration_ms}ms")
print(f"  Energy threshold: {vad.energy_threshold}")
print(f"  Speech threshold: {vad.speech_threshold}")
print(f"  Filter mode: {vad.filter_mode}")

# Generate test audio: 1 second at 16kHz
sample_rate = 16000
duration = 1.0
samples = int(duration * sample_rate)

# Create speech-like audio (440Hz tone)
t = np.linspace(0, duration, samples)
speech_audio = 0.3 * np.sin(2 * np.pi * 440 * t)

# Create silence (noise)
silence_audio = np.random.normal(0, 0.01, samples)

print(f"[OK] Generated test audio:")
print(f"  Speech audio shape: {speech_audio.shape}, mean energy: {np.sqrt(np.mean(speech_audio**2)):.4f}")
print(f"  Silence audio shape: {silence_audio.shape}, mean energy: {np.sqrt(np.mean(silence_audio**2)):.4f}")

# Test VAD analysis directly (using private method for testing)
speech_detected, speech_info = vad._analyze_audio(speech_audio, frame_samples=int(sample_rate * 30 / 1000))
silence_detected, silence_info = vad._analyze_audio(silence_audio, frame_samples=int(sample_rate * 30 / 1000))

print(f"\n[OK] VAD Analysis Results:")
print(f"  Speech detected: {speech_detected}")
print(f"    - Speech ratio: {speech_info['speech_ratio']:.2f}")
print(f"    - Avg energy: {speech_info['avg_energy']:.4f}")
print(f"    - Speech frames: {speech_info['speech_frames']}/{speech_info['num_frames']}")
print(f"  Silence detected: {silence_detected}")
print(f"    - Speech ratio: {silence_info['speech_ratio']:.2f}")
print(f"    - Avg energy: {silence_info['avg_energy']:.4f}")
print(f"    - Speech frames: {silence_info['speech_frames']}/{silence_info['num_frames']}")

# Verify results
assert speech_detected == True, "Speech should be detected in tone audio"
assert silence_detected == False, "Speech should NOT be detected in silence"
assert speech_info['avg_energy'] > silence_info['avg_energy'], "Speech energy should be higher than silence"

test_result = {
    'vad_initialized': True,
    'speech_detected': speech_detected,
    'silence_not_detected': not silence_detected,
    'speech_ratio': speech_info['speech_ratio'],
    'silence_ratio': silence_info['speech_ratio']
}
"#,
        )
        .unwrap();

        py.run(&code, None, None).unwrap();

        // Get test results
        let result = py
            .eval(&CString::new("test_result").unwrap(), None, None)
            .unwrap();

        let result_dict = result.downcast::<PyDict>().unwrap();

        let vad_init: bool = result_dict
            .get_item("vad_initialized")
            .unwrap()
            .unwrap()
            .extract()
            .unwrap();
        let speech_detected: bool = result_dict
            .get_item("speech_detected")
            .unwrap()
            .unwrap()
            .extract()
            .unwrap();
        let silence_not_detected: bool = result_dict
            .get_item("silence_not_detected")
            .unwrap()
            .unwrap()
            .extract()
            .unwrap();
        let speech_ratio: f64 = result_dict
            .get_item("speech_ratio")
            .unwrap()
            .unwrap()
            .extract()
            .unwrap();
        let silence_ratio: f64 = result_dict
            .get_item("silence_ratio")
            .unwrap()
            .unwrap()
            .extract()
            .unwrap();

        assert!(vad_init, "VAD should initialize successfully");
        assert!(speech_detected, "VAD should detect speech in 440Hz tone");
        assert!(
            silence_not_detected,
            "VAD should not detect speech in noise"
        );
        assert!(
            speech_ratio > 0.5,
            "Speech ratio should be high for tone: {}",
            speech_ratio
        );
        assert!(
            silence_ratio < 0.5,
            "Speech ratio should be low for silence: {}",
            silence_ratio
        );

        println!("\n✓ All VAD assertions passed!");
    });
}

#[tokio::test]
async fn test_vad_node_with_cpython_executor_single_chunk() {
    use remotemedia_runtime::{
        executor::Executor,
        manifest::{Manifest, ManifestMetadata, NodeManifest, RuntimeHint},
    };

    pyo3::prepare_freethreaded_python();

    // Set up Python path
    Python::attach(|py| {
        let python_client_path = std::env::current_dir()
            .unwrap()
            .parent()
            .unwrap()
            .join("python-client");
        let path_str = python_client_path.to_str().unwrap();
        let add_path =
            CString::new(format!("import sys; sys.path.insert(0, r'{}')", path_str)).unwrap();
        py.run(&add_path, None, None).unwrap();
    });

    // Create a synchronous wrapper around VAD for testing
    // Since VAD.process() is async generator, we'll test the _analyze_audio method directly
    let manifest = Manifest {
        version: "v1".to_string(),
        metadata: ManifestMetadata {
            name: "vad-wrapper-test".to_string(),
            description: Some("Test VAD analysis function".to_string()),
            created_at: None,
        },
        nodes: vec![NodeManifest {
            id: "vad_analyzer".to_string(),
            node_type: "VADAnalyzerSync".to_string(),
            params: json!({
                "frame_duration_ms": 30,
                "energy_threshold": 0.02,
                "speech_threshold": 0.3
            }),
            capabilities: None,
            host: None,
            runtime_hint: Some(RuntimeHint::Cpython),
            ..Default::default()
        }],
        connections: vec![],
    };

    // Create the synchronous wrapper node in Python
    Python::attach(|py| {
        let wrapper_code = CString::new(
            r#"
from remotemedia.nodes.audio import VoiceActivityDetector
import numpy as np

class VADAnalyzerSync:
    """Synchronous wrapper around VAD for testing."""

    def __init__(self, frame_duration_ms=30, energy_threshold=0.02, speech_threshold=0.3):
        self.vad = VoiceActivityDetector(
            frame_duration_ms=frame_duration_ms,
            energy_threshold=energy_threshold,
            speech_threshold=speech_threshold
        )
        self.sample_rate = 16000

    def process(self, audio_data):
        """Analyze audio and return detection result."""
        # Convert input to numpy array
        audio_array = np.array(audio_data, dtype=np.float32)

        # Calculate frame samples
        frame_samples = int(self.sample_rate * self.vad.frame_duration_ms / 1000)

        # Analyze
        is_speech, vad_info = self.vad._analyze_audio(audio_array, frame_samples)

        return {
            "is_speech": is_speech,
            "speech_ratio": vad_info["speech_ratio"],
            "avg_energy": vad_info["avg_energy"],
            "num_frames": vad_info["num_frames"]
        }

# Register in remotemedia.nodes
import sys, types
if 'remotemedia.nodes' not in sys.modules:
    sys.modules['remotemedia.nodes'] = types.ModuleType('remotemedia.nodes')

sys.modules['remotemedia.nodes'].VADAnalyzerSync = VADAnalyzerSync
"#,
        )
        .unwrap();
        py.run(&wrapper_code, None, None).unwrap();
    });

    let executor = Executor::new();

    // Generate speech audio (440Hz tone)
    let speech_audio: Vec<f64> = (0..16000)
        .map(|i| {
            let t = i as f64 / 16000.0;
            0.3 * (2.0 * std::f64::consts::PI * 440.0 * t).sin()
        })
        .collect();

    // Generate silence (low noise) - simple pseudo-random
    let silence_audio: Vec<f64> = (0..16000)
        .map(|i| {
            // Simple LCG for pseudo-random numbers (using wrapping operations)
            let x = ((i as u64).wrapping_mul(1103515245).wrapping_add(12345)) % (1u64 << 31);
            0.01 * ((x as f64 / (1u64 << 31) as f64) - 0.5) * 2.0
        })
        .collect();

    let test_inputs = vec![json!(speech_audio), json!(silence_audio)];

    let result = executor
        .execute_with_input(&manifest, test_inputs)
        .await
        .unwrap();

    println!("VAD Analyzer result: {:?}", result);
    assert_eq!(result.status, "success");

    let outputs = result.outputs.as_array().unwrap();
    assert_eq!(outputs.len(), 2);

    // Check speech detection
    let speech_result = &outputs[0];
    assert_eq!(speech_result["is_speech"], json!(true));
    println!("Speech detected: {}", speech_result);

    // Check silence detection
    let silence_result = &outputs[1];
    assert_eq!(silence_result["is_speech"], json!(false));
    println!("Silence detected: {}", silence_result);

    println!("\n✓ VAD Analyzer (CPython executor) test passed!");
}
