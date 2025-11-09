#!/usr/bin/env node
/**
 * Build Python stdlib virtual filesystem for browser
 *
 * This script reads the Python stdlib from runtime/target/wasm32-wasi/wasi-deps/usr
 * and creates a JSON representation that can be loaded in the browser.
 */

import fs from 'fs';
import path from 'path';
import { fileURLToPath } from 'url';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

const PYTHON_STDLIB_ROOT = path.join(__dirname, '../../runtime/target/wasm32-wasi/wasi-deps/usr');
const OUTPUT_FILE = path.join(__dirname, '../public/python-stdlib.json');

/**
 * Recursively read directory structure and file contents
 */
function buildFileTree(dirPath, relativePath = '') {
  const entries = fs.readdirSync(dirPath, { withFileTypes: true });
  const result = {};

  for (const entry of entries) {
    const fullPath = path.join(dirPath, entry.name);
    const entryRelativePath = path.join(relativePath, entry.name);

    if (entry.isDirectory()) {
      // Recursively process directory
      result[entry.name] = {
        type: 'directory',
        contents: buildFileTree(fullPath, entryRelativePath)
      };
    } else if (entry.isFile()) {
      // Read file contents as base64 for binary files, utf8 for text
      const ext = path.extname(entry.name).toLowerCase();
      const isBinary = ['.pyc', '.pyd', '.so', '.dll', '.dylib', '.a'].includes(ext);

      try {
        const content = fs.readFileSync(fullPath, isBinary ? 'base64' : 'utf8');
        result[entry.name] = {
          type: 'file',
          content: content,
          encoding: isBinary ? 'base64' : 'utf8'
        };
      } catch (err) {
        console.error(`Error reading file ${entryRelativePath}:`, err.message);
      }
    }
  }

  return result;
}

/**
 * Main build function
 */
function main() {
  console.log('Building Python stdlib virtual filesystem...');
  console.log(`Source: ${PYTHON_STDLIB_ROOT}`);
  console.log(`Output: ${OUTPUT_FILE}`);

  if (!fs.existsSync(PYTHON_STDLIB_ROOT)) {
    console.error(`Error: Python stdlib directory not found at ${PYTHON_STDLIB_ROOT}`);
    console.error('Please ensure the WASM binary has been built with Python stdlib.');
    process.exit(1);
  }

  // Build the file tree
  console.log('Reading directory structure...');
  const fileTree = buildFileTree(PYTHON_STDLIB_ROOT);

  // Write to JSON file
  console.log('Writing JSON file...');
  const json = JSON.stringify(fileTree, null, 2);
  fs.writeFileSync(OUTPUT_FILE, json, 'utf8');

  const sizeKB = (json.length / 1024).toFixed(2);
  const sizeMB = (json.length / 1024 / 1024).toFixed(2);

  console.log(`✓ Done! Created ${OUTPUT_FILE}`);
  console.log(`  Size: ${sizeKB} KB (${sizeMB} MB)`);
  console.log('\nNext steps:');
  console.log('  1. The JSON file will be loaded by the browser demo');
  console.log('  2. Consider compressing the file for faster loading');
  console.log('  3. The browser will cache this file after first load');
}

main();
