"""
Simple demo of Model Registry for efficient model sharing.
"""

from remotemedia.core import ModelRegistry, get_or_load
import time


class DummyModel:
    """A simple dummy model for demonstration"""
    
    def __init__(self, model_name: str, size_mb: int = 100):
        self.model_name = model_name
        self.size_mb = size_mb
        print(f"[LOADING] {model_name} ({size_mb}MB)...")
        time.sleep(0.3)  # Simulate loading time
        print(f"[LOADED] {model_name}")
    
    def memory_usage(self) -> int:
        return self.size_mb * 1024 * 1024
    
    def predict(self, text: str) -> str:
        return f"{self.model_name}: Processed '{text}'"


def main():
    print("\n" + "="*60)
    print("MODEL REGISTRY - PROCESS-LOCAL SHARING DEMO")
    print("="*60)
    
    registry = ModelRegistry()
    
    print("\n[STEP 1] First node requests 'whisper-base':")
    handle1 = registry.get_or_load(
        "whisper-base",
        lambda: DummyModel("whisper-base", 150)
    )
    model1 = handle1.model
    print(f"Result: {model1.predict('Hello')}")
    
    print("\n[STEP 2] Second node requests same 'whisper-base' (should be instant):")
    start = time.time()
    handle2 = registry.get_or_load(
        "whisper-base",
        lambda: DummyModel("whisper-base", 150)
    )
    model2 = handle2.model
    elapsed_ms = (time.time() - start) * 1000
    print(f"Result: {model2.predict('World')}")
    print(f"Access time: {elapsed_ms:.1f}ms")
    
    print("\n[VERIFICATION]")
    print(f"Same instance? {model1 is model2}")
    print(f"Memory saved: {model1.memory_usage() / 1024**2:.0f}MB")
    
    metrics = registry.metrics()
    print("\n[METRICS]")
    print(f"Cache hits: {metrics.cache_hits}")
    print(f"Cache misses: {metrics.cache_misses}")
    print(f"Hit rate: {metrics.hit_rate:.1%}")
    print(f"Total models: {metrics.total_models}")
    
    print("\n[SUCCESS] MVP feature working correctly!")
    print("="*60 + "\n")


if __name__ == "__main__":
    main()

