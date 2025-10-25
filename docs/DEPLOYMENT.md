# Deployment Guide

This document explains how to deploy the RemoteMedia browser demo to GitHub Pages.

## Prerequisites

- GitHub repository with Pages enabled
- Node.js 20+ and npm
- Rust toolchain with wasm32-wasip1 target

## Automated Deployment (GitHub Actions)

The repository includes a GitHub Actions workflow that automatically builds and deploys the browser demo.

### Setup

1. **Enable GitHub Pages** in your repository settings:
   - Go to `Settings` → `Pages`
   - Source: `GitHub Actions`
   - Save

2. **Push to main branch** (or `feat/pyo3-wasm-browser`):
   ```bash
   git push origin main
   ```

3. **Monitor deployment**:
   - Go to `Actions` tab in GitHub
   - Watch the "Deploy Browser Demo" workflow
   - After ~5-10 minutes, your demo will be live at:
     `https://<username>.github.io/<repo-name>/`

### Workflow Details

The GitHub Actions workflow (`.github/workflows/deploy-demo.yml`):

1. **Builds WASM runtime** from Rust source
2. **Copies WASM binary** to browser-demo/public/
3. **Builds browser demo** with Vite
4. **Deploys to GitHub Pages**

## Manual Deployment

If you prefer to deploy manually:

### 1. Build WASM Runtime

```bash
cd runtime
cargo build --target wasm32-wasip1 \
  --bin pipeline_executor_wasm \
  --no-default-features \
  --features wasm \
  --release
```

### 2. Copy WASM to Public Directory

```bash
cp runtime/target/wasm32-wasip1/release/pipeline_executor_wasm.wasm \
  browser-demo/public/
```

### 3. Build Browser Demo

```bash
cd browser-demo
npm install
npm run build:gh-pages  # Uses /remotemedia-sdk/ base path
```

### 4. Deploy to GitHub Pages

```bash
# Install gh-pages tool
npm install -g gh-pages

# Deploy dist/ directory
cd browser-demo
gh-pages -d dist
```

## Build Optimization

### WASM Binary Size

The WASM binary is ~20 MB in release mode. To optimize further:

```bash
# Install wasm-opt (from binaryen)
# https://github.com/WebAssembly/binaryen/releases

wasm-opt -O3 \
  -o runtime/target/wasm32-wasip1/release/pipeline_executor_wasm_opt.wasm \
  runtime/target/wasm32-wasip1/release/pipeline_executor_wasm.wasm
```

This can reduce size by 20-40% (20 MB → 12-15 MB).

### JavaScript Bundle Size

The Vite build splits the bundle into chunks:

- **pyodide** (~19 KB) - Pyodide loader (main runtime loaded from CDN)
- **wasi-shim** (~22 KB) - WASI polyfill for browser
- **jszip** (~97 KB) - ZIP extraction for .rmpkg
- **main** (~25 KB) - Demo UI code

Total: ~163 KB (gzipped: ~50 KB) + WASM files

## Performance Tuning

### Service Worker (Optional)

Add a service worker to cache WASM files for faster subsequent loads:

```javascript
// browser-demo/public/sw.js
self.addEventListener('install', (event) => {
  event.waitUntil(
    caches.open('remotemedia-v1').then((cache) => {
      return cache.addAll([
        '/pipeline_executor_wasm.wasm',
      ]);
    })
  );
});
```

### CDN for Static Assets

Consider hosting large files (WASM, Pyodide) on a CDN:

- **WASM**: Already optimized with ZIP compression in .rmpkg
- **Pyodide**: Already loaded from CDN (cdn.jsdelivr.net)

## Troubleshooting

### Build Fails with "wasm32-wasip1 not found"

```bash
rustup target add wasm32-wasip1
```

### GitHub Pages 404 Error

- Check that Pages is enabled in repository settings
- Ensure `Source` is set to `GitHub Actions` (not `Deploy from a branch`)
- Wait 5-10 minutes for initial deployment

### CORS Errors in Browser

The demo requires COOP/COEP headers for SharedArrayBuffer. GitHub Pages should handle this automatically. If you're deploying elsewhere, add these headers:

```
Cross-Origin-Opener-Policy: same-origin
Cross-Origin-Embedder-Policy: require-corp
```

### Large Bundle Size Warning

The WASM binary is legitimately large (~20 MB) due to embedded CPython. This is expected. Users download it once, then it's cached by the browser.

## Custom Domain

To use a custom domain:

1. Add a `CNAME` file to `browser-demo/public/`:
   ```
   demo.yourdomain.com
   ```

2. Configure DNS:
   ```
   demo.yourdomain.com  CNAME  <username>.github.io
   ```

3. Enable "Enforce HTTPS" in GitHub Pages settings

## Monitoring

### Analytics (Optional)

Add Google Analytics or Plausible to track usage:

```html
<!-- browser-demo/index.html -->
<script defer data-domain="yourdomain.com" src="https://plausible.io/js/script.js"></script>
```

### Performance Monitoring

Use Lighthouse or WebPageTest to monitor:
- Initial load time (target: <3s on 4G)
- Time to Interactive (target: <5s)
- Bundle size (target: <500 KB gzipped)

## Rollback

To rollback a deployment:

1. Go to `Actions` → `Deploy Browser Demo`
2. Find the previous successful run
3. Click `Re-run all jobs`

Or manually:

```bash
git revert HEAD
git push origin main
```

## Environment Variables

The build uses environment variables for configuration:

| Variable | Purpose | Default |
|----------|---------|---------|
| `VITE_BASE_PATH` | Base path for assets | `/` |
| `NODE_ENV` | Build mode | `production` |

GitHub Actions sets these automatically. For manual builds:

```bash
VITE_BASE_PATH=/remotemedia-sdk/ npm run build
```

## Next Steps

After deployment:

- [ ] Test the live demo in multiple browsers (Chrome, Firefox, Safari)
- [ ] Share the demo URL with users
- [ ] Monitor analytics for usage patterns
- [ ] Gather feedback for improvements

## Support

For deployment issues:

1. Check GitHub Actions logs
2. Verify GitHub Pages settings
3. Test local build: `npm run build && npm run preview`
4. Open an issue in the repository

## Resources

- [GitHub Pages Documentation](https://docs.github.com/en/pages)
- [GitHub Actions Documentation](https://docs.github.com/en/actions)
- [Vite Production Build Guide](https://vitejs.dev/guide/build.html)
