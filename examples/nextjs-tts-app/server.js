#!/usr/bin/env node

/**
 * JavaScript wrapper to run server.ts with tsx
 * This allows running "node server.js" which will execute the TypeScript version
 */

const { spawn } = require('child_process');
const path = require('path');

const tsxPath = path.join(__dirname, 'node_modules', '.bin', 'tsx');
const serverPath = path.join(__dirname, 'server.ts');

const child = spawn(tsxPath, [serverPath], {
  stdio: 'inherit',
  shell: true,
  env: process.env
});

child.on('error', (error) => {
  console.error('Failed to start server:', error);
  process.exit(1);
});

child.on('exit', (code) => {
  process.exit(code || 0);
});
