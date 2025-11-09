# Implement PyO3 WASM Browser Runtime

## Quick Summary

Compile the RemoteMedia Rust runtime to WebAssembly (`wasm32-wasi`) with embedded CPython (PyO3 + libpython) to enable Pipeline execution in web browsers, following the VMware Labs `wasi-py-rs-pyo3` reference implementation.

## Status

**Status**: Proposal Created
**Phase**: Approved
**Created**: 2025-01-24

## What This Adds

### Phase 1: Local WASM Execution (MVP)
- ✅ `wasm32-wasi` compilation target for Rust runtime
- ✅ Static linking of CPython via `wlr-libpy`
- ✅ WASM-compatible numpy marshaling (JSON+base64)
- ✅ Synchronous execution path for WASM
- ✅ Local testing with wasmtime

### Phase 2: Browser Integration
- ✅ TypeScript integration layer (@wasmer/sdk)
- ✅ .rmpkg packaging format for WASM bundles
- ✅ Browser demo application
- ✅ WASI filesystem integration

### Phase 3: Whisper WASM (Optional)
- ✅ Whisper.cpp WASM compilation
- ✅ Browser-based audio transcription

## Files

- [`proposal.md`](./proposal.md) - Full proposal with motivation, scope, and success criteria
- [`tasks.md`](./tasks.md) - Detailed implementation checklist (77 tasks)
- [`design.md`](./design.md) - Technical architecture and design decisions
- [`specs/pyo3-wasm-browser/spec.md`](./specs/pyo3-wasm-browser/spec.md) - Requirement specifications

## Key Design Decisions

1. **wasm32-wasi target**: More portable than Emscripten (works in Node.js, Deno, Cloudflare Workers, browser)
2. **PyO3 + libpython**: Use VMware Labs pre-built static library instead of custom compilation
3. **Sync execution**: Use `futures::executor::block_on` instead of tokio (WASM limitation)
4. **Base64 numpy**: JSON+base64 serialization instead of zero-copy rust-numpy (not available in WASM)
5. **@wasmer/sdk**: Better WASI filesystem support than alternatives

## Dependencies

### External
- VMware Labs `wlr-libpy` crate (provides libpython3.11.a for wasm32-wasi)
- PyO3 0.26 with `abi3-py311` feature
- @wasmer/sdk TypeScript library (browser runtime)

### Internal
- Existing Rust runtime (executor, manifest, nodes)
- RustPython VM (already supports WASM)
- Data marshaling infrastructure

## Timeline

**Estimated Total**: 22-30 hours

- Phase 1 (MVP): 8-10 hours
- Phase 2 (Browser): 6-8 hours
- Phase 3 (Whisper): 8-12 hours (optional)

## Success Metrics

### Phase 1
- [ ] Builds successfully: `cargo build --target wasm32-wasi`
- [ ] Runs in wasmtime: Math pipeline (Multiply → Add)
- [ ] Output matches native runtime (bit-identical)

### Phase 2
- [ ] Loads .rmpkg bundle in browser
- [ ] Executes pipeline from JavaScript
- [ ] Live browser demo deployed

### Phase 3
- [ ] Whisper transcription works in WASM
- [ ] Performance < 2x native runtime
- [ ] Full audio pipeline demo

## How to Use (After Implementation)

### Build WASM Binary
```bash
cargo build --target wasm32-wasi --bin pipeline_executor_wasm --release
```

### Run Locally
```bash
wasmtime \
  --mapdir /usr::target/wasm32-wasi/wasi-deps/usr \
  target/wasm32-wasi/release/pipeline_executor_wasm.wasm \
  < manifest.json
```

### Use in Browser
```typescript
import { PipelineRunner } from '@remotemedia/browser-runtime';

const runner = new PipelineRunner();
await runner.initialize('https://example.com/voice_pipeline.rmpkg');

const result = await runner.execute(manifestJson);
console.log('Pipeline result:', result);
```

## Related Changes

- `refactor-language-neutral-runtime`: Parent change providing Rust runtime foundation

## Related Specs

- `wasm-sandbox`: General WASM sandboxing requirements (this implements subset)
- `pipeline-packaging`: .rmpkg format (this extends for WASM bundles)

## References

- VMware Labs PyO3 WASM Example: https://github.com/vmware-labs/webassembly-language-runtimes/tree/main/python/examples/embedding/wasi-py-rs-pyo3
- PyO3 Documentation: https://pyo3.rs/v0.26.0/
- WASI Specification: https://github.com/WebAssembly/WASI
- @wasmer/sdk: https://wasmer.io/

## Questions or Feedback

Please review the proposal and provide feedback on:
1. Async execution strategy (sync vs wasm-bindgen-futures)
2. Bundle size concerns (~15-20MB for MVP)
3. Browser compatibility requirements
4. Performance expectations (1.5-2x native acceptable?)

---

**Ready for Review**: ✅ Yes
**Validation**: ✅ Passed `openspec validate --strict`
**Next Step**: Stakeholder approval
