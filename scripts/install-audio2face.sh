#!/usr/bin/env bash
# Download + verify + unpack the persona-engine Audio2Face bundle.
#
# Sources `audio2face.zip` (~632 MiB) from the persona-engine
# maintainer's HuggingFace repo, verifies its SHA-256 against the
# canonical manifest, and unpacks it under `models/audio2face/`.
# That layout matches what `Audio2FaceInference.cs` expects, so the
# Rust port (when M2.5 lands) can read it via the
# `AUDIO2FACE_TEST_BUNDLE` env var.
#
# Asset reference (canonical):
#   external/handcrafted-persona-engine/src/PersonaEngine/PersonaEngine.Lib/Assets/Manifest/install-manifest.json
#   id: "audio2face-bundle"
#   source: huggingface://fagenorn/persona-engine-assets@v1.0.0/audio2face/audio2face.zip
#
# Usage:
#   scripts/install-audio2face.sh                # default: models/audio2face/
#   scripts/install-audio2face.sh /custom/path   # alternative install dir
#
# Re-run-safe: skips download if the cached zip already verifies, and
# skips extraction if the unpacked tree already contains audio2face.onnx.

set -euo pipefail

# ── Configuration (matches install-manifest.json) ────────────────────────────
HF_REPO="fagenorn/persona-engine-assets"
HF_REVISION="v1.0.0"
HF_PATH="audio2face/audio2face.zip"
EXPECTED_SHA256="f792d64911dd0661016269cc859a91570c703bb933d5db630209269d6a016e04"
EXPECTED_BYTES=662800757

URL="https://huggingface.co/${HF_REPO}/resolve/${HF_REVISION}/${HF_PATH}"

# ── Paths ────────────────────────────────────────────────────────────────────
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
TARGET_DIR="${1:-${REPO_ROOT}/models/audio2face}"
ZIP_PATH="${TARGET_DIR}/audio2face.zip"
# `network.onnx` is the actual filename inside the zip — Audio2FaceInference.cs
# calls modelProvider.GetModelPath(ModelType.Audio2Face.Network), and the
# resolver maps that to `network.onnx`. Bundle also ships per-identity
# (Claire/James/Mark) `bs_skin_*.npz` + `model_data_*.npz` + JSON configs.
SENTINEL_FILE="${TARGET_DIR}/network.onnx"

mkdir -p "${TARGET_DIR}"

# ── Helpers ──────────────────────────────────────────────────────────────────
log() { printf '[install-audio2face] %s\n' "$*" >&2; }

sha256_of() {
    local path="$1"
    if command -v sha256sum >/dev/null 2>&1; then
        sha256sum "${path}" | awk '{print $1}'
    elif command -v shasum >/dev/null 2>&1; then
        shasum -a 256 "${path}" | awk '{print $1}'
    else
        log "ERROR: neither sha256sum nor shasum found"
        exit 1
    fi
}

bytes_of() {
    local path="$1"
    if [[ "$(uname)" == "Darwin" ]]; then
        stat -f '%z' "${path}"
    else
        stat -c '%s' "${path}"
    fi
}

# ── Step 1: short-circuit if already extracted ───────────────────────────────
if [[ -f "${SENTINEL_FILE}" ]]; then
    log "network.onnx already present at ${SENTINEL_FILE}"
    log "skipping download + extraction"
    log
    log "set this env var for tests:"
    log "  export AUDIO2FACE_TEST_BUNDLE=${TARGET_DIR}"
    exit 0
fi

# ── Step 2: download with resume + retries ───────────────────────────────────
need_download=true
if [[ -f "${ZIP_PATH}" ]]; then
    actual_bytes="$(bytes_of "${ZIP_PATH}")"
    if [[ "${actual_bytes}" == "${EXPECTED_BYTES}" ]]; then
        log "found existing ${ZIP_PATH} (${actual_bytes} bytes) — verifying SHA-256"
        actual_sha="$(sha256_of "${ZIP_PATH}")"
        if [[ "${actual_sha}" == "${EXPECTED_SHA256}" ]]; then
            log "cached zip verified; skipping download"
            need_download=false
        else
            log "cached zip SHA mismatch — redownloading"
            log "  expected: ${EXPECTED_SHA256}"
            log "  actual:   ${actual_sha}"
            rm -f "${ZIP_PATH}"
        fi
    else
        log "partial download detected (${actual_bytes}/${EXPECTED_BYTES} bytes) — resuming"
    fi
fi

if [[ "${need_download}" == "true" ]]; then
    if ! command -v curl >/dev/null 2>&1; then
        log "ERROR: curl not found"
        exit 1
    fi
    log "downloading ${URL}"
    log "  → ${ZIP_PATH} (~632 MiB; resumable)"
    # -C - resumes from existing partial file; -L follows HF's redirect
    # to the CDN; --retry handles transient 5xx; --fail surfaces 4xx
    # as a non-zero exit instead of writing an HTML error page over
    # the zip.
    curl --location \
        --fail \
        --continue-at - \
        --retry 5 \
        --retry-delay 4 \
        --retry-max-time 600 \
        --output "${ZIP_PATH}" \
        "${URL}"
fi

# ── Step 3: verify SHA-256 ───────────────────────────────────────────────────
log "verifying SHA-256 …"
actual_sha="$(sha256_of "${ZIP_PATH}")"
if [[ "${actual_sha}" != "${EXPECTED_SHA256}" ]]; then
    log "ERROR: SHA-256 mismatch — refusing to extract"
    log "  expected: ${EXPECTED_SHA256}"
    log "  actual:   ${actual_sha}"
    log "delete ${ZIP_PATH} and re-run if you want to retry the download"
    exit 1
fi
log "SHA-256 OK"

# ── Step 4: extract ──────────────────────────────────────────────────────────
if ! command -v unzip >/dev/null 2>&1; then
    log "ERROR: unzip not found (install via Homebrew/apt/etc.)"
    exit 1
fi
log "extracting into ${TARGET_DIR}"
unzip -q -o "${ZIP_PATH}" -d "${TARGET_DIR}"

# ── Step 5: verify the onnx file landed ──────────────────────────────────────
# Bundle layout per install-manifest.json: extractArchive=true, installPath=audio2face.
# The zip can either flatten directly into TARGET_DIR or contain a single
# top-level `audio2face/` folder. Handle both — the ONNX is the marker.
if [[ ! -f "${SENTINEL_FILE}" ]]; then
    nested="${TARGET_DIR}/audio2face/network.onnx"
    if [[ -f "${nested}" ]]; then
        log "flattening nested audio2face/ folder"
        # Move every entry up one level.
        ( cd "${TARGET_DIR}/audio2face" && mv -- * "${TARGET_DIR}/" )
        rmdir "${TARGET_DIR}/audio2face" 2>/dev/null || true
    fi
fi

if [[ ! -f "${SENTINEL_FILE}" ]]; then
    log "ERROR: extraction did not produce network.onnx at ${SENTINEL_FILE}"
    log "contents of ${TARGET_DIR}:"
    ls -la "${TARGET_DIR}" >&2
    exit 1
fi

log "extracted bundle inventory:"
( cd "${TARGET_DIR}" && ls -lh ) >&2

# ── Step 6: report ───────────────────────────────────────────────────────────
log
log "✅ Audio2Face bundle installed at ${TARGET_DIR}"
log
log "set this env var to enable tier-2 (real-model) Audio2Face tests:"
log "  export AUDIO2FACE_TEST_BUNDLE=${TARGET_DIR}"
log
log "the cached zip can be deleted to reclaim disk space:"
log "  rm ${ZIP_PATH}"
