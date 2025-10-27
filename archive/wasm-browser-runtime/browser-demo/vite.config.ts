import { defineConfig } from 'vite'

export default defineConfig({
  base: process.env.VITE_BASE_PATH || '/',
  server: {
    headers: {
      'Cross-Origin-Opener-Policy': 'same-origin',
      'Cross-Origin-Embedder-Policy': 'require-corp',
    },
  },
  build: {
    target: 'es2020',
    sourcemap: false,
    rollupOptions: {
      output: {
        manualChunks: {
          'pyodide': ['pyodide'],
          'wasi-shim': ['@bjorn3/browser_wasi_shim'],
          'jszip': ['jszip'],
        },
      },
    },
  },
  optimizeDeps: {
    exclude: ['@wasmer/sdk'],
  },
})
