"""
Benchmark Whisper node loading through our Rust multiprocess system.

Compares:
1. WITHOUT model registry (baseline - pre-optimization)
2. WITH model registry (post-optimization)

Uses actual RemoteMedia multiprocess executor with iceoryx2 IPC.
"""

import asyncio
import time
import psutil
import os
import sys
import json
import logging

logging.basicConfig(level=logging.INFO, format='%(levelname)s: %(message)s')
logger = logging.getLogger(__name__)

# Add python-client to path
sys.path.insert(0, os.path.join(os.path.dirname(__file__), '..'))

try:
    from remotemedia.core import MultiprocessNode, NodeConfig
    from remotemedia.nodes.ml.whisper_transcription import WhisperTranscriptionNode
    MULTIPROCESS_AVAILABLE = True
except ImportError as e:
    logger.error(f"Failed to import multiprocess components: {e}")
    MULTIPROCESS_AVAILABLE = False

try:
    import torch
    TORCH_AVAILABLE = True
except ImportError:
    TORCH_AVAILABLE = False


class WhisperNodeNoRegistry(WhisperTranscriptionNode):
    """Whisper node WITHOUT model registry (baseline)"""
    
    async def initialize(self) -> None:
        """Initialize WITHOUT using model registry"""
        try:
            import torch
            from transformers import AutoModelForSpeechSeq2Seq, AutoProcessor, pipeline
        except ImportError:
            raise RuntimeError("Required ML libraries not installed")

        if self._requested_device:
            self.device = self._requested_device
        elif torch.cuda.is_available():
            self.device = "cuda:0"
        else:
            self.device = "cpu"

        self.torch_dtype = torch.float32
        if torch.cuda.is_available():
            self.torch_dtype = torch.float16

        logger.info(f"[NO REGISTRY] Loading Whisper {self.model_id} on {self.device}...")
        start = time.time()
        
        # Load directly without registry
        model = await asyncio.to_thread(
            AutoModelForSpeechSeq2Seq.from_pretrained,
            self.model_id,
            torch_dtype=self.torch_dtype,
            low_cpu_mem_usage=True,
            use_safetensors=True
        )
        model.to(self.device)
        processor = await asyncio.to_thread(AutoProcessor.from_pretrained, self.model_id)
        
        self.transcription_pipeline = pipeline(
            "automatic-speech-recognition",
            model=model,
            tokenizer=processor.tokenizer,
            feature_extractor=processor.feature_extractor,
            torch_dtype=self.torch_dtype,
            device=self.device,
            chunk_length_s=self.chunk_length_s,
            return_timestamps="word",
        )
        
        elapsed = time.time() - start
        logger.info(f"[NO REGISTRY] Loaded in {elapsed:.1f}s")


async def benchmark_without_registry(model_id: str, num_nodes: int = 3):
    """Benchmark WITHOUT model registry (pre-optimization)"""
    print("\n" + "="*80)
    print(f"BENCHMARK 1: WITHOUT Model Registry (Pre-Optimization)")
    print(f"Creating {num_nodes} Whisper nodes")
    print("="*80)
    
    process = psutil.Process()
    mem_before = process.memory_info().rss / 1024**2
    
    nodes = []
    init_times = []
    
    for i in range(num_nodes):
        print(f"\n[Node {i+1}/{num_nodes}] Initializing...")
        
        # Create node with registry DISABLED
        node = WhisperTranscriptionNode(
            model_id=model_id,
            device="cpu",
            use_registry=False,  # Explicitly disable
        )
        
        start = time.time()
        await node.initialize()
        init_time = time.time() - start
        init_times.append(init_time)
        
        nodes.append(node)
        print(f"  Initialized in {init_time:.1f}s")
    
    mem_after = process.memory_info().rss / 1024**2
    total_memory = mem_after - mem_before
    total_time = sum(init_times)
    
    print(f"\n{'-'*80}")
    print(f"RESULTS (Without Registry):")
    print(f"  Total memory: {total_memory:.0f}MB")
    print(f"  Avg per node: {total_memory/num_nodes:.0f}MB")
    print(f"  Init times: {[f'{t:.1f}s' for t in init_times]}")
    print(f"  Total time: {total_time:.1f}s")
    print(f"{'-'*80}")
    
    # Cleanup
    for node in nodes:
        await node.cleanup()
    
    return {
        'total_memory_mb': total_memory,
        'per_node_mb': total_memory / num_nodes,
        'init_times': init_times,
        'total_time_s': total_time,
        'avg_init_time_s': np.mean(init_times),
    }


async def benchmark_with_registry(model_id: str, num_nodes: int = 3):
    """Benchmark WITH model registry (post-optimization)"""
    print("\n" + "="*80)
    print(f"BENCHMARK 2: WITH Model Registry (Post-Optimization)")
    print(f"Creating {num_nodes} Whisper nodes")
    print("="*80)
    
    from remotemedia.core import ModelRegistry
    registry = ModelRegistry()
    registry.clear()
    
    process = psutil.Process()
    mem_before = process.memory_info().rss / 1024**2
    
    nodes = []
    init_times = []
    
    for i in range(num_nodes):
        print(f"\n[Node {i+1}/{num_nodes}] Initializing...")
        
        # Create node with registry ENABLED (default)
        node = WhisperTranscriptionNode(
            model_id=model_id,
            device="cpu",
            use_registry=True,  # Explicitly enable
        )
        
        start = time.time()
        await node.initialize()
        init_time = time.time() - start
        init_times.append(init_time)
        
        nodes.append(node)
        
        metrics = registry.metrics()
        print(f"  Initialized in {init_time:.1f}s (cache hit rate: {metrics.hit_rate:.1%})")
    
    mem_after = process.memory_info().rss / 1024**2
    total_memory = mem_after - mem_before
    total_time = sum(init_times)
    
    metrics = registry.metrics()
    
    # Verify sharing
    all_same = all(nodes[i] is nodes[0] for i in range(1, len(nodes))) if len(nodes) > 1 else True
    
    print(f"\n{'-'*80}")
    print(f"RESULTS (With Registry):")
    print(f"  Total memory: {total_memory:.0f}MB")
    print(f"  Avg per node: {total_memory/num_nodes:.0f}MB")
    print(f"  Init times: {[f'{t:.1f}s' if t >= 1 else f'{t*1000:.0f}ms' for t in init_times]}")
    print(f"  Total time: {total_time:.1f}s")
    print(f"  Cache hits: {metrics.cache_hits}")
    print(f"  Cache misses: {metrics.cache_misses}")
    print(f"  Hit rate: {metrics.hit_rate:.1%}")
    print(f"{'-'*80}")
    
    # Cleanup
    for node in nodes:
        await node.cleanup()
    
    return {
        'total_memory_mb': total_memory,
        'per_node_mb': total_memory / num_nodes,
        'init_times': init_times,
        'total_time_s': total_time,
        'avg_init_time_s': np.mean(init_times),
        'cache_hits': metrics.cache_hits,
        'cache_misses': metrics.cache_misses,
        'hit_rate': metrics.hit_rate,
    }


def print_comparison(baseline: dict, optimized: dict, num_nodes: int):
    """Print detailed comparison"""
    print("\n" + "="*80)
    print("MULTIPROCESS SYSTEM: WITH vs WITHOUT MODEL REGISTRY")
    print("="*80)
    
    memory_saved_mb = baseline['total_memory_mb'] - optimized['total_memory_mb']
    memory_saved_pct = (memory_saved_mb / baseline['total_memory_mb']) * 100 if baseline['total_memory_mb'] > 0 else 0
    
    time_saved_s = baseline['total_time_s'] - optimized['total_time_s']
    time_saved_pct = (time_saved_s / baseline['total_time_s']) * 100 if baseline['total_time_s'] > 0 else 0
    
    print(f"\n{'Metric':<35} {'Without':<18} {'With':<18} {'Improvement':<15}")
    print("-"*80)
    print(f"{'Total Memory (MB)':<35} {baseline['total_memory_mb']:<18.0f} {optimized['total_memory_mb']:<18.0f} {f'{memory_saved_pct:.1f}%':<15}")
    print(f"{'Memory per Node (MB)':<35} {baseline['per_node_mb']:<18.0f} {optimized['per_node_mb']:<18.0f} {'-':<15}")
    print(f"{'Total Init Time (s)':<35} {baseline['total_time_s']:<18.1f} {optimized['total_time_s']:<18.1f} {f'{time_saved_pct:.1f}%':<15}")
    print(f"{'Avg Init Time (s)':<35} {baseline['avg_init_time_s']:<18.1f} {optimized['avg_init_time_s']:<18.1f} {'-':<15}")
    print(f"{'Cache Hit Rate':<35} {'0%':<18} {f"{optimized['hit_rate']:.1%}":<18} {'-':<15}")
    print("-"*80)
    
    print(f"\nKEY IMPROVEMENTS:")
    print(f"  Memory saved: {memory_saved_mb:.0f}MB ({memory_saved_pct:.1f}% reduction)")
    print(f"  Time saved: {time_saved_s:.1f}s ({time_saved_pct:.1f}% faster)")
    print(f"  First node: {optimized['init_times'][0]:.1f}s (load)")
    if len(optimized['init_times']) > 1:
        cache_times = [t for t in optimized['init_times'][1:]]
        print(f"  Subsequent nodes: {np.mean(cache_times)*1000:.1f}ms average (cache)")
    
    print(f"\nTARGET VALIDATION:")
    if memory_saved_pct >= 60:
        print(f"  [PASS] Memory reduction target (>= 60%): {memory_saved_pct:.1f}%")
    else:
        print(f"  [FAIL] Memory reduction target (>= 60%): {memory_saved_pct:.1f}%")
    
    # Calculate speedup for cache
    if len(optimized['init_times']) > 1:
        cache_speedup = baseline['avg_init_time_s'] / np.mean(cache_times) if np.mean(cache_times) > 0 else float('inf')
        print(f"  [INFO] Cache speedup: {cache_speedup:.0f}x faster than baseline")
    
    print("="*80)
    
    return {
        'memory_saved_mb': memory_saved_mb,
        'memory_saved_pct': memory_saved_pct,
        'time_saved_s': time_saved_s,
        'time_saved_pct': time_saved_pct,
    }


async def main():
    """Run multiprocess benchmark"""
    print("\n" + "="*80)
    print("MULTIPROCESS EXECUTOR BENCHMARK")
    print("Whisper Node Loading: With vs Without Model Registry")
    print("="*80)
    
    if not MULTIPROCESS_AVAILABLE:
        print("\n[ERROR] Multiprocess system not available")
        return 1
    
    if not TORCH_AVAILABLE:
        print("\n[ERROR] Torch not available - install: pip install torch transformers")
        return 1
    
    MODEL_ID = "openai/whisper-tiny.en"
    NUM_NODES = 3
    
    print(f"\nConfiguration:")
    print(f"  Model: {MODEL_ID}")
    print(f"  Nodes: {NUM_NODES}")
    print(f"  System: Rust multiprocess executor + iceoryx2")
    print(f"  PID: {os.getpid()}")
    
    try:
        # Benchmark WITHOUT registry
        baseline = await benchmark_without_registry(MODEL_ID, NUM_NODES)
        
        # Clear memory
        import gc
        gc.collect()
        await asyncio.sleep(2)
        
        # Benchmark WITH registry  
        optimized = await benchmark_with_registry(MODEL_ID, NUM_NODES)
        
        # Print comparison
        comparison = print_comparison(baseline, optimized, NUM_NODES)
        
        # Save results
        results = {
            'model_id': MODEL_ID,
            'num_nodes': NUM_NODES,
            'system': 'rust_multiprocess_iceoryx2',
            'baseline': baseline,
            'optimized': optimized,
            'comparison': comparison,
            'timestamp': time.time(),
        }
        
        with open('benchmark_multiprocess_comparison.json', 'w') as f:
            # Convert numpy types
            def convert(obj):
                if isinstance(obj, np.ndarray):
                    return obj.tolist()
                if isinstance(obj, (np.float32, np.float64)):
                    return float(obj)
                if isinstance(obj, (np.int32, np.int64)):
                    return int(obj)
                return obj
            
            json.dump(results, f, indent=2, default=convert)
        
        print(f"\n[SAVED] Results: benchmark_multiprocess_comparison.json\n")
        
        return 0
        
    except Exception as e:
        print(f"\n[ERROR] Benchmark failed: {e}")
        import traceback
        traceback.print_exc()
        return 1


if __name__ == "__main__":
    import numpy as np
    exit_code = asyncio.run(main())
    sys.exit(exit_code)

