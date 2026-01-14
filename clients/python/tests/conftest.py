"""
pytest configuration and fixtures for RemoteMedia SDK tests
"""

import pytest
import numpy as np
import os
from pathlib import Path


# OmniASR Testing Fixtures

@pytest.fixture
def sample_audio_16khz():
    """
    Generate sample audio at 16kHz for OmniASR testing.

    Returns:
        tuple: (audio_data: np.ndarray, sample_rate: int)
        - audio_data: 1D float32 array with 1 second of audio
        - sample_rate: 16000
    """
    sample_rate = 16000
    duration = 1.0  # seconds
    samples = int(sample_rate * duration)

    # Generate simple sine wave (440 Hz - A note)
    t = np.linspace(0, duration, samples, dtype=np.float32)
    audio_data = (0.5 * np.sin(2 * np.pi * 440 * t)).astype(np.float32)

    return audio_data, sample_rate


@pytest.fixture
def sample_audio_multi_duration():
    """
    Generate audio samples of various durations for testing.

    Returns:
        dict: Mapping duration -> (audio_data, sample_rate)
    """
    sample_rate = 16000
    durations = [0.5, 1.0, 5.0, 10.0, 30.0]  # seconds

    samples = {}
    for duration in durations:
        num_samples = int(sample_rate * duration)
        t = np.linspace(0, duration, num_samples, dtype=np.float32)
        audio_data = (0.5 * np.sin(2 * np.pi * 440 * t)).astype(np.float32)
        samples[duration] = (audio_data, sample_rate)

    return samples


@pytest.fixture
def sample_audio_stereo():
    """
    Generate stereo audio at 16kHz.

    Returns:
        tuple: (audio_data: np.ndarray, sample_rate: int)
        - audio_data: 2D float32 array (2 channels, samples)
        - sample_rate: 16000
    """
    sample_rate = 16000
    duration = 1.0
    samples = int(sample_rate * duration)

    t = np.linspace(0, duration, samples, dtype=np.float32)
    left_channel = 0.5 * np.sin(2 * np.pi * 440 * t)
    right_channel = 0.5 * np.sin(2 * np.pi * 554 * t)  # Different frequency

    audio_data = np.stack([left_channel, right_channel]).astype(np.float32)

    return audio_data, sample_rate


@pytest.fixture
def omniasr_model_config():
    """
    Configuration for OmniASR model testing.

    Returns:
        dict: Model configuration parameters
    """
    return {
        "model_card": "omniASR_LLM_300M",  # Smaller model for faster tests
        "language": "eng_Latn",  # English
        "chunking_mode": "none",
        "chunk_duration": 30.0,
        "device": "cpu",  # CPU for CI/CD compatibility
        "enable_alignment": False,
        "batch_size": 1,
    }


@pytest.fixture
def mock_omniasr_response():
    """
    Mock response from OmniASR transcription.

    Returns:
        dict: Mock transcription output
    """
    return {
        "text": "This is a test transcription.",
        "language": "eng_Latn",
        "success": True,
        "word_timestamps": [
            {"word": "This", "start": 0.0, "end": 0.2},
            {"word": "is", "start": 0.2, "end": 0.35},
            {"word": "a", "start": 0.35, "end": 0.42},
            {"word": "test", "start": 0.42, "end": 0.68},
            {"word": "transcription", "start": 0.68, "end": 1.2},
        ],
        "chunk_metadata": {
            "chunk_index": 0,
            "duration": 1.0,
            "sample_rate": 16000,
            "chunking_mode": "none",
            "device": "cpu",
        },
    }


@pytest.fixture
def fairseq2_cache_dir(tmp_path):
    """
    Temporary directory for FAIRSEQ2 model cache in tests.

    Args:
        tmp_path: pytest temporary directory fixture

    Returns:
        Path: Temporary cache directory path
    """
    cache_dir = tmp_path / "fairseq2_cache"
    cache_dir.mkdir()

    # Set environment variable for OmniASR
    os.environ["FAIRSEQ2_CACHE_DIR"] = str(cache_dir)

    yield cache_dir

    # Cleanup
    if "FAIRSEQ2_CACHE_DIR" in os.environ:
        del os.environ["FAIRSEQ2_CACHE_DIR"]


# General Testing Utilities

@pytest.fixture
def temp_audio_file(tmp_path, sample_audio_16khz):
    """
    Create a temporary WAV file for testing.

    Args:
        tmp_path: pytest temporary directory
        sample_audio_16khz: Audio data fixture

    Returns:
        Path: Path to temporary WAV file
    """
    import soundfile as sf

    audio_data, sample_rate = sample_audio_16khz
    wav_path = tmp_path / "test_audio.wav"

    sf.write(str(wav_path), audio_data, sample_rate)

    return wav_path


# Markers for test categorization

def pytest_configure(config):
    """Configure custom pytest markers."""
    config.addinivalue_line(
        "markers", "omniasr: Tests requiring OmniASR models (may be slow)"
    )
    config.addinivalue_line(
        "markers", "gpu: Tests requiring GPU acceleration"
    )
    config.addinivalue_line(
        "markers", "integration: Integration tests with external services"
    )
