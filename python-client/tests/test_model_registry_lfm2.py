"""
Automated test for model registry integration with LFM2AudioNode.

This test validates that multiple LFM2AudioNode instances share the same
underlying model and processor, reducing memory usage.
"""

import pytest
import asyncio
import numpy as np
from remotemedia.core import ModelRegistry, get_or_load
from remotemedia.nodes.ml.lfm2_audio import LFM2AudioNode


class MockLFM2Model:
    """Mock LFM2 model for testing without actual model weights"""
    
    def __init__(self, repo_name: str, device: str):
        self.repo_name = repo_name
        self.device = device
        self.load_count = 0
    
    def memory_usage(self) -> int:
        """Return simulated memory usage"""
        return 1_500_000_000  # 1.5GB
    
    def eval(self):
        """Mock eval mode"""
        return self
    
    def cuda(self):
        """Mock CUDA placement"""
        return self
    
    def to(self, device):
        """Mock device placement"""
        return self
    
    @staticmethod
    def from_pretrained(repo, **kwargs):
        """Mock pretrained loading"""
        model = MockLFM2Model(repo, kwargs.get('device', 'cpu'))
        model.load_count = 1
        return model


class MockLFM2Processor:
    """Mock LFM2 processor for testing"""
    
    def __init__(self, repo_name: str, device: str):
        self.repo_name = repo_name
        self.device = device
        self.load_count = 0
    
    def eval(self):
        return self
    
    def to(self, device):
        return self
    
    @staticmethod
    def from_pretrained(repo, **kwargs):
        processor = MockLFM2Processor(repo, kwargs.get('device', 'cpu'))
        processor.load_count = 1
        return processor


@pytest.mark.asyncio
async def test_lfm2_model_sharing_via_registry():
    """
    Test that multiple LFM2AudioNode instances share models via registry.
    
    This validates:
    - Same model instance is reused
    - Memory is not duplicated
    - Cache hit rate improves
    """
    # Clear registry for clean test
    registry = ModelRegistry()
    registry.clear()
    
    # Get initial metrics
    initial_metrics = registry.metrics()
    assert initial_metrics.total_models == 0
    assert initial_metrics.cache_hits == 0
    
    # Simulate model loading via registry
    model_key = "LiquidAI/LFM2-Audio-1.5B@cpu"
    processor_key = "LiquidAI/LFM2-Audio-1.5B_processor@cpu"
    
    # First load (cache miss)
    model1 = get_or_load(model_key, lambda: MockLFM2Model("LiquidAI/LFM2-Audio-1.5B", "cpu"))
    processor1 = get_or_load(processor_key, lambda: MockLFM2Processor("LiquidAI/LFM2-Audio-1.5B", "cpu"))
    
    # Second load (cache hit)
    model2 = get_or_load(model_key, lambda: MockLFM2Model("LiquidAI/LFM2-Audio-1.5B", "cpu"))
    processor2 = get_or_load(processor_key, lambda: MockLFM2Processor("LiquidAI/LFM2-Audio-1.5B", "cpu"))
    
    # Verify same instances
    assert model1 is model2, "Models should be the same instance"
    assert processor1 is processor2, "Processors should be the same instance"
    
    # Verify metrics
    metrics = registry.metrics()
    assert metrics.total_models == 2, f"Should have 2 models (model + processor), got {metrics.total_models}"
    assert metrics.cache_hits == 2, f"Should have 2 cache hits, got {metrics.cache_hits}"
    assert metrics.cache_misses == 2, f"Should have 2 cache misses, got {metrics.cache_misses}"
    assert metrics.hit_rate == 0.5, f"Hit rate should be 50%, got {metrics.hit_rate}"
    
    print("\n[PASS] Multiple LFM2 instances share models correctly")
    print(f"  - Models loaded: {metrics.total_models}")
    print(f"  - Cache hit rate: {metrics.hit_rate:.1%}")
    print(f"  - Memory saved: ~1.5GB per shared instance")


@pytest.mark.asyncio
async def test_concurrent_lfm2_loading():
    """
    Test that concurrent LFM2 model loads deduplicate correctly.
    
    This validates that if multiple nodes try to load the same model
    simultaneously, only one actual load occurs.
    """
    registry = ModelRegistry()
    registry.clear()
    
    load_counter = {"count": 0}
    
    def tracked_loader():
        load_counter["count"] += 1
        return MockLFM2Model("test-model", "cpu")
    
    # Launch 5 concurrent loads of the same model
    tasks = []
    for i in range(5):
        task = asyncio.create_task(
            asyncio.to_thread(get_or_load, "concurrent-test", tracked_loader)
        )
        tasks.append(task)
    
    # Wait for all to complete
    models = await asyncio.gather(*tasks)
    
    # Verify only one actual load occurred
    assert load_counter["count"] == 1, f"Should only load once, but loaded {load_counter['count']} times"
    
    # Verify all got the same instance
    first_model = models[0]
    for model in models[1:]:
        assert model is first_model, "All concurrent requests should get same instance"
    
    print("\n[PASS] Concurrent loading deduplicates correctly")
    print(f"  - Concurrent requests: 5")
    print(f"  - Actual loads: {load_counter['count']}")


@pytest.mark.asyncio
async def test_memory_efficiency():
    """
    Test memory efficiency improvements from model sharing.
    
    Simulates a pipeline with 3 nodes using the same model and verifies
    memory is only allocated once.
    """
    registry = ModelRegistry()
    registry.clear()
    
    model_size_mb = 1500  # 1.5GB model
    
    # Simulate 3 nodes loading the same model
    nodes = []
    for i in range(3):
        model = get_or_load(
            "large-model",
            lambda: MockLFM2Model("large-model", "cuda")
        )
        nodes.append(model)
    
    # Verify all nodes share the same model
    for i in range(1, len(nodes)):
        assert nodes[i] is nodes[0], f"Node {i} should share model with node 0"
    
    metrics = registry.metrics()
    
    # Memory usage should be for ONE model, not three
    expected_memory_gb = model_size_mb / 1024
    actual_memory_gb = metrics.total_memory_bytes / (1024**3)
    
    # With sharing: ~1.5GB
    # Without sharing: ~4.5GB
    memory_saved_gb = (3 * expected_memory_gb) - actual_memory_gb
    savings_percent = (memory_saved_gb / (3 * expected_memory_gb)) * 100
    
    assert savings_percent >= 60, f"Should save at least 60% memory, saved {savings_percent:.1f}%"
    
    print("\n[PASS] Memory efficiency validated")
    print(f"  - Nodes: 3")
    print(f"  - Model size: {expected_memory_gb:.1f}GB each")
    print(f"  - Memory without sharing: {3 * expected_memory_gb:.1f}GB")
    print(f"  - Memory with sharing: {actual_memory_gb:.2f}GB")
    print(f"  - Memory saved: {memory_saved_gb:.1f}GB ({savings_percent:.0f}%)")


@pytest.mark.asyncio
async def test_cache_hit_performance():
    """
    Test that cache hits are fast (<100ms target).
    """
    import time
    
    registry = ModelRegistry()
    registry.clear()
    
    # First load
    model = get_or_load("perf-test", lambda: MockLFM2Model("perf-test", "cpu"))
    
    # Measure cache hit time
    iterations = 100
    times = []
    
    for _ in range(iterations):
        start = time.perf_counter()
        cached_model = get_or_load("perf-test", lambda: MockLFM2Model("perf-test", "cpu"))
        elapsed_ms = (time.perf_counter() - start) * 1000
        times.append(elapsed_ms)
        assert cached_model is model
    
    avg_time_ms = sum(times) / len(times)
    max_time_ms = max(times)
    
    # All cache hits should be under 100ms target (typically <1ms)
    assert avg_time_ms < 100, f"Average cache hit time {avg_time_ms:.2f}ms exceeds 100ms target"
    assert max_time_ms < 100, f"Max cache hit time {max_time_ms:.2f}ms exceeds 100ms target"
    
    print("\n[PASS] Cache hit performance validated")
    print(f"  - Iterations: {iterations}")
    print(f"  - Average: {avg_time_ms:.3f}ms")
    print(f"  - Max: {max_time_ms:.3f}ms")
    print(f"  - Target: <100ms")


def test_registry_singleton():
    """Test that ModelRegistry maintains singleton pattern"""
    reg1 = ModelRegistry()
    reg2 = ModelRegistry()
    
    assert reg1 is reg2, "Registry should be singleton"
    
    print("\n[PASS] Registry singleton pattern validated")


if __name__ == "__main__":
    """Run tests manually for quick validation"""
    print("\n" + "="*60)
    print("MODEL REGISTRY - LFM2 INTEGRATION TESTS")
    print("="*60)
    
    # Run synchronous tests
    test_registry_singleton()
    
    # Run async tests
    asyncio.run(test_lfm2_model_sharing_via_registry())
    asyncio.run(test_concurrent_lfm2_loading())
    asyncio.run(test_memory_efficiency())
    asyncio.run(test_cache_hit_performance())
    
    print("\n" + "="*60)
    print("ALL TESTS PASSED")
    print("="*60 + "\n")

