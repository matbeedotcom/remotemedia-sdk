//! GPU loading smoke test for llama-cpp-4.
//!
//! Initializes the llama backend, enumerates every ggml backend device,
//! prints a human-readable summary, and exits non-zero if no GPU device
//! was registered. If `LLAMA_TEST_MODEL` is set, it additionally loads
//! that GGUF with `n_gpu_layers=1000` to confirm tensors actually move
//! onto the GPU.
//!
//! ```bash
//! # Phase 1: backend / device registration only (fast, no model)
//! cargo run --example cuda_smoke_test -p remotemedia-core \
//!     --features llama-cpp-cuda
//!
//! # Phase 2: also exercise model load + context creation on GPU
//! LLAMA_TEST_MODEL=/path/to/tiny.gguf \
//! cargo run --example cuda_smoke_test -p remotemedia-core \
//!     --features llama-cpp-cuda
//! ```

#[cfg(not(feature = "llama-cpp"))]
fn main() {
    eprintln!(
        "this example requires the `llama-cpp-cuda` feature; \
         re-run with `--features llama-cpp-cuda`"
    );
    std::process::exit(2);
}

#[cfg(feature = "llama-cpp")]
fn main() {
    use llama_cpp_4::llama_backend::LlamaBackend;
    use llama_cpp_sys_4::{
        ggml_backend_dev_count, ggml_backend_dev_description, ggml_backend_dev_get,
        ggml_backend_dev_memory, ggml_backend_dev_name, ggml_backend_dev_type,
        GGML_BACKEND_DEVICE_TYPE_ACCEL, GGML_BACKEND_DEVICE_TYPE_CPU,
        GGML_BACKEND_DEVICE_TYPE_GPU, GGML_BACKEND_DEVICE_TYPE_IGPU,
    };
    use std::ffi::CStr;

    fn cstr(p: *const std::os::raw::c_char) -> String {
        if p.is_null() {
            "<null>".into()
        } else {
            unsafe { CStr::from_ptr(p) }.to_string_lossy().into_owned()
        }
    }

    fn type_label(t: u32) -> &'static str {
        match t {
            x if x == GGML_BACKEND_DEVICE_TYPE_CPU => "CPU",
            x if x == GGML_BACKEND_DEVICE_TYPE_GPU => "GPU",
            x if x == GGML_BACKEND_DEVICE_TYPE_IGPU => "iGPU",
            x if x == GGML_BACKEND_DEVICE_TYPE_ACCEL => "ACCEL",
            _ => "OTHER",
        }
    }

    println!("== llama-cpp / ggml device smoke test ==");

    // Triggers the ggml_backend_registry constructor → CUDA backend init.
    // We don't void logs, so any `ggml_cuda_init: found N CUDA devices`
    // line from libggml-cuda.so will be printed to stderr right here.
    let _backend = LlamaBackend::init().expect("LlamaBackend::init() failed");

    let n = unsafe { ggml_backend_dev_count() };
    println!("ggml_backend_dev_count = {n}");

    let mut gpu_devs: Vec<usize> = Vec::new();
    for i in 0..n {
        let dev = unsafe { ggml_backend_dev_get(i) };
        if dev.is_null() {
            println!("  [{i}] <null device>");
            continue;
        }
        let name = unsafe { cstr(ggml_backend_dev_name(dev)) };
        let desc = unsafe { cstr(ggml_backend_dev_description(dev)) };
        let ty = unsafe { ggml_backend_dev_type(dev) };
        let mut free: usize = 0;
        let mut total: usize = 0;
        unsafe { ggml_backend_dev_memory(dev, &mut free as *mut _, &mut total as *mut _) };
        println!(
            "  [{i}] type={} name={:?} desc={:?} mem={:.2} GiB free / {:.2} GiB total",
            type_label(ty),
            name,
            desc,
            free as f64 / (1u64 << 30) as f64,
            total as f64 / (1u64 << 30) as f64,
        );
        if ty == GGML_BACKEND_DEVICE_TYPE_GPU {
            gpu_devs.push(i);
        }
    }

    if gpu_devs.is_empty() {
        eprintln!(
            "\nFAIL: no GPU device registered.\n\
             Likely causes:\n\
               - llama-cpp-4 was built without the `cuda` feature\n\
               - libggml-cuda.so.0 is not on the loader path next to the binary\n\
               - libggml-cuda.so.0's CUDA runtime deps cannot be resolved (run\n\
                 `ldd target/<profile>/<bin>` and `ldd …/libggml-cuda.so.0`)\n\
               - the .so contains no kernels for your GPU's compute capability\n\
                 (set CUDAARCHS in .cargo/config.toml; see CMake docs)"
        );
        std::process::exit(1);
    }

    println!("\nPASS: {} GPU device(s) registered", gpu_devs.len());

    // Optional Phase 2: actually load a GGUF and create a context with full
    // GPU offload. Lets us verify n_gpu_layers > 0 actually takes effect.
    if let Ok(path) = std::env::var("LLAMA_TEST_MODEL") {
        use llama_cpp_4::context::params::LlamaContextParams;
        use llama_cpp_4::model::params::LlamaModelParams;
        use llama_cpp_4::model::LlamaModel;
        use std::num::NonZeroU32;
        use std::pin::pin;

        println!("\n== Phase 2: loading {} on GPU ==", path);

        let model_params = LlamaModelParams::default().with_n_gpu_layers(1000);
        let model_params = pin!(model_params);
        let model = LlamaModel::load_from_file(&_backend, &path, &model_params)
            .expect("LlamaModel::load_from_file failed — see ggml/llama logs above");

        let ctx_params = LlamaContextParams::default()
            .with_n_ctx(NonZeroU32::new(512))
            .with_n_batch(128);
        let _ctx = model
            .new_context(&_backend, ctx_params)
            .expect("LlamaModel::new_context failed");

        println!("PASS: model + context created with n_gpu_layers=1000");
    } else {
        println!(
            "\n(Set LLAMA_TEST_MODEL=/path/to/tiny.gguf to also exercise model load.)"
        );
    }
}
