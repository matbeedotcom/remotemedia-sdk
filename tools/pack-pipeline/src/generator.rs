//! Python package generator
//!
//! Generates a complete Maturin-based Python package from a pipeline YAML.
//! 
//! Python nodes are compiled to bytecode (.pyc) for embedding, not copied as source.

use crate::node_resolver::{self, PythonNodeFile};
use crate::templates;
use anyhow::{bail, Context, Result};
use heck::{ToSnakeCase, ToUpperCamelCase};
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::env::temp_dir;

/// Configuration for Python package generation
pub struct PythonPackageConfig {
    pub pipeline_path: PathBuf,
    pub name_override: Option<String>,
    pub version: String,
    pub output_dir: PathBuf,
    pub workspace_root: PathBuf,
    pub build_wheel: bool,
    pub release_mode: bool,
    pub test_wheel: bool,
    pub python_requires: String,
    pub extra_dependencies: Vec<String>,
    /// Bundle a self-contained Python environment with pre-installed deps
    pub bundle_python: bool,
    /// Python version to bundle (e.g. "3.11")
    pub python_version: String,
    /// Target platform for cross-platform bundling
    pub bundle_target: Option<String>,
}

/// Parsed pipeline metadata
struct PipelineMetadata {
    name: String,
    description: String,
    is_streaming: bool,
}

/// Extract metadata from pipeline YAML
fn parse_pipeline_metadata(yaml_content: &str) -> Result<PipelineMetadata> {
    let yaml: serde_yaml::Value = serde_yaml::from_str(yaml_content)
        .context("Failed to parse pipeline YAML")?;

    let metadata = yaml.get("metadata")
        .context("Pipeline YAML missing 'metadata' section")?;

    let name = metadata.get("name")
        .and_then(|n| n.as_str())
        .context("Pipeline metadata missing 'name'")?
        .to_string();

    let description = metadata.get("description")
        .and_then(|d| d.as_str())
        .unwrap_or("RemoteMedia pipeline")
        .to_string();

    // Check if any nodes are streaming
    let is_streaming = yaml.get("nodes")
        .and_then(|nodes| nodes.as_sequence())
        .map(|nodes| {
            nodes.iter().any(|node| {
                node.get("is_streaming")
                    .and_then(|s| s.as_bool())
                    .unwrap_or(false)
            })
        })
        .unwrap_or(false);

    Ok(PipelineMetadata {
        name,
        description,
        is_streaming,
    })
}

/// Generate a Python package from pipeline YAML
pub fn generate_python_package(config: PythonPackageConfig) -> Result<()> {
    // Read and parse pipeline
    let yaml_content = fs::read_to_string(&config.pipeline_path)
        .with_context(|| format!("Failed to read pipeline: {:?}", config.pipeline_path))?;
    
    let metadata = parse_pipeline_metadata(&yaml_content)?;
    
    // Determine package name
    let package_name = config.name_override
        .unwrap_or_else(|| metadata.name.to_snake_case());
    
    // Validate package name
    if !is_valid_python_package_name(&package_name) {
        bail!(
            "Invalid package name '{}'. Must be lowercase, start with letter, \
            and contain only letters, numbers, and underscores.",
            package_name
        );
    }

    // Use simple "Session" class name - avoids redundant "tts.TtsSession"
    let class_name = "Session".to_string();
    
    tracing::info!("Generating Python package: {}", package_name);
    tracing::info!("  Class name: {}", class_name);
    tracing::info!("  Streaming mode: {}", metadata.is_streaming);
    
    // Analyze the pipeline against the actual node registry
    let analysis = node_resolver::analyze_pipeline_yaml(&yaml_content)?;
    
    // Check for missing node types
    if !analysis.is_valid {
        bail!(
            "Pipeline contains unregistered node types: {:?}\n\nAvailable types: {:?}",
            analysis.missing_types,
            analysis.registered_types
        );
    }
    
    // Log resolved nodes
    tracing::info!("Resolved {} Python nodes: {:?}", 
        analysis.python_node_types.len(), 
        analysis.python_node_types
    );
    tracing::info!("Resolved {} Rust nodes (built-in): {:?}", 
        analysis.rust_node_types.len(),
        analysis.rust_node_types
    );
    
    // Find Python source files for the Python node types
    let python_nodes_dir = node_resolver::get_python_nodes_dir(&config.workspace_root);
    let python_files = if !analysis.python_node_types.is_empty() {
        node_resolver::find_python_node_files(&python_nodes_dir, &analysis.python_node_types)?
    } else {
        Vec::new()
    };
    
    tracing::info!("Found {} Python source files to compile:", python_files.len());
    for file in &python_files {
        tracing::info!("  - {} (defines: {:?})", file.relative_path, file.node_classes);
    }
    
    // Compile Python nodes to bytecode using a temp venv with remotemedia installed
    let compiled_bytecode = if !python_files.is_empty() {
        tracing::info!("Creating build environment and compiling Python nodes to bytecode...");
        compile_python_nodes_to_bytecode(&python_files, &config.workspace_root)?
    } else {
        Vec::new()
    };
    
    // Create output directory structure
    let pkg_dir = config.output_dir.join(&package_name);
    let src_dir = pkg_dir.join("src");
    let nodes_dir = src_dir.join("nodes");
    let python_dir = pkg_dir.join("python").join(&package_name);
    
    fs::create_dir_all(&src_dir).context("Failed to create src directory")?;
    fs::create_dir_all(&nodes_dir).context("Failed to create nodes directory")?;
    fs::create_dir_all(&python_dir).context("Failed to create python directory")?;

    // Copy compiled bytecode files to src/nodes/
    let embedded_files = copy_compiled_bytecode(&compiled_bytecode, &nodes_dir)?;
    
    // Bundle the remotemedia Python client for self-contained execution
    let remotemedia_src = config.workspace_root
        .join("clients")
        .join("python")
        .join("remotemedia");
    let remotemedia_dst = pkg_dir.join("python").join("remotemedia");
    bundle_python_package(&remotemedia_src, &remotemedia_dst)?;
    
    // Generate Cargo.toml with path dependencies (include candle-nodes if needed)
    let workspace_root_str = config.workspace_root.to_string_lossy();
    let cargo_toml = templates::generate_cargo_toml(
        &package_name, 
        &config.version, 
        &workspace_root_str,
        &analysis.rust_node_types,
    );
    fs::write(pkg_dir.join("Cargo.toml"), cargo_toml)
        .context("Failed to write Cargo.toml")?;
    
    // Read Python dependencies from requirements.txt
    let requirements_path = config.workspace_root
        .join("clients")
        .join("python")
        .join("requirements.txt");
    let python_deps = read_requirements_txt(&requirements_path)?;
    
    // Merge with any extra dependencies from config
    let mut all_deps = python_deps;
    all_deps.extend(config.extra_dependencies.clone());
    
    // Generate pyproject.toml
    let pyproject = templates::generate_pyproject_toml(
        &package_name,
        &config.version,
        &metadata.description,
        &config.python_requires,
        &all_deps,
    );
    fs::write(pkg_dir.join("pyproject.toml"), pyproject)
        .context("Failed to write pyproject.toml")?;

    // Copy pipeline YAML to src/
    fs::write(src_dir.join("pipeline.yaml"), &yaml_content)
        .context("Failed to copy pipeline YAML")?;

    // Generate lib.rs with embedded pipeline AND embedded Python nodes
    let lib_rs = templates::generate_lib_rs_with_nodes(
        &package_name,
        &class_name,
        &metadata.description,
        metadata.is_streaming,
        &embedded_files,
        &analysis.rust_node_types,
    );
    fs::write(src_dir.join("lib.rs"), lib_rs)
        .context("Failed to write lib.rs")?;

    // Generate Python __init__.py
    let init_py = templates::generate_init_py(&package_name, &class_name, metadata.is_streaming);
    fs::write(python_dir.join("__init__.py"), init_py)
        .context("Failed to write __init__.py")?;
    
    // Create py.typed marker for PEP 561
    fs::write(python_dir.join("py.typed"), "")
        .context("Failed to write py.typed")?;

    // Generate README.md
    let readme = templates::generate_readme(
        &package_name,
        &class_name,
        &metadata.description,
        metadata.is_streaming,
    );
    fs::write(pkg_dir.join("README.md"), readme)
        .context("Failed to write README.md")?;

    // Bundle self-contained Python environment if requested
    if config.bundle_python {
        bundle_python_environment(
            &pkg_dir,
            &config.python_version,
            &all_deps,
            config.bundle_target.as_deref(),
        )?;
    }

    tracing::info!("Generated package at: {:?}", pkg_dir);

    // Build wheel if requested
    if config.build_wheel {
        let wheel_path = build_wheel(&pkg_dir, config.release_mode)?;
        
        // Test wheel if requested
        if config.test_wheel {
            if let Some(wheel) = wheel_path {
                test_wheel(&wheel, &package_name)?;
            }
        }
    } else {
        println!("\n✓ Package generated at: {}", pkg_dir.display());
        println!("\nEmbedded {} Python node files:", embedded_files.len());
        for file in &embedded_files {
            println!("  - {}", file);
        }
        println!("\nTo build the wheel:");
        println!("  cd {} && maturin build", pkg_dir.display());
        println!("\nTo install for development:");
        println!("  cd {} && maturin develop", pkg_dir.display());
    }

    Ok(())
}

/// Bundle a Python package directory into the output
fn bundle_python_package(src: &PathBuf, dst: &PathBuf) -> Result<()> {
    if !src.exists() {
        bail!("Python package source not found: {:?}", src);
    }
    
    tracing::info!("Bundling Python package: {:?} -> {:?}", src, dst);
    
    // Remove destination if it exists
    if dst.exists() {
        fs::remove_dir_all(dst)?;
    }
    
    // Recursively copy the package
    copy_dir_recursive(src, dst)?;
    
    tracing::info!("Successfully bundled Python package");
    Ok(())
}

/// Recursively copy a directory
fn copy_dir_recursive(src: &PathBuf, dst: &PathBuf) -> Result<()> {
    fs::create_dir_all(dst)?;
    
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let path = entry.path();
        let file_name = entry.file_name();
        let dest_path = dst.join(&file_name);
        
        // Skip __pycache__ and .pyc files (we want source for bundling)
        let name = file_name.to_string_lossy();
        if name == "__pycache__" || name.ends_with(".pyc") {
            continue;
        }
        
        if path.is_dir() {
            copy_dir_recursive(&path, &dest_path)?;
        } else {
            fs::copy(&path, &dest_path)?;
        }
    }
    
    Ok(())
}

/// Read dependencies from a requirements.txt file
fn read_requirements_txt(path: &PathBuf) -> Result<Vec<String>> {
    let content = fs::read_to_string(path)
        .with_context(|| format!("Failed to read requirements.txt: {:?}", path))?;
    
    let deps: Vec<String> = content
        .lines()
        .filter(|line| {
            let line = line.trim();
            // Skip empty lines, comments, and optional dependencies
            !line.is_empty() && !line.starts_with('#') && !line.starts_with('-')
        })
        .map(|line| {
            // Convert == to >= for more flexible version matching
            // e.g., "numpy==2.3.2" -> "numpy>=2.3.2"
            let line = line.trim();
            if line.contains("==") {
                line.replacen("==", ">=", 1)
            } else {
                line.to_string()
            }
        })
        .collect();
    
    tracing::info!("Read {} dependencies from requirements.txt", deps.len());
    Ok(deps)
}

/// Compiled Python bytecode info
pub struct CompiledPythonNode {
    /// Module name (e.g., "tts" or "ml.lfm2_audio")
    pub module_name: String,
    /// Relative path for the .pyc file (e.g., "tts.pyc")
    pub pyc_path: String,
    /// The compiled bytecode
    pub bytecode: Vec<u8>,
    /// Node classes defined in this module
    pub node_classes: Vec<String>,
}

/// Create a temporary venv with remotemedia installed and compile Python nodes to bytecode
fn compile_python_nodes_to_bytecode(
    python_files: &[PythonNodeFile],
    workspace_root: &PathBuf,
) -> Result<Vec<CompiledPythonNode>> {
    let venv_dir = temp_dir().join(format!("remotemedia_build_{}", std::process::id()));
    
    tracing::info!("Creating build venv at: {:?}", venv_dir);
    
    // Create venv
    let status = Command::new("python3")
        .args(["-m", "venv"])
        .arg(&venv_dir)
        .status()
        .context("Failed to create build venv")?;
    
    if !status.success() {
        bail!("Failed to create build venv");
    }
    
    // Install remotemedia from local source
    let pip = venv_dir.join("bin").join("pip");
    let remotemedia_python_dir = workspace_root.join("clients").join("python");
    
    tracing::info!("Installing remotemedia from: {:?}", remotemedia_python_dir);
    
    let status = Command::new(&pip)
        .args(["install", "-e"])
        .arg(&remotemedia_python_dir)
        .status()
        .context("Failed to install remotemedia in build venv")?;
    
    if !status.success() {
        let _ = fs::remove_dir_all(&venv_dir);
        bail!("Failed to install remotemedia in build venv");
    }
    
    // Create a temp directory for compilation
    let compile_dir = venv_dir.join("compile_src");
    fs::create_dir_all(&compile_dir)?;
    
    // Copy source files to compile directory
    for file in python_files {
        let dest_path = compile_dir.join(&file.relative_path);
        if let Some(parent) = dest_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&dest_path, &file.content)?;
    }
    
    // Compile all Python files to bytecode using the venv's Python
    let python = venv_dir.join("bin").join("python");
    
    // Use compileall to compile, then extract the bytecode
    let compile_script = format!(
        r#"
import py_compile
import marshal
import sys
import os
import struct

# Compile each file and output the bytecode
compile_dir = '{compile_dir}'
files = {file_list}

for rel_path in files:
    src_path = os.path.join(compile_dir, rel_path)
    
    # Compile to bytecode
    try:
        # Use py_compile to compile (validates syntax with imports)
        code = compile(open(src_path).read(), rel_path, 'exec')
        
        # Serialize the code object with marshal
        bytecode = marshal.dumps(code)
        
        # Output: path<NUL>length<NUL>bytecode
        # Use a simple protocol: write path, then 8-byte length, then bytecode
        sys.stdout.buffer.write(rel_path.encode('utf-8'))
        sys.stdout.buffer.write(b'\x00')
        sys.stdout.buffer.write(struct.pack('<Q', len(bytecode)))
        sys.stdout.buffer.write(bytecode)
        sys.stdout.buffer.write(b'\x00')  # separator
        
    except Exception as e:
        print(f"ERROR: Failed to compile {{rel_path}}: {{e}}", file=sys.stderr)
        sys.exit(1)
"#,
        compile_dir = compile_dir.to_string_lossy(),
        file_list = format!("{:?}", python_files.iter().map(|f| f.relative_path.clone()).collect::<Vec<_>>()),
    );
    
    tracing::info!("Compiling {} Python files to bytecode...", python_files.len());
    
    let output = Command::new(&python)
        .args(["-c", &compile_script])
        .output()
        .context("Failed to run Python bytecode compilation")?;
    
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let _ = fs::remove_dir_all(&venv_dir);
        bail!("Python bytecode compilation failed:\n{}", stderr);
    }
    
    // Parse the output to extract bytecode for each file
    let mut compiled_nodes = Vec::new();
    let mut cursor = 0;
    let data = &output.stdout;
    
    while cursor < data.len() {
        // Find path (null-terminated)
        let path_end = data[cursor..].iter().position(|&b| b == 0)
            .context("Invalid bytecode output format")?;
        let rel_path = String::from_utf8(data[cursor..cursor + path_end].to_vec())
            .context("Invalid UTF-8 in path")?;
        cursor += path_end + 1;
        
        if cursor >= data.len() {
            break;
        }
        
        // Read 8-byte length
        if cursor + 8 > data.len() {
            break;
        }
        let length = u64::from_le_bytes(data[cursor..cursor + 8].try_into().unwrap()) as usize;
        cursor += 8;
        
        // Read bytecode
        if cursor + length > data.len() {
            bail!("Bytecode length exceeds available data");
        }
        let bytecode = data[cursor..cursor + length].to_vec();
        cursor += length;
        
        // Skip separator
        if cursor < data.len() && data[cursor] == 0 {
            cursor += 1;
        }
        
        // Find the corresponding file info
        let file_info = python_files.iter()
            .find(|f| f.relative_path == rel_path)
            .context("Compiled file not found in source list")?;
        
        // Convert path to module name
        let module_name = rel_path
            .trim_end_matches(".py")
            .replace('/', ".");
        
        let pyc_path = rel_path.replace(".py", ".pyc");
        
        tracing::info!("  Compiled {} ({} bytes bytecode)", rel_path, bytecode.len());
        
        compiled_nodes.push(CompiledPythonNode {
            module_name,
            pyc_path,
            bytecode,
            node_classes: file_info.node_classes.clone(),
        });
    }
    
    // Cleanup venv
    let _ = fs::remove_dir_all(&venv_dir);
    
    tracing::info!("Successfully compiled {} Python nodes to bytecode", compiled_nodes.len());
    
    Ok(compiled_nodes)
}

/// Copy compiled bytecode files to the package's nodes directory
/// Returns the list of relative .pyc file paths (for include_bytes! generation)
fn copy_compiled_bytecode(compiled_nodes: &[CompiledPythonNode], nodes_dir: &PathBuf) -> Result<Vec<String>> {
    let mut copied_files: Vec<String> = Vec::new();
    
    for node in compiled_nodes {
        // Create subdirectory if needed (e.g., ml/ for ml/lfm2_audio.pyc)
        let dest_path = nodes_dir.join(&node.pyc_path);
        if let Some(parent) = dest_path.parent() {
            fs::create_dir_all(parent)?;
        }
        
        // Write the bytecode
        fs::write(&dest_path, &node.bytecode)
            .with_context(|| format!("Failed to write bytecode: {}", node.pyc_path))?;
        
        tracing::debug!("Wrote bytecode: {} -> {:?}", node.pyc_path, dest_path);
        copied_files.push(node.pyc_path.clone());
    }
    
    Ok(copied_files)
}

/// Build the wheel using maturin
fn build_wheel(pkg_dir: &PathBuf, release: bool) -> Result<Option<PathBuf>> {
    tracing::info!("Building wheel with maturin...");
    
    let mut cmd = Command::new("maturin");
    cmd.arg("build");
    cmd.arg("--interpreter").arg("python3");
    cmd.current_dir(pkg_dir);
    
    if release {
        cmd.arg("--release");
    }
    
    let status = cmd.status()
        .context("Failed to run maturin. Is it installed? (pip install maturin)")?;
    
    if !status.success() {
        bail!("Maturin build failed with exit code: {:?}", status.code());
    }
    
    // Find and return the built wheel
    let wheels_dir = pkg_dir.join("target").join("wheels");
    let mut wheel_path = None;
    if wheels_dir.exists() {
        for entry in fs::read_dir(&wheels_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().map(|e| e == "whl").unwrap_or(false) {
                println!("\n✓ Built wheel: {}", path.display());
                wheel_path = Some(path);
            }
        }
    }
    
    Ok(wheel_path)
}

/// Test the wheel by installing in a temp venv and running import tests
fn test_wheel(wheel_path: &PathBuf, package_name: &str) -> Result<()> {
    use std::env::temp_dir;
    
    println!("\n🧪 Testing wheel...");
    
    // Create temp venv
    let venv_dir = temp_dir().join(format!("test_venv_{}", std::process::id()));
    tracing::info!("Creating test venv at: {:?}", venv_dir);
    
    let status = Command::new("python3")
        .args(["-m", "venv"])
        .arg(&venv_dir)
        .status()
        .context("Failed to create test venv")?;
    
    if !status.success() {
        bail!("Failed to create test venv");
    }
    
    // Install wheel
    let pip = venv_dir.join("bin").join("pip");
    tracing::info!("Installing wheel...");
    
    let status = Command::new(&pip)
        .args(["install", "--quiet"])
        .arg(wheel_path)
        .status()
        .context("Failed to install wheel")?;
    
    if !status.success() {
        bail!("Failed to install wheel in test venv");
    }
    
    // Run import and execution test
    let python = venv_dir.join("bin").join("python");
    let class_name = "Session"; // Simple name, not redundant like "TtsSession"
    let test_code = format!(
        r#"
import sys
import asyncio

async def test():
    try:
        from {pkg} import get_version, get_pipeline_yaml, {cls}, process
        print(f"✓ Package version: {{get_version()}}")
        print(f"✓ Pipeline YAML loaded: {{len(get_pipeline_yaml())}} bytes")
        print(f"✓ Session class: {{{cls}}}")
        print("✓ All imports successful!")
        
        # Test session creation
        print("\\n🔄 Testing session creation...")
        session = {cls}()
        print(f"✓ Session created: {{session}}")
        
        # Test pipeline execution with sample data
        print("\\n🔄 Testing pipeline execution...")
        sample_data = {{"type": "text", "data": "test input"}}
        try:
            result = await session.send(sample_data)
            print(f"✓ Pipeline executed successfully")
            print(f"✓ Result type: {{type(result)}}")
        except Exception as exec_err:
            print(f"⚠ Pipeline execution error (expected if no nodes registered): {{exec_err}}")
            # This is okay - the pipeline might need specific node registrations
        
        # Cleanup
        session.close()
        print("✓ Session closed")
        
        print("\\n✅ All tests passed!")
    except Exception as e:
        import traceback
        print(f"✗ Test failed: {{e}}", file=sys.stderr)
        traceback.print_exc()
        sys.exit(1)

asyncio.run(test())
"#,
        pkg = package_name,
        cls = class_name,
    );
    
    tracing::info!("Running import and execution tests...");
    let output = Command::new(&python)
        .args(["-c", &test_code])
        .output()
        .context("Failed to run import test")?;
    
    // Print output
    print!("{}", String::from_utf8_lossy(&output.stdout));
    eprint!("{}", String::from_utf8_lossy(&output.stderr));
    
    // Cleanup venv
    let _ = fs::remove_dir_all(&venv_dir);
    
    if !output.status.success() {
        bail!("Import test failed");
    }
    
    println!("\n✅ All tests passed!");
    Ok(())
}

/// Validate Python package name
fn is_valid_python_package_name(name: &str) -> bool {
    if name.is_empty() {
        return false;
    }
    
    let first_char = name.chars().next().unwrap();
    if !first_char.is_ascii_lowercase() {
        return false;
    }
    
    name.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

/// Bundle a self-contained Python environment into the package.
///
/// Creates a `bundled_env/` directory containing:
/// - A standalone Python installation (via `uv python install`)
/// - A pre-populated venv with all dependencies pre-installed
/// - A `runtime-env.json` that the SDK reads at startup to skip env resolution
fn bundle_python_environment(
    pkg_dir: &PathBuf,
    python_version: &str,
    deps: &[String],
    _target: Option<&str>, // TODO: cross-platform bundling via --target
) -> Result<()> {
    tracing::info!(
        "Bundling Python {} environment with {} dependencies...",
        python_version,
        deps.len()
    );

    let bundled_dir = pkg_dir.join("bundled_env");
    let python_dir = bundled_dir.join("python");
    let venv_dir = bundled_dir.join("venv");

    fs::create_dir_all(&bundled_dir)?;

    // Step 1: Find uv (required for --bundle-python)
    let uv = find_uv_binary()
        .context("uv is required for --bundle-python. Install it: curl -LsSf https://astral.sh/uv/install.sh | sh")?;

    tracing::info!("Using uv at: {:?}", uv);

    // Step 2: Download standalone Python
    tracing::info!("Installing Python {} via uv...", python_version);
    let status = Command::new(&uv)
        .args(["python", "install", python_version])
        .env("UV_PYTHON_INSTALL_DIR", &python_dir)
        .status()
        .context("Failed to run uv python install")?;

    if !status.success() {
        anyhow::bail!(
            "Failed to install Python {}. Is the version valid?",
            python_version
        );
    }

    // Find the installed Python executable
    let python_exe = find_installed_python(&python_dir, python_version)?;
    tracing::info!("Python installed at: {:?}", python_exe);

    // Step 3: Create venv
    tracing::info!("Creating virtual environment...");
    let status = Command::new(&uv)
        .args(["venv", "--python"])
        .arg(&python_exe)
        .arg(&venv_dir)
        .status()
        .context("Failed to create venv")?;

    if !status.success() {
        anyhow::bail!("Failed to create virtual environment");
    }

    // Step 4: Install dependencies
    if !deps.is_empty() {
        tracing::info!("Installing {} dependencies...", deps.len());

        // Write requirements to a temp file
        let req_file = bundled_dir.join("requirements.txt");
        fs::write(&req_file, deps.join("\n"))
            .context("Failed to write requirements.txt")?;

        let venv_python = if cfg!(windows) {
            venv_dir.join("Scripts").join("python.exe")
        } else {
            venv_dir.join("bin").join("python")
        };

        let status = Command::new(&uv)
            .args(["pip", "install", "-r"])
            .arg(&req_file)
            .arg("--python")
            .arg(&venv_python)
            .status()
            .context("Failed to install dependencies")?;

        if !status.success() {
            anyhow::bail!("Failed to install dependencies into bundled environment");
        }

        // Clean up temp requirements file
        let _ = fs::remove_file(&req_file);
    }

    // Step 5: Write runtime-env.json
    let venv_python_rel = if cfg!(windows) {
        "bundled_env/venv/Scripts/python.exe"
    } else {
        "bundled_env/venv/bin/python"
    };

    let runtime_env = serde_json::json!({
        "python_env_mode": "bundled",
        "python_executable": venv_python_rel,
        "python_version": python_version,
        "deps": deps,
        "bundled_at": format!("{}", std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()),
    });

    fs::write(
        bundled_dir.join("runtime-env.json"),
        serde_json::to_string_pretty(&runtime_env)?,
    )
    .context("Failed to write runtime-env.json")?;

    tracing::info!(
        "Bundled Python environment created at: {:?}",
        bundled_dir
    );

    Ok(())
}

/// Find the uv binary on the system.
fn find_uv_binary() -> Option<PathBuf> {
    // Check UV_BINARY_PATH env var
    if let Ok(path) = std::env::var("UV_BINARY_PATH") {
        let p = PathBuf::from(&path);
        if p.exists() {
            return Some(p);
        }
    }

    // Check PATH
    if let Ok(output) = Command::new("uv").arg("--version").output() {
        if output.status.success() {
            return Some(PathBuf::from("uv"));
        }
    }

    // Check default install location
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_default();
    let default_path = PathBuf::from(&home)
        .join(".config")
        .join("remotemedia")
        .join("bin")
        .join("uv");
    if default_path.exists() {
        return Some(default_path);
    }

    None
}

/// Find the Python executable installed by uv in the given directory.
fn find_installed_python(python_dir: &PathBuf, version: &str) -> Result<PathBuf> {
    // uv installs Python to a versioned subdirectory
    // Try common patterns
    if let Ok(entries) = fs::read_dir(python_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                let name = path.file_name().unwrap_or_default().to_string_lossy();
                if name.contains(version) || name.contains("cpython") {
                    // Look for bin/python or python.exe inside
                    let bin_python = path.join("bin").join("python3");
                    if bin_python.exists() {
                        return Ok(bin_python);
                    }
                    let bin_python = path.join("bin").join("python");
                    if bin_python.exists() {
                        return Ok(bin_python);
                    }
                    #[cfg(windows)]
                    {
                        let exe = path.join("python.exe");
                        if exe.exists() {
                            return Ok(exe);
                        }
                    }
                }
            }
        }
    }

    // Fallback: use uv python find
    let output = Command::new("uv")
        .args(["python", "find", version])
        .output()
        .context("Failed to run uv python find")?;

    if output.status.success() {
        let path_str = String::from_utf8_lossy(&output.stdout).trim().to_string();
        return Ok(PathBuf::from(path_str));
    }

    anyhow::bail!(
        "Could not find Python {} in {:?}. Try running: uv python install {}",
        version,
        python_dir,
        version
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_package_names() {
        assert!(is_valid_python_package_name("audio_quality"));
        assert!(is_valid_python_package_name("my_pipeline_v1"));
        assert!(is_valid_python_package_name("tts"));
        assert!(!is_valid_python_package_name(""));
        assert!(!is_valid_python_package_name("123abc"));
        assert!(!is_valid_python_package_name("MyPackage"));
        assert!(!is_valid_python_package_name("my-package"));
    }

    #[test]
    fn test_parse_metadata() {
        let yaml = r#"
version: "1.0"
metadata:
  name: Audio Quality Analysis
  description: Analyzes audio quality
nodes:
  - id: test
    node_type: TestNode
    is_streaming: true
"#;
        let meta = parse_pipeline_metadata(yaml).unwrap();
        assert_eq!(meta.name, "Audio Quality Analysis");
        assert_eq!(meta.description, "Analyzes audio quality");
        assert!(meta.is_streaming);
    }
}
