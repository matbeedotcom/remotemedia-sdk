# Getting Started Examples

Welcome to your RemoteMedia SDK journey! These examples are designed for beginners with step-by-step tutorials.

## Philosophy

**Learn by doing**: Each example builds on the previous one, introducing new concepts gradually.

**Time commitment**: 5-15 minutes per example
**Prerequisites**: Basic Python knowledge
**Goal**: Understand core concepts and run your first pipeline

---

## Learning Path

Follow these examples in order for the best learning experience:

### 1. Hello Pipeline (5 minutes)
**Path**: [01-hello-pipeline/](01-hello-pipeline/)

Your first RemoteMedia pipeline. Learn the basics of pipeline manifests and node connections.

**You'll learn**:
- Pipeline manifest format (YAML)
- How to define nodes and connections
- Running a simple audio processing pipeline
- Understanding pipeline outputs

**Prerequisites**: None - start here!

---

### 2. Basic Audio Processing (10 minutes)
**Path**: [02-basic-audio/](02-basic-audio/)

Process audio files with resampling and format conversion using native Rust nodes.

**You'll learn**:
- Audio data types and formats
- Resampling with rubato (high-quality)
- Format conversion (PCM, samples, etc.)
- Reading and writing audio files

**Prerequisites**: Completed Hello Pipeline

---

### 3. Python-Rust Interop (15 minutes)
**Path**: [03-python-rust-interop/](03-python-rust-interop/)

Combine Python flexibility with Rust performance by mixing node types in one pipeline.

**You'll learn**:
- When to use Rust vs Python nodes
- Performance differences
- Zero-copy data transfer with numpy
- Runtime selection (Python/Rust/Auto)

**Prerequisites**: Completed Basic Audio Processing

---

## What's Next?

After completing these examples, you're ready for:

**Advanced Examples** → [../01-advanced/](../01-advanced/)
- Multiprocess execution
- Streaming pipelines
- Custom transports

**Real Applications** → [../02-applications/](../02-applications/)
- Full-stack web apps
- Production deployments

---

## Common Setup

### System Requirements
- Python 3.9+
- 2GB RAM
- Linux, macOS, or Windows

### Install SDK
```bash
pip install remotemedia>=0.4.0
```

### Verify Installation
```bash
python -c "import remotemedia; print(remotemedia.__version__)"
```

Expected output: `0.4.0` or higher

---

## Running Your First Example

```bash
# Navigate to first example
cd 01-hello-pipeline/

# Read the README
cat README.md

# Install dependencies
pip install -r requirements.txt

# Run it!
python main.py
```

---

## Getting Help

**Stuck?** Check:
1. Example README troubleshooting section
2. [Main documentation](../../docs/)
3. [GitHub Issues](https://github.com/org/remotemedia-sdk/issues)

**Common first-time issues**:
- **"Module not found"**: Run `pip install remotemedia`
- **"Audio file not found"**: Some examples download sample audio automatically
- **"Rust runtime unavailable"**: SDK falls back to Python (works, but slower)

---

## Tips for Success

✅ **Follow the order** - Examples build on each other
✅ **Read the full README** - Don't skip prerequisites or setup steps
✅ **Run the code** - Hands-on learning is most effective
✅ **Experiment** - Try modifying parameters and see what happens
✅ **Ask questions** - Use GitHub Issues if you get stuck

---

**Ready to start?** → [01-hello-pipeline/](01-hello-pipeline/)

**Last Updated**: 2025-11-07
**SDK Version**: v0.4.0+
