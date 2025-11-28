#!/bin/bash
# Build script for Voice Assistant
# Filters out Windows paths to prevent linuxdeploy permission errors in WSL2

set -e

cd "$(dirname "$0")"

# Filter out Windows system paths that cause linuxdeploy to fail
if [[ "$PATH" == *"/mnt/c/WINDOWS"* ]]; then
    export PATH=$(echo "$PATH" | tr ":" "\n" | grep -v "/mnt/c/WINDOWS" | tr "\n" ":" | sed 's/:$//')
    echo "Filtered Windows paths from PATH for WSL2 compatibility"
fi

npm run tauri build "$@"
