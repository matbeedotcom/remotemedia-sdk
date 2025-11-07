"""
Demo of Model Registry for efficient model sharing.

This example demonstrates how multiple nodes can share a single model instance,
reducing memory usage and initialization time.
"""

from remotemedia.core import ModelRegistry, get_or_load
import time
import sys

class DummyModel:
    """A simple dummy model for demonstration"""
    
    def __init__(self, model_name: str, size_mb: int = 100):
        self.model_name = model_name
        self.size_mb = size_mb
        print(f"  🔄 Loading {model_name} ({size_mb}MB)...")
        time.sleep(0.5)  # Simulate loading time
        print(f"  ✅ {model_name} loaded!")
    
    def memory_usage(self) -> int:
        """Return memory usage in bytes"""
        return self.size_mb * 1024 * 1024
    
    def predict(self, text: str) -> str:
        """Simulate inference"""
        return f"{self.model_name}: Processed '{text}'"


def demo_basic_sharing():
    """Demonstrate basic model sharing"""
    print("\n" + "="*60)
    print("Demo 1: Basic Model Sharing")
    print("="*60)
    
    registry = ModelRegistry()
    
    print("\n1️⃣  First node requests 'whisper-base':")
    def load_whisper():
        return DummyModel("whisper-base", size_mb=150)
    
    handle1 = registry.get_or_load("whisper-base", load_whisper)
    model1 = handle1.model
    result1 = model1.predict("Hello world")
    print(f"   Result: {result1}")
    
    print("\n2️⃣  Second node requests same 'whisper-base':")
    handle2 = registry.get_or_load("whisper-base", load_whisper)
    model2 = handle2.model
    result2 = model2.predict("Bonjour monde")
    print(f"   Result: {result2}")
    
    print("\n🎯 Verification:")
    print(f"   Same model instance? {model1 is model2}")
    print(f"   Memory saved: {model1.memory_usage() / 1024**2:.0f}MB")
    
    metrics = registry.metrics()
    print(f"\n📊 Metrics:")
    print(f"   Cache hits: {metrics.cache_hits}")
    print(f"   Cache misses: {metrics.cache_misses}")
    print(f"   Hit rate: {metrics.hit_rate:.1%}")


def demo_multiple_models():
    """Demonstrate sharing multiple different models"""
    print("\n" + "="*60)
    print("Demo 2: Multiple Models")
    print("="*60)
    
    registry = ModelRegistry()
    
    # Load different models
    models_config = [
        ("whisper-base", 150),
        ("llama-7b", 7000),
        ("clip-vit", 350),
    ]
    
    for model_name, size_mb in models_config:
        print(f"\n📦 Loading {model_name}...")
        handle = registry.get_or_load(
            model_name,
            lambda n=model_name, s=size_mb: DummyModel(n, s)
        )
        print(f"   ✓ {model_name} ready")
    
    # Access already-loaded models
    print("\n🔄 Accessing cached models...")
    for model_name, _ in models_config:
        start = time.time()
        handle = registry.get_or_load(
            model_name,
            lambda: DummyModel(model_name, 0)  # Won't be called
        )
        elapsed = (time.time() - start) * 1000
        print(f"   ⚡ {model_name}: {elapsed:.1f}ms")
    
    # Show all loaded models
    print("\n📋 Loaded Models:")
    for model_info in registry.list_models():
        print(f"   - {model_info.model_id}: {model_info.memory_bytes / 1024**2:.0f}MB")
    
    metrics = registry.metrics()
    print(f"\n📊 Total Memory: {metrics.total_memory_bytes / 1024**3:.2f}GB")


def demo_convenience_function():
    """Demonstrate the convenience function"""
    print("\n" + "="*60)
    print("Demo 3: Convenience Function")
    print("="*60)
    
    print("\n🎯 Using get_or_load() convenience function:")
    
    # First call - loads the model
    print("\n1️⃣  First call:")
    model1 = get_or_load("my-model", lambda: DummyModel("my-model", 200))
    
    # Second call - returns cached instance
    print("\n2️⃣  Second call (instant):")
    start = time.time()
    model2 = get_or_load("my-model", lambda: DummyModel("my-model", 200))
    elapsed = (time.time() - start) * 1000
    
    print(f"\n✅ Same instance: {model1 is model2}")
    print(f"⚡ Access time: {elapsed:.1f}ms")


def main():
    """Run all demos"""
    print("\n" + "🚀 "*20)
    print(" " * 15 + "MODEL REGISTRY DEMO")
    print("🚀 "*20)
    
    try:
        demo_basic_sharing()
        demo_multiple_models()
        demo_convenience_function()
        
        print("\n" + "✨ "*20)
        print(" " * 15 + "ALL DEMOS COMPLETE!")
        print("✨ "*20 + "\n")
        
    except KeyboardInterrupt:
        print("\n\n⚠️  Demo interrupted by user")
        sys.exit(0)
    except Exception as e:
        print(f"\n\n❌ Error: {e}")
        import traceback
        traceback.print_exc()
        sys.exit(1)


if __name__ == "__main__":
    main()

