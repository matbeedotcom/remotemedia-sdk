# Development Scripts

Utility scripts organized by purpose for RemoteMedia SDK development.

## Directory Structure

```
scripts/
├── build/      # Build and compilation scripts
├── debug/      # Debugging and diagnostic tools
├── test/       # Test runners and test utilities
└── validate/   # Repository validation scripts
```

## Build Scripts

**Location**: [build/](build/)

Scripts for building components, dependencies, and compilation tasks.

| Script | Purpose | Usage |
|--------|---------|-------|
| [build-whisper-wasi.sh](build/build-whisper-wasi.sh) | Build Whisper.cpp for WASM/WASI | `./scripts/build/build-whisper-wasi.sh` |

## Debug Scripts

**Location**: [debug/](debug/)

Tools for debugging runtime issues, inspecting system state, and diagnostics.

| Script | Purpose | Usage |
|--------|---------|-------|
| [debug_iox2_services.py](debug/debug_iox2_services.py) | Debug iceoryx2 IPC services and channels | `python scripts/debug/debug_iox2_services.py` |

## Test Scripts

**Location**: [test/](test/)

Test runners, integration test utilities, and test automation.

| Script | Purpose | Usage |
|--------|---------|-------|
| [run_ipc_integration_test.sh](test/run_ipc_integration_test.sh) | Run IPC integration tests | `./scripts/test/run_ipc_integration_test.sh` |

## Validation Scripts

**Location**: [validate/](validate/)

Repository structure validation and compliance checking.

| Script | Purpose | Usage |
|--------|---------|-------|
| [check-repo-structure.sh](validate/check-repo-structure.sh) | Validate root directory structure | `./scripts/validate/check-repo-structure.sh` |
| [check-examples.sh](validate/check-examples.sh) | Validate example documentation | `./scripts/validate/check-examples.sh` |
| [check-docs.sh](validate/check-docs.sh) | Validate documentation completeness | `./scripts/validate/check-docs.sh` |

## Adding New Scripts

When adding a new development script:

1. **Choose the right directory** based on purpose:
   - `build/` - Compilation, dependency building
   - `debug/` - Runtime diagnostics, inspection
   - `test/` - Test execution, test utilities
   - `validate/` - Structure/compliance checking

2. **Make it executable**:
   ```bash
   chmod +x scripts/[purpose]/your-script.sh
   ```

3. **Add documentation**:
   - Add entry to this README
   - Include usage comments in the script
   - Document required dependencies

4. **Follow conventions**:
   - Use clear, descriptive names
   - Include error handling
   - Output clear success/failure messages

## Common Workflows

### Building Components
```bash
# Build WASM components
./scripts/build/build-whisper-wasi.sh

# Build Rust runtime
cd runtime-core && cargo build --release
```

### Running Tests
```bash
# Run all Python tests
pytest

# Run specific integration tests
./scripts/test/run_ipc_integration_test.sh

# Validate repository structure
./scripts/validate/check-repo-structure.sh
```

### Debugging Issues
```bash
# Check iceoryx2 IPC state
python scripts/debug/debug_iox2_services.py

# View runtime logs
tail -f runtime.log
```

## CI/CD Integration

Many scripts are used in CI/CD pipelines:

- **Validation scripts** run on every PR ([repo-structure-validation.yml](../.github/workflows/repo-structure-validation.yml))
- **Test scripts** run in test workflows
- **Build scripts** used in release builds

See [.github/workflows/](../.github/workflows/) for workflow definitions.

## Getting Help

- **Script-specific help**: Run script with `--help` flag (if supported)
- **General questions**: See [CONTRIBUTING.md](../CONTRIBUTING.md)
- **Issues**: [GitHub Issues](https://github.com/org/remotemedia-sdk/issues)

---

**Last Updated**: 2025-11-07
**Repository Version**: v0.4.0+
