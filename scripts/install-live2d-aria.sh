#!/usr/bin/env bash
# Download + verify + unpack the persona-engine Aria Live2D model.
#
# Sources `aria.zip` (~46 MiB) from the persona-engine maintainer's
# HuggingFace repo, verifies its SHA-256 against the canonical
# install-manifest, and unpacks under `models/live2d/aria/`. That
# layout matches what `Live2DRenderNode` will consume (next pass,
# M4.1) via the `LIVE2D_TEST_MODEL_PATH` env var.
#
# Asset reference (canonical):
#   external/handcrafted-persona-engine/src/PersonaEngine/PersonaEngine.Lib/Assets/Manifest/install-manifest.json
#   id: "live2d-aria"
#   source: huggingface://fagenorn/persona-engine-assets@v1.0.0/live2d/aria.zip
#
# Aria is rigged for the persona-engine's lip-sync + expression
# pipeline out of the box: VBridger lip-sync params, the canonical
# emoji→expression+motion mapping (`happy`/`Happy`, `excited_star`/
# `Excited`, …) per `external/handcrafted-persona-engine/Live2D.md`.
# That makes it the natural test asset for our M4 milestone.
#
# Note: rendering Aria requires the Cubism Core SDK to be installed
# separately — see crates/cubism-core-sys/CUBISM_SDK.md.
#
# Usage:
#   scripts/install-live2d-aria.sh                # default: models/live2d/aria/
#   scripts/install-live2d-aria.sh /custom/path   # alternative install dir
#
# Re-run-safe: skips download if the cached zip already verifies, and
# skips extraction if the unpacked tree already contains aria.model3.json.

set -euo pipefail

# ── Configuration (matches install-manifest.json) ────────────────────────────
HF_REPO="fagenorn/persona-engine-assets"
HF_REVISION="v1.0.0"
HF_PATH="live2d/aria.zip"
EXPECTED_SHA256="6b4b3b0209bf84fadc4879d95c49151793275a4096a5c7b89aa793056f3868bc"
EXPECTED_BYTES=45865703

URL="https://huggingface.co/${HF_REPO}/resolve/${HF_REVISION}/${HF_PATH}"

# ── Paths ────────────────────────────────────────────────────────────────────
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
TARGET_DIR="${1:-${REPO_ROOT}/models/live2d/aria}"
ZIP_PATH="${TARGET_DIR}/aria.zip"
# `aria.model3.json` is the canonical entry point that the Cubism
# loader resolves first; everything else (.moc3, textures, motions,
# expressions, physics) is referenced from inside it.
SENTINEL_FILE="${TARGET_DIR}/aria.model3.json"

mkdir -p "${TARGET_DIR}"

# ── Helpers ──────────────────────────────────────────────────────────────────
log() { printf '[install-live2d-aria] %s\n' "$*" >&2; }

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
    log "aria.model3.json already present at ${SENTINEL_FILE}"
    log "skipping download + extraction"
    log
    log "set this env var for tests:"
    log "  export LIVE2D_TEST_MODEL_PATH=${SENTINEL_FILE}"
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
    log "  → ${ZIP_PATH} (~46 MiB; resumable)"
    curl --location \
        --fail \
        --continue-at - \
        --retry 5 \
        --retry-delay 4 \
        --retry-max-time 300 \
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

# ── Step 5: locate model3.json ───────────────────────────────────────────────
# Bundle layout per install-manifest.json: extractArchive=true,
# installPath=live2d/aria. The zip may flatten directly into
# TARGET_DIR or contain a single top-level `aria/` folder. Handle
# both — the model3.json is the marker.
if [[ ! -f "${SENTINEL_FILE}" ]]; then
    nested="${TARGET_DIR}/aria/aria.model3.json"
    if [[ -f "${nested}" ]]; then
        log "flattening nested aria/ folder"
        ( cd "${TARGET_DIR}/aria" && mv -- * "${TARGET_DIR}/" )
        rmdir "${TARGET_DIR}/aria" 2>/dev/null || true
    fi
fi

# Some bundles ship with a different model3.json filename (e.g.
# `Aria.model3.json` capitalized, or stem matches the parent dir).
# Re-resolve via glob if the canonical name didn't land.
if [[ ! -f "${SENTINEL_FILE}" ]]; then
    found="$(find "${TARGET_DIR}" -maxdepth 3 -iname '*.model3.json' -print -quit 2>/dev/null || true)"
    if [[ -n "${found}" ]]; then
        log "found model3.json at ${found}; using it as the test target"
        SENTINEL_FILE="${found}"
    fi
fi

if [[ ! -f "${SENTINEL_FILE}" ]]; then
    log "ERROR: extraction did not produce a *.model3.json under ${TARGET_DIR}"
    log "contents of ${TARGET_DIR}:"
    ls -la "${TARGET_DIR}" >&2
    exit 1
fi

log "extracted bundle inventory:"
( cd "${TARGET_DIR}" && find . -maxdepth 3 -type f -printf '%P\n' 2>/dev/null \
    || find . -maxdepth 3 -type f ) >&2

# ── Step 6: report ───────────────────────────────────────────────────────────
log
log "✅ Aria Live2D model installed at ${TARGET_DIR}"
log
log "set this env var to enable tier-2 (real-model) Live2D tests:"
log "  export LIVE2D_TEST_MODEL_PATH=${SENTINEL_FILE}"
log
log "rendering also requires the Cubism Core SDK; see"
log "  crates/cubism-core-sys/CUBISM_SDK.md"
log
log "the cached zip can be deleted to reclaim disk space:"
log "  rm ${ZIP_PATH}"
