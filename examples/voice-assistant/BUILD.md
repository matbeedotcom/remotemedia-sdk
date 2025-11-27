# Build Instructions

## Prerequisites

### All Platforms

- Node.js 18+ and npm
- Rust 1.70+ and Cargo
- Tauri CLI: `npm install -g @tauri-apps/cli`

### Platform-Specific

#### macOS

```bash
# Install Xcode command line tools
xcode-select --install
```

#### Windows

- Visual Studio Build Tools 2019+
- WebView2 (usually pre-installed on Windows 10/11)

```powershell
# Using winget
winget install Microsoft.VisualStudio.2022.BuildTools
winget install Microsoft.EdgeWebView2Runtime
```

#### Linux

```bash
# Ubuntu/Debian
sudo apt update
sudo apt install libwebkit2gtk-4.1-dev \
    build-essential \
    curl \
    wget \
    file \
    libssl-dev \
    libayatana-appindicator3-dev \
    librsvg2-dev \
    libasound2-dev

# Fedora
sudo dnf install webkit2gtk4.1-devel \
    openssl-devel \
    curl \
    wget \
    file \
    libappindicator-gtk3-devel \
    librsvg2-devel \
    alsa-lib-devel

# Arch
sudo pacman -S webkit2gtk-4.1 \
    base-devel \
    curl \
    wget \
    file \
    openssl \
    appmenu-gtk-module \
    libappindicator-gtk3 \
    librsvg \
    alsa-lib
```

## Development Build

```bash
# Install dependencies
npm install

# Run in development mode with hot reload
npm run tauri dev
```

## Production Build

```bash
# Build optimized release
npm run tauri build
```

Output locations by platform:

- **macOS**: `src-tauri/target/release/bundle/macos/`
- **Windows**: `src-tauri/target/release/bundle/msi/`
- **Linux**: `src-tauri/target/release/bundle/deb/` and `appimage/`

## Cross-Compilation

### Building for macOS from Linux/Windows

Not directly supported. Use a macOS machine or CI with macOS runners.

### Building for Windows from Linux

```bash
# Install cross-compilation toolchain
rustup target add x86_64-pc-windows-msvc

# Build (requires Wine and Windows SDK)
npm run tauri build -- --target x86_64-pc-windows-msvc
```

### Building for Linux from macOS/Windows

```bash
# Using Docker
docker run -it --rm \
  -v $(pwd):/app \
  -w /app \
  rust:latest \
  bash -c "apt-get update && apt-get install -y libwebkit2gtk-4.1-dev && npm run tauri build"
```

## CI/CD

See `.github/workflows/` for automated build configurations.

## Signing and Notarization

### macOS

1. Obtain an Apple Developer certificate
2. Set environment variables:
   ```bash
   export APPLE_SIGNING_IDENTITY="Developer ID Application: Your Name"
   export APPLE_ID="your@email.com"
   export APPLE_PASSWORD="app-specific-password"
   ```
3. Build with signing:
   ```bash
   npm run tauri build
   ```

### Windows

1. Obtain a code signing certificate
2. Set environment variables:
   ```powershell
   $env:TAURI_SIGNING_PRIVATE_KEY = "path/to/key.pfx"
   $env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD = "password"
   ```
3. Build with signing:
   ```powershell
   npm run tauri build
   ```

## Troubleshooting

### "pkg-config not found"

```bash
# Ubuntu/Debian
sudo apt install pkg-config

# macOS
brew install pkg-config
```

### "webkit2gtk not found"

Install the WebKit development package for your distribution (see Linux prerequisites).

### "ALSA not found"

```bash
# Ubuntu/Debian
sudo apt install libasound2-dev

# Fedora
sudo dnf install alsa-lib-devel
```

### Build hangs or runs out of memory

- Increase available memory (at least 4GB recommended)
- Try building with fewer parallel jobs:
  ```bash
  CARGO_BUILD_JOBS=2 npm run tauri build
  ```
