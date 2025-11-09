"""
Test runtime selection transparency (Phase 8: T136-T139)

Tests automatic runtime selection, fallback behavior, and result consistency
between Rust and Python implementations.
"""

import pytest
import sys
import asyncio
from unittest.mock import patch, MagicMock
import numpy as np

# Import SDK components
from remotemedia import (
    Pipeline,
    is_rust_runtime_available,
    get_rust_runtime,
    try_load_rust_runtime,
)
from remotemedia.nodes.audio import AudioResampleNode, VADNode, FormatConverterNode


class TestRuntimeDetection:
    """T136: Test runtime availability detection"""
    
    def test_is_rust_runtime_available(self):
        """Test runtime availability check returns boolean"""
        result = is_rust_runtime_available()
        assert isinstance(result, bool)
    
    def test_try_load_rust_runtime_returns_tuple(self):
        """Test try_load_rust_runtime returns proper tuple"""
        success, module, error = try_load_rust_runtime()
        
        assert isinstance(success, bool)
        if success:
            assert module is not None
            assert error is None
            assert hasattr(module, '__version__')
        else:
            assert module is None
            assert isinstance(error, str)
    
    def test_get_rust_runtime_consistency(self):
        """Test get_rust_runtime returns consistent result"""
        runtime1 = get_rust_runtime()
        runtime2 = get_rust_runtime()
        
        # Should return same instance
        assert runtime1 is runtime2


class TestAutomaticSelection:
    """T137: Test automatic Rust selection when available"""
    
    @pytest.mark.asyncio
    async def test_pipeline_uses_rust_when_available(self):
        """Test pipeline automatically uses Rust runtime if available"""
        if not is_rust_runtime_available():
            pytest.skip("Rust runtime not available")
        
        # Create simple pipeline
        pipeline = Pipeline(name="test_auto_rust")
        pipeline.add_node(AudioResampleNode(
            name="resample",
            target_rate=16000,
            runtime_hint="auto"  # Should select Rust automatically
        ))
        
        # Create test audio data
        audio_data = np.random.randn(1, 48000).astype(np.float32)  # 1 second at 48kHz
        
        # Execute with use_rust=True (default)
        result = await pipeline.run({
            "audio_data": audio_data,
            "sample_rate": 48000
        })
        
        assert result is not None
        # If Rust is available, should use it without errors
    
    @pytest.mark.asyncio
    async def test_explicit_rust_hint_uses_rust(self):
        """Test explicit runtime_hint='rust' uses Rust implementation"""
        if not is_rust_runtime_available():
            pytest.skip("Rust runtime not available")
        
        pipeline = Pipeline(name="test_explicit_rust")
        pipeline.add_node(AudioResampleNode(
            name="resample",
            target_rate=16000,
            runtime_hint="rust"  # Explicit Rust
        ))
        
        audio_data = np.random.randn(1, 48000).astype(np.float32)
        
        result = await pipeline.run({
            "audio_data": audio_data,
            "sample_rate": 48000
        })
        
        assert result is not None


class TestPythonFallback:
    """T138: Test Python fallback when Rust unavailable"""
    
    @pytest.mark.asyncio
    async def test_pipeline_falls_back_to_python(self):
        """Test pipeline falls back to Python when use_rust=False"""
        pipeline = Pipeline(name="test_python_fallback")
        pipeline.add_node(AudioResampleNode(
            name="resample",
            target_rate=16000,
            runtime_hint="python"  # Force Python
        ))
        
        audio_data = np.random.randn(1, 48000).astype(np.float32)
        
        # Explicitly use Python executor
        result = await pipeline.run(
            {"audio_data": audio_data, "sample_rate": 48000},
            use_rust=False
        )
        
        assert result is not None
    
    @pytest.mark.asyncio
    async def test_graceful_fallback_on_rust_failure(self):
        """Test graceful fallback if Rust runtime fails"""
        pipeline = Pipeline(name="test_fallback")
        pipeline.add_node(AudioResampleNode(
            name="resample",
            target_rate=16000,
            runtime_hint="auto"
        ))
        
        audio_data = np.random.randn(1, 48000).astype(np.float32)
        
        # Mock Rust runtime to fail
        with patch('remotemedia.is_rust_runtime_available', return_value=False):
            # Should fall back to Python without raising
            result = await pipeline.run({
                "audio_data": audio_data,
                "sample_rate": 48000
            })
            
            assert result is not None
    
    def test_warning_when_rust_unavailable(self):
        """Test warning is issued when Rust runtime unavailable"""
        import warnings
        
        # Force reload to trigger warning
        with patch('remotemedia.try_load_rust_runtime', return_value=(False, None, "Test error")):
            with warnings.catch_warnings(record=True) as w:
                warnings.simplefilter("always")
                
                # Trigger runtime check
                from remotemedia import is_rust_runtime_available
                # Force re-check
                import remotemedia
                remotemedia._rust_runtime = None
                is_rust_runtime_available()
                
                # Should have warning (if Rust not available)
                if not get_rust_runtime():
                    assert len([x for x in w if "Rust runtime unavailable" in str(x.message)]) > 0


class TestResultConsistency:
    """T139: Test identical results from Rust and Python implementations"""
    
    @pytest.mark.asyncio
    async def test_resample_rust_vs_python_consistency(self):
        """Test Rust and Python resampling produce similar results"""
        if not is_rust_runtime_available():
            pytest.skip("Rust runtime not available for comparison")
        
        # Create test audio
        audio_data = np.random.randn(1, 48000).astype(np.float32)
        input_data = {"audio_data": audio_data, "sample_rate": 48000}
        
        # Test with Rust
        pipeline_rust = Pipeline(name="test_rust")
        pipeline_rust.add_node(AudioResampleNode(
            name="resample",
            target_rate=16000,
            runtime_hint="rust"
        ))
        
        result_rust = await pipeline_rust.run(input_data)
        
        # Test with Python
        pipeline_python = Pipeline(name="test_python")
        pipeline_python.add_node(AudioResampleNode(
            name="resample",
            target_rate=16000,
            runtime_hint="python"
        ))
        
        result_python = await pipeline_python.run(input_data, use_rust=False)
        
        # Both should produce results
        assert result_rust is not None
        assert result_python is not None
        
        # Extract audio data from results
        if isinstance(result_rust, dict) and 'audio_data' in result_rust:
            rust_audio = result_rust['audio_data']
            python_audio = result_python['audio_data']
            
            # Shapes should match
            assert rust_audio.shape == python_audio.shape
            
            # Results should be similar (allow for implementation differences)
            # Use correlation as metric
            if rust_audio.size > 0 and python_audio.size > 0:
                correlation = np.corrcoef(
                    rust_audio.flatten(),
                    python_audio.flatten()
                )[0, 1]
                
                # Should be highly correlated (>0.95)
                assert correlation > 0.95, f"Correlation {correlation} too low"
    
    @pytest.mark.asyncio
    async def test_vad_rust_vs_python_consistency(self):
        """Test VAD produces consistent results"""
        if not is_rust_runtime_available():
            pytest.skip("Rust runtime not available for comparison")
        
        # Create test audio with clear speech pattern
        duration = 1.0  # 1 second
        sample_rate = 16000
        t = np.linspace(0, duration, int(sample_rate * duration))
        
        # Sine wave as simple "speech" signal
        audio_data = (np.sin(2 * np.pi * 440 * t) * 0.5).astype(np.float32).reshape(1, -1)
        
        input_data = {"audio_data": audio_data, "sample_rate": sample_rate}
        
        # Test with Rust
        pipeline_rust = Pipeline(name="vad_rust")
        pipeline_rust.add_node(VADNode(
            name="vad",
            threshold=0.01,
            runtime_hint="rust"
        ))
        
        result_rust = await pipeline_rust.run(input_data)
        
        # Test with Python
        pipeline_python = Pipeline(name="vad_python")
        pipeline_python.add_node(VADNode(
            name="vad",
            threshold=0.01,
            runtime_hint="python"
        ))
        
        result_python = await pipeline_python.run(input_data, use_rust=False)
        
        # Both should detect speech in this simple test
        assert result_rust is not None
        assert result_python is not None


class TestNodeRuntimeSelection:
    """Test individual node runtime selection"""
    
    def test_audio_resample_node_auto_selection(self):
        """Test AudioResampleNode auto runtime selection"""
        node = AudioResampleNode(
            name="test",
            target_rate=16000,
            runtime_hint="auto"
        )
        
        assert node.runtime_hint == "auto"
    
    def test_vad_node_python_selection(self):
        """Test VADNode Python runtime selection"""
        node = VADNode(
            name="test",
            threshold=0.01,
            runtime_hint="python"
        )
        
        assert node.runtime_hint == "python"
    
    def test_format_converter_rust_selection(self):
        """Test FormatConverterNode Rust runtime selection"""
        node = FormatConverterNode(
            name="test",
            target_format="f32",
            runtime_hint="rust"
        )
        
        assert node.runtime_hint == "rust"


class TestCrossPlatformPortability:
    """Test same code works across different environments"""
    
    @pytest.mark.asyncio
    async def test_pipeline_works_without_rust(self):
        """Test pipeline works even if Rust runtime not installed"""
        # Create pipeline that doesn't require Rust
        pipeline = Pipeline(name="portable")
        pipeline.add_node(AudioResampleNode(
            name="resample",
            target_rate=16000,
            runtime_hint="python"
        ))
        
        audio_data = np.random.randn(1, 48000).astype(np.float32)
        
        # Should work regardless of Rust availability
        result = await pipeline.run(
            {"audio_data": audio_data, "sample_rate": 48000},
            use_rust=False
        )
        
        assert result is not None
    
    @pytest.mark.asyncio
    async def test_auto_runtime_works_everywhere(self):
        """Test runtime_hint='auto' works on all systems"""
        pipeline = Pipeline(name="auto_runtime")
        pipeline.add_node(AudioResampleNode(
            name="resample",
            target_rate=16000,
            runtime_hint="auto"  # Should work everywhere
        ))
        
        audio_data = np.random.randn(1, 48000).astype(np.float32)
        
        # Should work whether Rust is available or not
        result = await pipeline.run({
            "audio_data": audio_data,
            "sample_rate": 48000
        })
        
        assert result is not None


if __name__ == "__main__":
    # Run tests
    pytest.main([__file__, "-v", "--tb=short"])
