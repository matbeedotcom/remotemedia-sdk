#!/usr/bin/env node

/**
 * create-package.js
 *
 * Creates .rmpkg package files for RemoteMedia browser execution.
 *
 * Usage:
 *   node scripts/create-package.js \
 *     --manifest examples/calculator.rmpkg.json \
 *     --wasm ../runtime/target/wasm32-wasip1/release/pipeline_executor_wasm.wasm \
 *     --output calculator.rmpkg
 *
 * Or use the npm script:
 *   npm run package -- --manifest examples/calculator.rmpkg.json --output calculator.rmpkg
 */

import { createWriteStream, createReadStream, statSync, readFileSync } from 'fs';
import { pipeline } from 'stream/promises';
import { resolve, basename } from 'path';
import archiver from 'archiver';
import { parseArgs } from 'util';

// Parse command line arguments
const { values } = parseArgs({
  options: {
    manifest: { type: 'string', short: 'm' },
    wasm: { type: 'string', short: 'w' },
    output: { type: 'string', short: 'o' },
    metadata: { type: 'string' },
    help: { type: 'boolean', short: 'h' }
  }
});

if (values.help || !values.manifest || !values.output) {
  console.log(`
Usage: node scripts/create-package.js [options]

Options:
  -m, --manifest <path>   Path to manifest JSON file (required)
  -w, --wasm <path>       Path to WASM runtime binary (default: ../runtime/target/wasm32-wasip1/release/pipeline_executor_wasm.wasm)
  -o, --output <path>     Output .rmpkg file path (required)
  --metadata <path>       Optional metadata.json file
  -h, --help              Show this help message

Examples:
  # Create package with default WASM binary
  node scripts/create-package.js \\
    --manifest examples/calculator.rmpkg.json \\
    --output calculator.rmpkg

  # Create package with custom WASM binary
  node scripts/create-package.js \\
    --manifest examples/calculator.rmpkg.json \\
    --wasm path/to/custom.wasm \\
    --output calculator.rmpkg

  # Using npm script
  npm run package -- --manifest examples/calculator.rmpkg.json --output calculator.rmpkg
`);
  process.exit(values.help ? 0 : 1);
}

// Default WASM path
const DEFAULT_WASM = '../runtime/target/wasm32-wasip1/release/pipeline_executor_wasm.wasm';

async function createPackage(options) {
  const {
    manifestPath,
    wasmPath = DEFAULT_WASM,
    outputPath,
    metadataPath
  } = options;

  console.log('Creating .rmpkg package...');
  console.log(`  Manifest: ${manifestPath}`);
  console.log(`  WASM:     ${wasmPath}`);
  console.log(`  Output:   ${outputPath}`);

  // Resolve paths
  const resolvedManifest = resolve(manifestPath);
  const resolvedWasm = resolve(wasmPath);
  const resolvedOutput = resolve(outputPath);
  const resolvedMetadata = metadataPath ? resolve(metadataPath) : null;

  // Validate inputs
  try {
    statSync(resolvedManifest);
  } catch (err) {
    console.error(`Error: Manifest file not found: ${resolvedManifest}`);
    process.exit(1);
  }

  try {
    statSync(resolvedWasm);
  } catch (err) {
    console.error(`Error: WASM file not found: ${resolvedWasm}`);
    console.error(`  Did you build the WASM binary? Run:`);
    console.error(`  cd runtime && cargo build --target wasm32-wasip1 --bin pipeline_executor_wasm --release --no-default-features --features wasm`);
    process.exit(1);
  }

  if (resolvedMetadata) {
    try {
      statSync(resolvedMetadata);
    } catch (err) {
      console.error(`Error: Metadata file not found: ${resolvedMetadata}`);
      process.exit(1);
    }
  }

  // Validate manifest JSON
  let manifest;
  try {
    const manifestContent = readFileSync(resolvedManifest, 'utf8');
    manifest = JSON.parse(manifestContent);
  } catch (err) {
    console.error(`Error: Invalid JSON in manifest file: ${err.message}`);
    process.exit(1);
  }

  // Validate manifest has runtime.target
  if (!manifest.runtime || !manifest.runtime.target) {
    console.warn(`Warning: Manifest missing runtime.target field. Adding default "wasm32-wasi"`);
    manifest.runtime = manifest.runtime || {};
    manifest.runtime.target = 'wasm32-wasi';
  }

  if (manifest.runtime.target !== 'wasm32-wasi') {
    console.error(`Error: Invalid runtime.target "${manifest.runtime.target}". Must be "wasm32-wasi" for browser execution.`);
    process.exit(1);
  }

  // Get file sizes
  const wasmStat = statSync(resolvedWasm);
  const manifestStat = statSync(resolvedManifest);

  console.log('\nPackage contents:');
  console.log(`  manifest.json:  ${formatBytes(manifestStat.size)}`);
  console.log(`  runtime.wasm:   ${formatBytes(wasmStat.size)}`);

  // Generate metadata if not provided
  let metadata = null;
  if (resolvedMetadata) {
    const metadataContent = readFileSync(resolvedMetadata, 'utf8');
    metadata = JSON.parse(metadataContent);
  } else {
    // Auto-generate metadata from manifest
    metadata = generateMetadata(manifest, wasmStat.size);
    console.log(`  metadata.json:  ${formatBytes(JSON.stringify(metadata).length)} (auto-generated)`);
  }

  // Create ZIP archive
  const output = createWriteStream(resolvedOutput);
  const archive = archiver('zip', { zlib: { level: 9 } });

  archive.on('warning', (err) => {
    if (err.code === 'ENOENT') {
      console.warn('Warning:', err);
    } else {
      throw err;
    }
  });

  archive.on('error', (err) => {
    throw err;
  });

  // Pipe archive to output file
  archive.pipe(output);

  // Add files to archive
  archive.file(resolvedManifest, { name: 'manifest.json' });
  archive.file(resolvedWasm, { name: 'runtime.wasm' });

  if (metadata) {
    archive.append(JSON.stringify(metadata, null, 2), { name: 'metadata.json' });
  }

  // Finalize archive
  await archive.finalize();

  // Wait for output stream to finish
  await new Promise((resolve, reject) => {
    output.on('close', resolve);
    output.on('error', reject);
  });

  const outputStat = statSync(resolvedOutput);
  console.log(`\n✅ Package created successfully!`);
  console.log(`  Output: ${resolvedOutput}`);
  console.log(`  Size:   ${formatBytes(outputStat.size)}`);
  console.log(`  Nodes:  ${manifest.nodes.length} (${getNodeTypes(manifest).join(', ')})`);

  if (wasmStat.size > 30 * 1024 * 1024) {
    console.log(`\n⚠️  Warning: WASM binary is large (${formatBytes(wasmStat.size)})`);
    console.log(`  Consider using wasm-opt to reduce size:`);
    console.log(`  wasm-opt -O3 -o optimized.wasm runtime.wasm`);
  }
}

function generateMetadata(manifest, wasmSize) {
  const nodeTypes = getNodeTypes(manifest);
  const hasPythonNodes = nodeTypes.some(t =>
    t.includes('Python') || t === 'TextProcessorNode' || t === 'DataTransformNode'
  );
  const hasRustNodes = nodeTypes.some(t =>
    t === 'MultiplyNode' || t === 'AddNode'
  );

  return {
    package_name: manifest.metadata?.name || 'unnamed-pipeline',
    package_version: manifest.metadata?.version || '1.0.0',
    author: manifest.metadata?.author || 'Unknown',
    license: manifest.metadata?.license || 'MIT',
    created_at: new Date().toISOString(),
    runtime_info: {
      wasm_size_bytes: wasmSize,
      has_python_nodes: hasPythonNodes,
      has_rust_nodes: hasRustNodes,
      node_types: nodeTypes,
      node_count: manifest.nodes.length,
      connection_count: manifest.connections?.length || 0
    },
    performance_hints: {
      expected_load_time_ms: wasmSize < 10 * 1024 * 1024 ? 50 : 200,
      expected_execution_time_ms: hasPythonNodes ? 50 : 10,
      memory_requirement_mb: Math.ceil(wasmSize / (1024 * 1024)) + 20
    }
  };
}

function getNodeTypes(manifest) {
  return [...new Set(manifest.nodes.map(n => n.node_type))];
}

function formatBytes(bytes) {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(2)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(2)} MB`;
}

// Run
createPackage({
  manifestPath: values.manifest,
  wasmPath: values.wasm,
  outputPath: values.output,
  metadataPath: values.metadata
}).catch(err => {
  console.error('Error:', err.message);
  process.exit(1);
});
