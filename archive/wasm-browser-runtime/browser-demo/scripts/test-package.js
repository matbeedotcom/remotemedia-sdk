#!/usr/bin/env node

/**
 * test-package.js
 *
 * Validates .rmpkg package files by extracting and checking contents.
 *
 * Usage:
 *   node scripts/test-package.js calculator.rmpkg
 */

import { readFileSync } from 'fs';
import { parseArgs } from 'util';
import JSZip from 'jszip';

// Parse command line arguments
const args = process.argv.slice(2);
if (args.length === 0 || args[0] === '--help' || args[0] === '-h') {
  console.log(`
Usage: node scripts/test-package.js <package.rmpkg>

Examples:
  node scripts/test-package.js calculator.rmpkg
  npm run test-package -- calculator.rmpkg
`);
  process.exit(args[0] === '--help' || args[0] === '-h' ? 0 : 1);
}

const packagePath = args[0];

async function testPackage(pkgPath) {
  console.log(`Testing package: ${pkgPath}`);
  console.log('='.repeat(60));

  // Read package file
  let packageBuffer;
  try {
    packageBuffer = readFileSync(pkgPath);
    console.log(`✓ Package file loaded (${formatBytes(packageBuffer.length)})`);
  } catch (err) {
    console.error(`✗ Failed to read package file: ${err.message}`);
    process.exit(1);
  }

  // Extract ZIP
  const zip = new JSZip();
  let archive;
  try {
    archive = await zip.loadAsync(packageBuffer);
    console.log(`✓ ZIP archive extracted`);
  } catch (err) {
    console.error(`✗ Failed to extract ZIP: ${err.message}`);
    process.exit(1);
  }

  const files = Object.keys(archive.files);
  console.log(`\nPackage contents (${files.length} files):`);
  files.forEach(f => {
    const file = archive.files[f];
    if (!file.dir) {
      console.log(`  - ${f} (${formatBytes(file._data?.uncompressedSize || 0)})`);
    }
  });

  // Validate required files
  const requiredFiles = ['manifest.json', 'runtime.wasm'];
  const missingFiles = requiredFiles.filter(f => !files.includes(f));

  if (missingFiles.length > 0) {
    console.error(`\n✗ Missing required files: ${missingFiles.join(', ')}`);
    process.exit(1);
  }
  console.log(`\n✓ All required files present`);

  // Validate manifest.json
  console.log('\nValidating manifest.json...');
  const manifestFile = archive.files['manifest.json'];
  let manifest;
  try {
    const manifestText = await manifestFile.async('text');
    manifest = JSON.parse(manifestText);
    console.log(`✓ Valid JSON`);
  } catch (err) {
    console.error(`✗ Invalid manifest JSON: ${err.message}`);
    process.exit(1);
  }

  // Check manifest fields
  const errors = [];
  const warnings = [];

  if (!manifest.version) {
    errors.push('Missing field: version');
  } else if (manifest.version !== 'v1') {
    warnings.push(`Unexpected version: ${manifest.version} (expected v1)`);
  }

  if (!manifest.nodes || !Array.isArray(manifest.nodes)) {
    errors.push('Missing or invalid field: nodes');
  }

  if (!manifest.connections || !Array.isArray(manifest.connections)) {
    errors.push('Missing or invalid field: connections');
  }

  if (!manifest.runtime) {
    warnings.push('Missing runtime configuration');
  } else {
    if (!manifest.runtime.target) {
      errors.push('Missing field: runtime.target');
    } else if (manifest.runtime.target !== 'wasm32-wasi') {
      errors.push(`Invalid runtime.target: ${manifest.runtime.target} (expected wasm32-wasi)`);
    } else {
      console.log(`✓ Runtime target: ${manifest.runtime.target}`);
    }
  }

  if (manifest.nodes) {
    console.log(`✓ Nodes: ${manifest.nodes.length}`);
    manifest.nodes.forEach((node, i) => {
      if (!node.id) errors.push(`Node ${i}: missing id`);
      if (!node.node_type) errors.push(`Node ${i}: missing node_type`);
    });

    if (errors.length === 0) {
      const nodeTypes = [...new Set(manifest.nodes.map(n => n.node_type))];
      console.log(`  Node types: ${nodeTypes.join(', ')}`);
    }
  }

  if (manifest.connections) {
    console.log(`✓ Connections: ${manifest.connections.length}`);
  }

  if (manifest.metadata) {
    console.log(`✓ Metadata:`);
    if (manifest.metadata.name) console.log(`  Name: ${manifest.metadata.name}`);
    if (manifest.metadata.description) console.log(`  Description: ${manifest.metadata.description}`);
    if (manifest.metadata.author) console.log(`  Author: ${manifest.metadata.author}`);
    if (manifest.metadata.version) console.log(`  Version: ${manifest.metadata.version}`);
  }

  // Validate runtime.wasm
  console.log('\nValidating runtime.wasm...');
  const wasmFile = archive.files['runtime.wasm'];
  let wasmBuffer;
  try {
    wasmBuffer = await wasmFile.async('arraybuffer');
    console.log(`✓ WASM binary extracted (${formatBytes(wasmBuffer.byteLength)})`);
  } catch (err) {
    console.error(`✗ Failed to extract WASM: ${err.message}`);
    process.exit(1);
  }

  // Check WASM magic number
  const wasmBytes = new Uint8Array(wasmBuffer);
  const magicNumber = [0x00, 0x61, 0x73, 0x6d]; // "\0asm"
  const hasMagic = magicNumber.every((byte, i) => wasmBytes[i] === byte);

  if (!hasMagic) {
    errors.push('Invalid WASM magic number');
  } else {
    console.log(`✓ Valid WASM magic number`);
  }

  // Check metadata.json (optional)
  if (files.includes('metadata.json')) {
    console.log('\nValidating metadata.json (optional)...');
    const metadataFile = archive.files['metadata.json'];
    try {
      const metadataText = await metadataFile.async('text');
      const metadata = JSON.parse(metadataText);
      console.log(`✓ Valid JSON`);

      if (metadata.package_name) console.log(`  Package: ${metadata.package_name}`);
      if (metadata.package_version) console.log(`  Version: ${metadata.package_version}`);
      if (metadata.runtime_info) {
        const info = metadata.runtime_info;
        console.log(`  Runtime info:`);
        console.log(`    WASM size: ${formatBytes(info.wasm_size_bytes || 0)}`);
        console.log(`    Has Python nodes: ${info.has_python_nodes || false}`);
        console.log(`    Has Rust nodes: ${info.has_rust_nodes || false}`);
      }
    } catch (err) {
      warnings.push(`Invalid metadata.json: ${err.message}`);
    }
  }

  // Print summary
  console.log('\n' + '='.repeat(60));

  if (errors.length > 0) {
    console.log(`\n❌ VALIDATION FAILED (${errors.length} errors)`);
    errors.forEach(e => console.log(`  - ${e}`));
    if (warnings.length > 0) {
      console.log(`\nWarnings (${warnings.length}):`);
      warnings.forEach(w => console.log(`  - ${w}`));
    }
    process.exit(1);
  }

  if (warnings.length > 0) {
    console.log(`\n⚠️  VALIDATION PASSED WITH WARNINGS (${warnings.length})`);
    warnings.forEach(w => console.log(`  - ${w}`));
  } else {
    console.log('\n✅ VALIDATION PASSED');
  }

  console.log(`\nPackage is ready to use in browser!`);
  console.log(`Upload to the browser demo at: http://localhost:5173`);
}

function formatBytes(bytes) {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(2)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(2)} MB`;
}

// Run test
testPackage(packagePath).catch(err => {
  console.error('Test failed:', err.message);
  process.exit(1);
});
