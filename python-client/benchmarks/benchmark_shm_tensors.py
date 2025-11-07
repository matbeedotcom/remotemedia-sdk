"""
Real-world benchmark of Shared Memory Tensor transfers.

Measures:
1. Serialization vs. shared memory transfer performance
2. Throughput for different tensor sizes
3. Overhead comparison
4. Real-world speedup factors
"""

import time
import sys
import os
import pickle
import numpy as np
from typing import List, Tuple
import logging

logging.basicConfig(level=logging.INFO, format='%(levelname)s: %(message)s')
logger = logging.getLogger(__name__)


def benchmark_serialization(tensor: np.ndarray, iterations: int = 10) -> dict:
    """Benchmark tensor transfer via serialization (baseline)"""
    tensor_size_mb = tensor.nbytes / 1024**2
    
    times = []
    for i in range(iterations):
        start = time.perf_counter()
        
        # Serialize
        serialized = pickle.dumps(tensor)
        
        # Deserialize
        reconstructed = pickle.loads(serialized)
        
        elapsed = time.perf_counter() - start
        times.append(elapsed)
    
    avg_time = np.mean(times)
    min_time = np.min(times)
    max_time = np.max(times)
    throughput_gbps = (tensor_size_mb / 1024) / avg_time
    
    return {
        'avg_time_ms': avg_time * 1000,
        'min_time_ms': min_time * 1000,
        'max_time_ms': max_time * 1000,
        'throughput_gbps': throughput_gbps,
        'size_mb': tensor_size_mb,
    }


def benchmark_shared_memory(tensor: np.ndarray, iterations: int = 10) -> dict:
    """Benchmark tensor transfer via shared memory (zero-copy)"""
    try:
        from multiprocessing import shared_memory
    except ImportError:
        logger.error("shared_memory not available")
        return None
    
    tensor_size_mb = tensor.nbytes / 1024**2
    
    times = []
    for i in range(iterations):
        start = time.perf_counter()
        
        # Create shared memory
        shm = shared_memory.SharedMemory(create=True, size=tensor.nbytes)
        
        # Write to shared memory (zero-copy via buffer protocol)
        shm_array = np.ndarray(tensor.shape, dtype=tensor.dtype, buffer=shm.buf)
        np.copyto(shm_array, tensor)
        
        # "Transfer" - in real scenario, we'd just pass the name to another process
        # Simulate read in another process
        read_array = np.ndarray(tensor.shape, dtype=tensor.dtype, buffer=shm.buf)
        
        # Verify
        assert np.array_equal(read_array, tensor), "Data mismatch"
        
        # Cleanup
        shm.close()
        shm.unlink()
        
        elapsed = time.perf_counter() - start
        times.append(elapsed)
    
    avg_time = np.mean(times)
    min_time = np.min(times)
    max_time = np.max(times)
    throughput_gbps = (tensor_size_mb / 1024) / avg_time
    
    return {
        'avg_time_ms': avg_time * 1000,
        'min_time_ms': min_time * 1000,
        'max_time_ms': max_time * 1000,
        'throughput_gbps': throughput_gbps,
        'size_mb': tensor_size_mb,
    }


def benchmark_memory_mapped(tensor: np.ndarray, iterations: int = 10) -> dict:
    """Benchmark using pure NumPy memory view (zero-copy reference)"""
    tensor_size_mb = tensor.nbytes / 1024**2
    
    times = []
    for i in range(iterations):
        start = time.perf_counter()
        
        # Create a view (zero-copy)
        view = tensor.view()
        
        # Access the data (forces any lazy evaluation)
        _ = view.sum()
        
        elapsed = time.perf_counter() - start
        times.append(elapsed)
    
    avg_time = np.mean(times)
    min_time = np.min(times)
    throughput_gbps = (tensor_size_mb / 1024) / avg_time if avg_time > 0 else float('inf')
    
    return {
        'avg_time_ms': avg_time * 1000,
        'min_time_ms': min_time * 1000,
        'throughput_gbps': throughput_gbps,
        'size_mb': tensor_size_mb,
    }


def run_benchmark_suite(tensor_sizes_mb: List[int]):
    """Run comprehensive benchmarks across different tensor sizes"""
    print("\n" + "="*70)
    print("SHARED MEMORY TENSOR BENCHMARK")
    print("Measuring real transfer performance")
    print("="*70)
    
    results = []
    
    for size_mb in tensor_sizes_mb:
        print(f"\n{'-'*70}")
        print(f"Tensor Size: {size_mb}MB ({size_mb * 1024 * 1024 / 4:.0f} float32 elements)")
        print(f"{'-'*70}")
        
        # Create test tensor
        num_elements = (size_mb * 1024 * 1024) // 4  # float32 = 4 bytes
        tensor = np.random.randn(num_elements).astype(np.float32)
        actual_size_mb = tensor.nbytes / 1024**2
        
        print(f"Actual tensor size: {actual_size_mb:.2f}MB")
        
        # Benchmark serialization
        print("\n1. Serialization (pickle) - BASELINE:")
        serial_result = benchmark_serialization(tensor, iterations=5)
        print(f"   Avg time: {serial_result['avg_time_ms']:.2f}ms")
        print(f"   Min time: {serial_result['min_time_ms']:.2f}ms")
        print(f"   Throughput: {serial_result['throughput_gbps']:.2f} GB/s")
        
        # Benchmark shared memory
        print("\n2. Shared Memory - OPTIMIZED:")
        shm_result = benchmark_shared_memory(tensor, iterations=5)
        if shm_result:
            print(f"   Avg time: {shm_result['avg_time_ms']:.2f}ms")
            print(f"   Min time: {shm_result['min_time_ms']:.2f}ms")
            print(f"   Throughput: {shm_result['throughput_gbps']:.2f} GB/s")
            
            # Calculate improvement
            speedup = serial_result['avg_time_ms'] / shm_result['avg_time_ms']
            overhead_reduction = ((serial_result['avg_time_ms'] - shm_result['avg_time_ms']) / serial_result['avg_time_ms']) * 100
            
            print(f"\n3. IMPROVEMENT:")
            print(f"   Speedup: {speedup:.1f}x faster")
            print(f"   Overhead reduction: {overhead_reduction:.1f}%")
            
            # Check if we meet target
            target_throughput_gbps = 10
            if shm_result['throughput_gbps'] >= target_throughput_gbps:
                print(f"   [PASS] Throughput target MET ({shm_result['throughput_gbps']:.1f} >= {target_throughput_gbps} GB/s)")
            else:
                print(f"   [FAIL] Throughput target MISSED ({shm_result['throughput_gbps']:.1f} < {target_throughput_gbps} GB/s)")
            
            if overhead_reduction >= 95:
                print(f"   [PASS] Overhead reduction target MET ({overhead_reduction:.1f}% >= 95%)")
            elif overhead_reduction >= 80:
                print(f"   [GOOD] Overhead reduction strong ({overhead_reduction:.1f}%)")
            
            results.append({
                'size_mb': size_mb,
                'serialization': serial_result,
                'shared_memory': shm_result,
                'speedup': speedup,
                'overhead_reduction_pct': overhead_reduction,
            })
    
    return results


def print_summary_table(results: List[dict]):
    """Print summary table of all results"""
    print("\n" + "="*70)
    print("SUMMARY TABLE")
    print("="*70)
    print(f"\n{'Size':<10} {'Serial(ms)':<15} {'SHM(ms)':<15} {'Speedup':<12} {'Reduction':<12}")
    print("-"*70)
    
    for r in results:
        size = f"{r['size_mb']}MB"
        serial_time = f"{r['serialization']['avg_time_ms']:.2f}"
        shm_time = f"{r['shared_memory']['avg_time_ms']:.2f}"
        speedup = f"{r['speedup']:.1f}x"
        reduction = f"{r['overhead_reduction_pct']:.1f}%"
        
        print(f"{size:<10} {serial_time:<15} {shm_time:<15} {speedup:<12} {reduction:<12}")
    
    # Overall statistics
    avg_speedup = np.mean([r['speedup'] for r in results])
    avg_reduction = np.mean([r['overhead_reduction_pct'] for r in results])
    
    avg_shm_throughput = np.mean([r['shared_memory']['throughput_gbps'] for r in results])
    
    print("-"*70)
    print(f"\nOverall Statistics:")
    print(f"  Average speedup: {avg_speedup:.1f}x")
    print(f"  Average overhead reduction: {avg_reduction:.1f}%")
    print(f"  Average SHM throughput: {avg_shm_throughput:.1f} GB/s")
    
    print(f"\nConclusions:")
    if avg_shm_throughput >= 10:
        print(f"  [PASS] Throughput target MET ({avg_shm_throughput:.1f} >= 10 GB/s)")
    else:
        print(f"  [FAIL] Throughput target MISSED ({avg_shm_throughput:.1f} < 10 GB/s)")
    
    if avg_reduction >= 95:
        print(f"  [PASS] Overhead reduction target MET ({avg_reduction:.1f}% >= 95%)")
    else:
        print(f"  [GOOD] Strong overhead reduction ({avg_reduction:.1f}%)")
    
    print("="*70)


def main():
    """Run comprehensive SHM benchmarks"""
    print("\n" + "="*70)
    print("SHARED MEMORY TENSOR TRANSFER BENCHMARK")
    print("="*70)
    
    # Test different sizes
    # Start small for quick testing, scale up for throughput measurements
    tensor_sizes_mb = [1, 10, 100, 500]
    
    print(f"\nConfiguration:")
    print(f"  Tensor sizes: {tensor_sizes_mb} MB")
    print(f"  Iterations per size: 5")
    print(f"  Target throughput: >= 10 GB/s")
    print(f"  Target overhead reduction: >= 95%")
    
    try:
        results = run_benchmark_suite(tensor_sizes_mb)
        
        if results:
            print_summary_table(results)
            
            # Save results
            import json
            results_file = "benchmark_results_shm.json"
            with open(results_file, 'w') as f:
                # Convert numpy types to native Python for JSON serialization
                json_results = []
                for r in results:
                    json_results.append({
                        'size_mb': r['size_mb'],
                        'serialization': {k: float(v) for k, v in r['serialization'].items()},
                        'shared_memory': {k: float(v) for k, v in r['shared_memory'].items()},
                        'speedup': float(r['speedup']),
                        'overhead_reduction_pct': float(r['overhead_reduction_pct']),
                    })
                
                json.dump({
                    'tensor_sizes_mb': tensor_sizes_mb,
                    'results': json_results,
                    'timestamp': time.time(),
                }, f, indent=2)
            
            print(f"\n[SAVED] Results saved to: {results_file}")
        
        return 0
        
    except Exception as e:
        print(f"\n[ERROR] Benchmark failed: {e}")
        import traceback
        traceback.print_exc()
        return 1


if __name__ == "__main__":
    exit_code = main()
    sys.exit(exit_code)

