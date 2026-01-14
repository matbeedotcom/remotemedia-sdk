# Cross-Language Zero-Copy IPC Architecture

## Overview

This document outlines the architecture for implementing zero-copy inter-process communication (IPC) between Python, Rust, and Node.js using IceOryx2 as the shared memory foundation.

## Table of Contents

1. [Core Concepts](#core-concepts)
2. [IceOryx2 Fundamentals](#iceoryx2-fundamentals)
3. [Shared Memory Layout](#shared-memory-layout)
4. [Language Bindings](#language-bindings)
5. [Implementation Guide](#implementation-guide)
6. [Performance Characteristics](#performance-characteristics)
7. [Safety Considerations](#safety-considerations)
8. [References](#references)

---

## Core Concepts

### What is Zero-Copy IPC?

Zero-copy IPC eliminates data copying between processes by having all participants access the same physical memory region. Instead of serializing data and copying it through pipes/sockets:

1. Producer writes data directly into shared memory
2. Producer sends only an **8-byte offset** to consumers
3. Consumers reconstruct pointer: `local_ptr = base_address + offset`
4. All processes read/write the same physical bytes

### Why Offsets Instead of Pointers?

Each process has its own virtual address space. The same physical memory maps to **different virtual addresses** in each process:

```
Physical Memory:    [  Data at 0x1000  ]
                           |
        +------------------+------------------+
        |                  |                  |
Process A: 0x7f001000  Process B: 0x7f002000  Process C: 0x7f003000
```

IceOryx2 uses **relative pointers** (offsets from segment base) that work regardless of where memory is mapped.

---

## IceOryx2 Fundamentals

### Architecture Layers

```
┌─────────────────────────────────────────────────────────────┐
│                    Application Layer                        │
│         (Rust / Python / C++ / C / Node.js)                │
├─────────────────────────────────────────────────────────────┤
│                    Service Layer                            │
│    Publish-Subscribe | Request-Response | Events | Blackboard│
├─────────────────────────────────────────────────────────────┤
│                    Port Layer                               │
│         Publisher | Subscriber | Client | Server            │
├─────────────────────────────────────────────────────────────┤
│                Communication Abstraction Layer (CAL)        │
│         Shared Memory | Lock-free Queues | Events           │
├─────────────────────────────────────────────────────────────┤
│                    POSIX / OS Layer                         │
│              shm_open | mmap | futex | eventfd              │
└─────────────────────────────────────────────────────────────┘
```

### File System Layout

IceOryx2 uses two locations for its data:

```
/dev/shm/                              # Shared memory segments (Linux)
├── iox2_<service_hash>_data           # Payload data pool
├── iox2_<service_hash>_mgmt           # Management structures
└── iox2_<node_hash>_...               # Per-node resources

/tmp/iceoryx2/                         # Configuration and discovery
├── services/
│   ├── <service_name>.toml            # Service static config
│   └── ...
├── nodes/
│   └── <node_id>/                     # Node metadata
└── config.toml                        # Global configuration
```

### Service Discovery

Services are identified by name and discovered via filesystem:

1. Creator writes service config to `/tmp/iceoryx2/services/<name>.toml`
2. Participants read config to find shared memory segment names
3. All participants `shm_open()` the same segments
4. Type safety enforced via type name + size + alignment matching

### Messaging Patterns

| Pattern | Description | Use Case |
|---------|-------------|----------|
| **Publish-Subscribe** | 1-to-N async messaging | Sensor data, logs, events |
| **Request-Response** | Bidirectional RPC | Service calls, queries |
| **Events** | Lightweight notifications | Signaling, triggers |
| **Blackboard** | Shared key-value store | Configuration, state |

---

## Shared Memory Layout

### Critical: `#[repr(C)]` Requirement

For cross-language zero-copy, struct memory layout must be **identical** across all languages. Rust's default layout is not guaranteed stable.

```rust
// WRONG - Rust may reorder/pad arbitrarily
struct BadExample {
    a: u8,
    b: u64,
    c: u16,
}

// CORRECT - C ABI guarantees stable layout
#[repr(C)]
struct GoodExample {
    a: u8,
    _pad1: [u8; 7],  // Explicit padding to align b
    b: u64,
    c: u16,
    _pad2: [u8; 6],  // Pad to 8-byte boundary
}
```

### Alignment Rules

| Type | Size | Alignment |
|------|------|-----------|
| `u8` / `i8` | 1 | 1 |
| `u16` / `i16` | 2 | 2 |
| `u32` / `i32` / `f32` | 4 | 4 |
| `u64` / `i64` / `f64` | 8 | 8 |
| `u128` / `i128` | 16 | 16 |
| Struct | Sum of fields + padding | Max alignment of fields |

### Example: Shared Data Structure

**Rust Definition (canonical)**

```rust
/// Shared sensor data - identical layout across all languages
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct SensorData {
    /// Nanoseconds since Unix epoch
    pub timestamp_ns: u64,      // offset 0, size 8, align 8

    /// Temperature in Celsius
    pub temperature: f64,       // offset 8, size 8, align 8

    /// Atmospheric pressure in hPa
    pub pressure: f64,          // offset 16, size 8, align 8

    /// Relative humidity percentage (0-100)
    pub humidity: f32,          // offset 24, size 4, align 4

    /// Sensor status flags
    pub status: u32,            // offset 28, size 4, align 4

    /// Sensor identifier
    pub sensor_id: [u8; 16],    // offset 32, size 16, align 1

    /// Reserved for future use
    pub _reserved: [u8; 8],     // offset 48, size 8, align 1
}
// Total size: 56 bytes, alignment: 8

// Compile-time verification
const _: () = assert!(std::mem::size_of::<SensorData>() == 56);
const _: () = assert!(std::mem::align_of::<SensorData>() == 8);
```

**Python Definition (ctypes)**

```python
import ctypes

class SensorData(ctypes.Structure):
    _pack_ = 8  # Match Rust alignment
    _fields_ = [
        ("timestamp_ns", ctypes.c_uint64),      # offset 0
        ("temperature", ctypes.c_double),        # offset 8
        ("pressure", ctypes.c_double),           # offset 16
        ("humidity", ctypes.c_float),            # offset 24
        ("status", ctypes.c_uint32),             # offset 28
        ("sensor_id", ctypes.c_uint8 * 16),      # offset 32
        ("_reserved", ctypes.c_uint8 * 8),       # offset 48
    ]

# Verification
assert ctypes.sizeof(SensorData) == 56
```

**Node.js Definition (DataView)**

```typescript
// Type definition
interface SensorData {
  timestampNs: bigint;      // offset 0, 8 bytes
  temperature: number;       // offset 8, 8 bytes
  pressure: number;          // offset 16, 8 bytes
  humidity: number;          // offset 24, 4 bytes
  status: number;            // offset 28, 4 bytes
  sensorId: Uint8Array;      // offset 32, 16 bytes
  // _reserved at offset 48, 8 bytes (ignored)
}

const SENSOR_DATA_SIZE = 56;

// Zero-copy reader
function readSensorData(buffer: Buffer, offset: number = 0): SensorData {
  const view = new DataView(buffer.buffer, buffer.byteOffset + offset);
  return {
    timestampNs: view.getBigUint64(0, true),   // little-endian
    temperature: view.getFloat64(8, true),
    pressure: view.getFloat64(16, true),
    humidity: view.getFloat32(24, true),
    status: view.getUint32(28, true),
    sensorId: new Uint8Array(buffer.buffer, buffer.byteOffset + offset + 32, 16),
  };
}

// Zero-copy writer
function writeSensorData(buffer: Buffer, data: SensorData, offset: number = 0): void {
  const view = new DataView(buffer.buffer, buffer.byteOffset + offset);
  view.setBigUint64(0, data.timestampNs, true);
  view.setFloat64(8, data.temperature, true);
  view.setFloat64(16, data.pressure, true);
  view.setFloat32(24, data.humidity, true);
  view.setUint32(28, data.status, true);
  new Uint8Array(buffer.buffer, buffer.byteOffset + offset + 32, 16).set(data.sensorId);
}
```

**C/C++ Definition**

```c
#include <stdint.h>

typedef struct __attribute__((packed, aligned(8))) {
    uint64_t timestamp_ns;      // offset 0
    double   temperature;       // offset 8
    double   pressure;          // offset 16
    float    humidity;          // offset 24
    uint32_t status;            // offset 28
    uint8_t  sensor_id[16];     // offset 32
    uint8_t  _reserved[8];      // offset 48
} SensorData;

_Static_assert(sizeof(SensorData) == 56, "SensorData size mismatch");
_Static_assert(_Alignof(SensorData) == 8, "SensorData alignment mismatch");
```

### Type Safety Protocol

IceOryx2 validates type compatibility at connection time using:

1. **Type name**: String identifier (e.g., "SensorData")
2. **Size**: `sizeof(T)` must match
3. **Alignment**: `alignof(T)` must match

```rust
// IceOryx2 internally checks:
if subscriber.type_name() != publisher.type_name() ||
   subscriber.type_size() != publisher.type_size() ||
   subscriber.type_alignment() != publisher.type_alignment() {
    return Err(IncompatibleTypes);
}
```

---

## Language Bindings

### Rust (Native - iceoryx2 crate)

```toml
# Cargo.toml
[dependencies]
iceoryx2 = "0.7"
```

```rust
use iceoryx2::prelude::*;

// Publisher
let node = NodeBuilder::new().create::<ipc::Service>()?;
let service = node
    .service_builder(&"sensor/data".try_into()?)
    .publish_subscribe::<SensorData>()
    .create()?;
let publisher = service.publisher_builder().create()?;

// Zero-copy publish
let sample = publisher.loan_uninit()?;
let sample = sample.write_payload(SensorData { /* ... */ });
sample.send()?;

// Subscriber
let subscriber = service.subscriber_builder().create()?;
if let Some(sample) = subscriber.receive()? {
    let data: &SensorData = sample.payload();
    // Zero-copy read - data points directly into shared memory
}
```

### Python (iceoryx2 PyPI package)

```bash
pip install iceoryx2
```

```python
import iceoryx2
import ctypes

# Publisher
node = iceoryx2.Node.new("python-publisher")
service = node.service_builder("sensor/data") \
    .publish_subscribe(SensorData) \
    .create()
publisher = service.publisher_builder().create()

# Zero-copy publish
sample = publisher.loan_uninit()
data = SensorData.from_buffer(sample.payload())  # ctypes cast, no copy
data.timestamp_ns = time.time_ns()
data.temperature = 23.5
sample.assume_init().send()

# Subscriber
subscriber = service.subscriber_builder().create()
sample = subscriber.receive()
if sample:
    data = SensorData.from_buffer(sample.payload())  # Zero-copy read
    print(f"Temp: {data.temperature}")
```

### Node.js (napi-rs bindings - to be implemented)

```typescript
// Proposed API
import { Node, ServiceBuilder } from 'iceoryx2-node';

// Publisher
const node = new Node("nodejs-publisher");
const service = node.serviceBuilder("sensor/data")
    .publishSubscribe<SensorData>(SENSOR_DATA_SIZE)
    .create();
const publisher = service.publisherBuilder().create();

// Zero-copy publish
const sample = publisher.loanUninit();
writeSensorData(sample.buffer, {
    timestampNs: BigInt(Date.now()) * 1000000n,
    temperature: 23.5,
    pressure: 1013.25,
    humidity: 65.0,
    status: 0,
    sensorId: new Uint8Array(16),
});
sample.send();

// Subscriber
const subscriber = service.subscriberBuilder().create();
const sample = subscriber.receive();
if (sample) {
    const data = readSensorData(sample.buffer);  // Zero-copy read
    console.log(`Temp: ${data.temperature}`);
}
```

---

## Implementation Guide

### Node.js Bindings via napi-rs

#### Project Structure

```
iceoryx2-node/
├── Cargo.toml
├── src/
│   ├── lib.rs              # Entry point, module exports
│   ├── node.rs             # Node wrapper
│   ├── service.rs          # Service builder wrappers
│   ├── publisher.rs        # Publisher + Sample
│   ├── subscriber.rs       # Subscriber + Sample
│   ├── sample.rs           # Zero-copy buffer handling
│   └── error.rs            # Error types
├── index.js                # JS entry point
├── index.d.ts              # TypeScript definitions
└── package.json
```

#### Cargo.toml

```toml
[package]
name = "iceoryx2-node"
version = "0.1.0"
edition = "2021"

[lib]
crate-type = ["cdylib"]

[dependencies]
napi = { version = "2", features = ["napi6", "serde-json"] }
napi-derive = "2"
iceoryx2 = "0.7"

[build-dependencies]
napi-build = "2"

[profile.release]
lto = true
opt-level = 3
```

#### Core Implementation

```rust
// src/lib.rs
use napi::bindgen_prelude::*;
use napi_derive::napi;

mod node;
mod service;
mod publisher;
mod subscriber;
mod sample;

pub use node::*;
pub use service::*;
pub use publisher::*;
pub use subscriber::*;
pub use sample::*;
```

```rust
// src/sample.rs
use napi::bindgen_prelude::*;
use napi::{Env, JsBuffer, JsBufferValue};
use napi_derive::napi;
use std::marker::PhantomData;

/// Wrapper for received sample with zero-copy buffer access
#[napi]
pub struct ReceivedSample {
    // Store the iceoryx2 sample to keep shared memory reference alive
    inner: Option<iceoryx2::sample::Sample<ipc::Service, [u8], ()>>,
    ptr: *const u8,
    len: usize,
}

#[napi]
impl ReceivedSample {
    /// Get zero-copy buffer view into shared memory
    ///
    /// SAFETY: The returned buffer points directly into shared memory.
    /// Do not use after calling `release()` or after this sample is dropped.
    #[napi]
    pub fn buffer(&self, env: Env) -> Result<JsBuffer> {
        if self.inner.is_none() {
            return Err(Error::from_reason("Sample already released"));
        }

        unsafe {
            env.create_buffer_with_borrowed_data(
                self.ptr,
                self.len,
                self as *const _ as *mut std::ffi::c_void,
                prevent_gc_release,
            )
        }
    }

    /// Explicitly release the sample back to the pool
    #[napi]
    pub fn release(&mut self) {
        self.inner.take();
    }
}

// prevent_gc_release is called when Buffer is garbage collected
extern "C" fn prevent_gc_release(_env: napi::sys::napi_env, _data: *mut std::ffi::c_void) {
    // Sample will be dropped when ReceivedSample is dropped
    // This callback just prevents early release
}

/// Wrapper for loaned sample (for publishing)
#[napi]
pub struct LoanedSample {
    inner: Option<iceoryx2::sample::SampleMutUninit<ipc::Service, [u8], ()>>,
    ptr: *mut u8,
    len: usize,
}

#[napi]
impl LoanedSample {
    /// Get mutable buffer for writing payload
    #[napi]
    pub fn buffer(&mut self, env: Env) -> Result<JsBuffer> {
        if self.inner.is_none() {
            return Err(Error::from_reason("Sample already sent or released"));
        }

        unsafe {
            env.create_buffer_with_borrowed_data(
                self.ptr,
                self.len,
                std::ptr::null_mut(),
                noop_release,
            )
        }
    }

    /// Send the sample (zero-copy - just pushes offset to queue)
    #[napi]
    pub fn send(&mut self) -> Result<()> {
        let sample = self.inner.take()
            .ok_or_else(|| Error::from_reason("Sample already sent"))?;

        unsafe {
            sample.assume_init().send()
                .map_err(|e| Error::from_reason(format!("{:?}", e)))?;
        }
        Ok(())
    }
}

extern "C" fn noop_release(_env: napi::sys::napi_env, _data: *mut std::ffi::c_void) {}
```

#### Build Script

```rust
// build.rs
extern crate napi_build;

fn main() {
    napi_build::setup();
}
```

#### TypeScript Definitions

```typescript
// index.d.ts
export class Node {
    constructor(name: string);
    serviceBuilder(serviceName: string): ServiceBuilder;
}

export class ServiceBuilder {
    publishSubscribe(payloadSize: number): PublishSubscribeBuilder;
    requestResponse(requestSize: number, responseSize: number): RequestResponseBuilder;
}

export class PublishSubscribeBuilder {
    create(): PublishSubscribeService;
    open(): PublishSubscribeService;
    openOrCreate(): PublishSubscribeService;
}

export class PublishSubscribeService {
    publisherBuilder(): PublisherBuilder;
    subscriberBuilder(): SubscriberBuilder;
}

export class PublisherBuilder {
    create(): Publisher;
}

export class Publisher {
    loanUninit(): LoanedSample;
    send(data: Buffer): void;  // Copy-based convenience method
}

export class LoanedSample {
    readonly buffer: Buffer;   // Zero-copy mutable view
    send(): void;
}

export class SubscriberBuilder {
    create(): Subscriber;
}

export class Subscriber {
    receive(): ReceivedSample | null;
    receiveAsync(): Promise<ReceivedSample>;
}

export class ReceivedSample {
    readonly buffer: Buffer;   // Zero-copy read-only view
    release(): void;
}
```

---

## Performance Characteristics

### Latency Comparison

| Method | Latency | Notes |
|--------|---------|-------|
| IceOryx2 (polling) | ~100ns | Same-machine, optimized |
| IceOryx2 (waitset) | ~1µs | With event notification |
| Unix Domain Socket | ~10µs | Kernel copy involved |
| TCP localhost | ~50µs | Full network stack |
| Redis (localhost) | ~100µs | Serialization + network |
| gRPC (localhost) | ~200µs | Protobuf + HTTP/2 |

### Throughput

- **Message rate**: 10M+ messages/second (small payloads)
- **Bandwidth**: Limited only by memory bandwidth (~50 GB/s on modern systems)
- **Payload transfer**: Always 8 bytes (offset only), regardless of actual data size

### Memory Usage

| Component | Memory |
|-----------|--------|
| Per-service overhead | ~64 KB |
| Per-subscriber connection | ~4 KB |
| Payload pool | Configurable (default: 64 slots) |
| Management segment | ~1 MB per service |

---

## Safety Considerations

### Lifetime Management

**Problem**: JavaScript's garbage collector may free a Buffer while shared memory is still referenced.

**Solution**: Pin samples until explicitly released:

```rust
#[napi]
impl ReceivedSample {
    // Buffer creation stores reference back to sample
    pub fn buffer(&self, env: Env) -> Result<JsBuffer> {
        // ... see implementation above
    }
}
```

### Thread Safety

**Problem**: Node.js Buffer creation must happen on main thread.

**Solution**: Use napi's `ThreadsafeFunction` for async notifications:

```rust
use napi::threadsafe_function::{ThreadsafeFunction, ThreadsafeFunctionCallMode};

#[napi]
impl Subscriber {
    #[napi]
    pub fn on_data(&self, callback: JsFunction) -> Result<()> {
        let tsfn: ThreadsafeFunction<ReceivedSample, ErrorStrategy::Fatal> = callback
            .create_threadsafe_function(0, |ctx| {
                Ok(vec![ctx.value])
            })?;

        // Spawn thread to wait on iceoryx2 WaitSet
        std::thread::spawn(move || {
            // When data arrives, call back to JS main thread
            tsfn.call(sample, ThreadsafeFunctionCallMode::Blocking);
        });

        Ok(())
    }
}
```

### Memory Corruption Prevention

1. **Type checking**: IceOryx2 validates type name + size + alignment at connect
2. **Bounds checking**: Validate buffer sizes before access
3. **Poison values**: Fill uninitialized memory with detectable patterns (debug builds)

### Race Condition Prevention

IceOryx2 uses lock-free algorithms:

- **SPSC queues**: Single-producer single-consumer for each connection
- **Atomic operations**: For queue head/tail pointers
- **Memory barriers**: Ensure visibility across cores

---

## Common Pitfalls

### 1. Struct Padding Mismatch

```rust
// WRONG - Different padding on different platforms
#[repr(C)]
struct Bad {
    a: u8,
    b: u64,  // May have 7 bytes padding before, or not
}

// CORRECT - Explicit padding
#[repr(C)]
struct Good {
    a: u8,
    _pad: [u8; 7],
    b: u64,
}
```

### 2. Endianness Issues

Always use explicit endianness in JavaScript:

```typescript
// WRONG - platform-dependent
const value = buffer.readDoubleLE(0);  // Might work, might not

// CORRECT - explicit little-endian (matches x86/ARM)
const view = new DataView(buffer.buffer, buffer.byteOffset);
const value = view.getFloat64(0, true);  // true = little-endian
```

### 3. Buffer Lifetime Bugs

```typescript
// WRONG - Buffer may be invalid after receive() returns
const sample = subscriber.receive();
const buffer = sample.buffer;
sample.release();  // !! buffer now points to freed memory !!
console.log(buffer[0]);  // UNDEFINED BEHAVIOR

// CORRECT - Use buffer before releasing
const sample = subscriber.receive();
const data = readSensorData(sample.buffer);  // Copy data out
sample.release();  // Now safe
console.log(data.temperature);
```

### 4. Forgetting to Send Loaned Samples

```typescript
// WRONG - Sample is leaked, pool slot never returned
function publish() {
    const sample = publisher.loanUninit();
    writeSensorData(sample.buffer, data);
    // Forgot to call sample.send()!
    return;  // Sample leaked
}

// CORRECT - Always send or explicitly release
function publish() {
    const sample = publisher.loanUninit();
    try {
        writeSensorData(sample.buffer, data);
        sample.send();
    } catch (e) {
        sample.release();  // Return to pool on error
        throw e;
    }
}
```

---

## References

### Official Documentation

- [IceOryx2 GitHub](https://github.com/eclipse-iceoryx/iceoryx2)
- [IceOryx2 Book](https://ekxide.github.io/iceoryx2-book/main/)
- [IceOryx2 Rust Docs](https://docs.rs/iceoryx2/latest/iceoryx2/)
- [IceOryx2 PyPI](https://pypi.org/project/iceoryx2/)

### napi-rs Resources

- [napi-rs GitHub](https://github.com/napi-rs/napi-rs)
- [napi-rs Documentation](https://napi.rs/)
- [napi-rs Buffer Handling](https://napi.rs/docs/concepts/typed-array)
- [napi-rs External Objects](https://napi.rs/docs/concepts/external)

### POSIX Shared Memory

- [shm_open(3) Manual](https://man7.org/linux/man-pages/man3/shm_open.3.html)
- [shm_overview(7) Manual](https://www.man7.org/linux/man-pages/man7/shm_overview.7.html)

### Blog Posts & Articles

- [Implementing True Zero-Copy Communication](https://ekxide.io/blog/how-to-implement-zero-copy-communication/)
- [IceOryx2 v0.7.0 Release](https://ekxide.io/blog/iceoryx2-0-7-release/)
- [IceOryx2 v0.6.0 Release](https://ekxide.io/blog/iceoryx2-0-6-release/)

---

## Appendix A: Quick Reference Card

### Memory Layout Checklist

- [ ] All shared structs use `#[repr(C)]`
- [ ] Explicit padding fields added where needed
- [ ] Size verified with `static_assert` / `const _: ()`
- [ ] Alignment verified
- [ ] Endianness documented (typically little-endian)

### Service Creation Checklist

- [ ] Unique service name chosen
- [ ] Payload type defined identically in all languages
- [ ] Publisher/subscriber count configured appropriately
- [ ] History/buffer depth set based on use case

### Performance Checklist

- [ ] Reuse Node/Service instances (expensive to create)
- [ ] Use `loan_uninit()` + `write_payload()` for zero-copy publish
- [ ] Release samples promptly to return slots to pool
- [ ] Use WaitSet for event-driven instead of busy-polling

---

## Appendix B: Troubleshooting

### "Incompatible types" Error

**Cause**: Type name, size, or alignment mismatch between publisher and subscriber.

**Fix**:
1. Verify struct definitions match exactly across languages
2. Check for padding differences
3. Ensure same type name string is used

### "No more samples available" Error

**Cause**: All slots in publisher's pool are loaned out.

**Fix**:
1. Ensure samples are sent or released
2. Increase publisher's `max_loaned_samples` setting
3. Check for memory leaks in sample handling

### Corrupted Data

**Cause**: Usually endianness or padding mismatch.

**Fix**:
1. Use explicit endianness in DataView calls
2. Verify struct layout with hex dump
3. Add magic number/CRC for validation

### Segmentation Fault in Node.js

**Cause**: Buffer used after sample released.

**Fix**:
1. Copy data out of buffer before releasing sample
2. Use reference counting to track buffer usage
3. Enable AddressSanitizer in debug builds
