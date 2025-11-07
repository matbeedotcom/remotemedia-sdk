"""
Real-world benchmark of Model Registry using actual Whisper models.

Measures:
1. Memory usage with/without model sharing
2. Cache hit performance
3. Concurrent loading behavior
4. Real memory savings with actual ML models
"""

import asyncio
import time
import psutil
import os
import sys
import numpy as np
from typing import Optional
import logging

logging.basicConfig(level=logging.INFO, format='%(levelname)s: %(message)s')
logger = logging.getLogger(__name__)

# Import model registry
sys.path.insert(0, os.path.join(os.path.dirname(__file__), '..'))
from remotemedia.core import ModelRegistry, get_or_load

# Import Whisper dependencies
try:
    import torch
    from transformers import AutoModelForSpeechSeq2Seq, AutoProcessor
    WHISPER_AVAILABLE = True
except ImportError:
    WHISPER_AVAILABLE = False
    logger.warning("Whisper dependencies not available - install: pip install torch transformers")


class WhisperModelWrapper:
    """Wrapper for Whisper model with memory tracking"""
    
    def __init__(self, model_id: str, device: str = "cpu"):
        self.model_id = model_id
        self.device = device
        self.model = None
        self.processor = None
        self._memory_before = None
        self._memory_after = None
        
    def load(self):
        """Load the model and track memory"""
        logger.info(f"Loading {self.model_id} on {self.device}...")
        
        # Measure memory before
        process = psutil.Process()
        self._memory_before = process.memory_info().rss / 1024**2  # MB
        
        start_time = time.time()
        
        # Load model
        torch_dtype = torch.float16 if self.device == "cuda" and torch.cuda.is_available() else torch.float32
        
        self.model = AutoModelForSpeechSeq2Seq.from_pretrained(
            self.model_id,
            torch_dtype=torch_dtype,
            low_cpu_mem_usage=True,
            use_safetensors=True
        )
        self.model.to(self.device)
        
        self.processor = AutoProcessor.from_pretrained(self.model_id)
        
        load_time = time.time() - start_time
        
        # Measure memory after
        self._memory_after = process.memory_info().rss / 1024**2  # MB
        memory_used = self._memory_after - self._memory_before
        
        logger.info(f"Loaded in {load_time:.1f}s, memory: {memory_used:.0f}MB")
        
        return self
    
    def memory_usage(self) -> int:
        """Return estimated memory in bytes"""
        if self._memory_before and self._memory_after:
            return int((self._memory_after - self._memory_before) * 1024**2)
        return 100 * 1024**2  # Default 100MB
    
    def infer(self, audio: np.ndarray) -> str:
        """Simple inference for testing"""
        inputs = self.processor(audio, sampling_rate=16000, return_tensors="pt")
        with torch.no_grad():
            generated_ids = self.model.generate(inputs["input_features"])
        transcription = self.processor.batch_decode(generated_ids, skip_special_tokens=True)[0]
        return transcription


async def benchmark_without_registry(model_id: str, device: str, num_instances: int = 3):
    """Benchmark WITHOUT model registry (baseline)"""
    print("\n" + "="*70)
    print(f"BENCHMARK 1: WITHOUT Model Registry (Baseline)")
    print(f"Loading {num_instances} instances of {model_id}")
    print("="*70)
    
    process = psutil.Process()
    mem_before = process.memory_info().rss / 1024**2
    
    models = []
    load_times = []
    
    for i in range(num_instances):
        print(f"\n[Instance {i+1}/{num_instances}]")
        start = time.time()
        model = WhisperModelWrapper(model_id, device).load()
        load_time = time.time() - start
        load_times.append(load_time)
        models.append(model)
    
    mem_after = process.memory_info().rss / 1024**2
    total_memory = mem_after - mem_before
    
    print(f"\n{'-'*70}")
    print(f"RESULTS (Without Registry):")
    print(f"  Total memory used: {total_memory:.0f}MB")
    print(f"  Average per instance: {total_memory/num_instances:.0f}MB")
    print(f"  Load times: {[f'{t:.1f}s' for t in load_times]}")
    print(f"  Total load time: {sum(load_times):.1f}s")
    print(f"{'-'*70}")
    
    return {
        'total_memory_mb': total_memory,
        'per_instance_mb': total_memory / num_instances,
        'load_times': load_times,
        'total_load_time': sum(load_times),
    }


async def benchmark_with_registry(model_id: str, device: str, num_instances: int = 3):
    """Benchmark WITH model registry"""
    print("\n" + "="*70)
    print(f"BENCHMARK 2: WITH Model Registry")
    print(f"Loading {num_instances} instances of {model_id}")
    print("="*70)
    
    # Clear registry
    registry = ModelRegistry()
    registry.clear()
    
    process = psutil.Process()
    mem_before = process.memory_info().rss / 1024**2
    
    model_key = f"{model_id}@{device}"
    models = []
    load_times = []
    
    for i in range(num_instances):
        print(f"\n[Instance {i+1}/{num_instances}]")
        start = time.time()
        
        # Use registry - first call loads, others hit cache
        model = get_or_load(
            model_key,
            lambda: WhisperModelWrapper(model_id, device).load()
        )
        
        load_time = time.time() - start
        load_times.append(load_time)
        models.append(model)
        
        # Show cache stats
        metrics = registry.metrics()
        print(f"  Cache hit rate: {metrics.hit_rate:.1%}")
    
    mem_after = process.memory_info().rss / 1024**2
    total_memory = mem_after - mem_before
    
    # Verify all instances are the same
    for i in range(1, len(models)):
        assert models[i] is models[0], f"Instance {i} should be same as instance 0"
    
    print(f"\n{'-'*70}")
    print(f"RESULTS (With Registry):")
    print(f"  Total memory used: {total_memory:.0f}MB")
    print(f"  Average per instance: {total_memory/num_instances:.0f}MB")
    print(f"  Load times: {[f'{t:.3f}s' if t < 1 else f'{t:.1f}s' for t in load_times]}")
    print(f"  Total load time: {sum(load_times):.1f}s")
    print(f"  All instances identical: {all(m is models[0] for m in models)}")
    
    metrics = registry.metrics()
    print(f"\n  Registry Metrics:")
    print(f"    Cache hits: {metrics.cache_hits}")
    print(f"    Cache misses: {metrics.cache_misses}")
    print(f"    Hit rate: {metrics.hit_rate:.1%}")
    print(f"{'-'*70}")
    
    return {
        'total_memory_mb': total_memory,
        'per_instance_mb': total_memory / num_instances,
        'load_times': load_times,
        'total_load_time': sum(load_times),
        'cache_hits': metrics.cache_hits,
        'cache_misses': metrics.cache_misses,
        'hit_rate': metrics.hit_rate,
    }


def print_comparison(baseline, with_registry, num_instances):
    """Print detailed comparison"""
    print("\n" + "="*70)
    print("COMPARISON SUMMARY")
    print("="*70)
    
    memory_saved_mb = baseline['total_memory_mb'] - with_registry['total_memory_mb']
    memory_saved_pct = (memory_saved_mb / baseline['total_memory_mb']) * 100
    
    time_saved = baseline['total_load_time'] - with_registry['total_load_time']
    time_saved_pct = (time_saved / baseline['total_load_time']) * 100
    
    print(f"\nMemory Usage:")
    print(f"  Without registry: {baseline['total_memory_mb']:.0f}MB")
    print(f"  With registry:    {with_registry['total_memory_mb']:.0f}MB")
    print(f"  Memory saved:     {memory_saved_mb:.0f}MB ({memory_saved_pct:.1f}%)")
    
    print(f"\nLoading Time:")
    print(f"  Without registry: {baseline['total_load_time']:.1f}s")
    print(f"  With registry:    {with_registry['total_load_time']:.1f}s")
    print(f"  Time saved:       {time_saved:.1f}s ({time_saved_pct:.1f}%)")
    
    print(f"\nCache Performance:")
    print(f"  First load:  {with_registry['load_times'][0]:.1f}s")
    cache_times_ms = [t*1000 for t in with_registry['load_times'][1:]]
    print(f"  Cache hits:  {[f'{t:.2f}ms' for t in cache_times_ms]}")
    if len(with_registry['load_times']) > 1 and with_registry['load_times'][1] > 0:
        print(f"  Speedup:     {with_registry['load_times'][0] / with_registry['load_times'][1]:.0f}x faster")
    else:
        print(f"  Speedup:     Instant (cache hits too fast to measure reliably)")
    
    print(f"\nRegistry Metrics:")
    print(f"  Cache hits:   {with_registry['cache_hits']}")
    print(f"  Cache misses: {with_registry['cache_misses']}")
    print(f"  Hit rate:     {with_registry['hit_rate']:.1%}")
    
    print(f"\nConclusions:")
    if memory_saved_pct >= 60:
        print(f"  [PASS] Memory reduction target MET ({memory_saved_pct:.1f}% >= 60%)")
    else:
        print(f"  [FAIL] Memory reduction target MISSED ({memory_saved_pct:.1f}% < 60%)")
    
    avg_cache_time_ms = np.mean(with_registry['load_times'][1:]) * 1000
    if avg_cache_time_ms < 100:
        print(f"  [PASS] Cache performance target MET ({avg_cache_time_ms:.2f}ms < 100ms)")
    else:
        print(f"  [FAIL] Cache performance target MISSED ({avg_cache_time_ms:.2f}ms >= 100ms)")
    
    print("="*70 + "\n")
    
    cache_speedup = 1
    if len(with_registry['load_times']) > 1 and with_registry['load_times'][1] > 0:
        cache_speedup = with_registry['load_times'][0] / with_registry['load_times'][1]
    
    return {
        'memory_saved_mb': memory_saved_mb,
        'memory_saved_pct': memory_saved_pct,
        'time_saved_s': time_saved,
        'cache_speedup': cache_speedup,
        'avg_cache_time_ms': avg_cache_time_ms,
    }


async def main():
    """Run comprehensive benchmarks"""
    print("\n" + "="*70)
    print(" "*15 + "MODEL REGISTRY BENCHMARK")
    print(" "*12 + "Real Whisper Model Performance")
    print("="*70)
    
    if not WHISPER_AVAILABLE:
        print("\n[ERROR] Whisper dependencies not installed")
        print("   Install with: pip install torch transformers")
        return 1
    
    # Configuration
    MODEL_ID = "openai/whisper-tiny.en"  # Small model for faster benchmarking
    DEVICE = "cpu"  # Use CPU for consistent measurements
    NUM_INSTANCES = 3
    
    print(f"\nConfiguration:")
    print(f"  Model: {MODEL_ID}")
    print(f"  Device: {DEVICE}")
    print(f"  Instances: {NUM_INSTANCES}")
    print(f"  Process PID: {os.getpid()}")
    
    try:
        # Benchmark WITHOUT registry (baseline)
        baseline = await benchmark_without_registry(MODEL_ID, DEVICE, NUM_INSTANCES)
        
        # Clear memory
        import gc
        gc.collect()
        
        # Small delay to stabilize
        await asyncio.sleep(2)
        
        # Benchmark WITH registry
        with_registry = await benchmark_with_registry(MODEL_ID, DEVICE, NUM_INSTANCES)
        
        # Print comparison
        results = print_comparison(baseline, with_registry, NUM_INSTANCES)
        
        # Save results to file
        import json
        results_file = "benchmark_results_whisper.json"
        with open(results_file, 'w') as f:
            json.dump({
                'model_id': MODEL_ID,
                'device': DEVICE,
                'num_instances': NUM_INSTANCES,
                'baseline': baseline,
                'with_registry': with_registry,
                'comparison': results,
                'timestamp': time.time(),
            }, f, indent=2)
        
        print(f"[SAVED] Results saved to: {results_file}\n")
        
        return 0
        
    except Exception as e:
        print(f"\n[ERROR] Benchmark failed: {e}")
        import traceback
        traceback.print_exc()
        return 1


if __name__ == "__main__":
    exit_code = asyncio.run(main())
    sys.exit(exit_code)

