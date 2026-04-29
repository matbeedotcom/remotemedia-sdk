#!/usr/bin/env bash
# Build + run the Qwen S2S WebRTC server with Live2D avatar lip-sync
# (`qwen_s2s_avatar_webrtc_server` example).
#
# Resolves all model paths to absolute paths from the workspace root so
# the cubism-core-sys build script (which runs in its own crate dir)
# and the Audio2Face/Live2D loaders (which may run in spawned tasks
# with different cwds) all find their assets.
#
# Override any of the asset paths or runtime knobs by exporting them
# before invoking this script. Defaults below assume `install-audio2face.sh`
# and `install-live2d-aria.sh` have run, and the Qwen GGUF model lives
# under `./models/`.
#
# Usage:
#   scripts/run-avatar-s2s.sh                 # build + run on :8082
#   scripts/run-avatar-s2s.sh --port 9000     # custom port
#   scripts/run-avatar-s2s.sh --build-only    # build, don't launch
#   scripts/run-avatar-s2s.sh --release       # release build (much faster)
#
# Env-var overrides (all optional):
#   LIVE2D_CUBISM_CORE_DIR    Cubism SDK root         (default: sdk/CubismSdkForNative-5-r.5)
#   LIVE2D_AVATAR_MODEL_PATH  Aria .model3.json       (default: models/live2d/aria/aria.model3.json)
#   AUDIO2FACE_BUNDLE_PATH    persona-engine bundle   (default: models/audio2face)
#   QWEN_MODEL_PATH           GGUF model              (default: unsloth/Qwen3.6-27B-GGUF:UD-Q4_K_XL)
#   AUDIO2FACE_IDENTITY       Claire | James | Mark   (default: Claire)
#   AVATAR_WIDTH/HEIGHT       Render size             (default: 512x512)
#   KOKORO_VOICE              Kokoro voice            (default: af_heart)
#   PORT                      Listen port             (default: 8082; --port flag overrides)

set -euo pipefail

# ── Locate workspace root (script is in scripts/, root is one up) ───────────
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
WORKSPACE_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
cd "${WORKSPACE_ROOT}"

# ── Defaults ─────────────────────────────────────────────────────────────────
: "${LIVE2D_CUBISM_CORE_DIR:=${WORKSPACE_ROOT}/sdk/CubismSdkForNative-5-r.5}"
: "${LIVE2D_AVATAR_MODEL_PATH:=${WORKSPACE_ROOT}/models/live2d/aria/aria.model3.json}"
: "${AUDIO2FACE_BUNDLE_PATH:=${WORKSPACE_ROOT}/models/audio2face}"
: "${QWEN_MODEL_PATH:=unsloth/Qwen3.6-27B-GGUF:UD-Q4_K_XL}"
: "${PORT:=8082}"

# Optional knobs — only export if set so the example's own defaults
# stay authoritative.
export LIVE2D_CUBISM_CORE_DIR
export LIVE2D_AVATAR_MODEL_PATH
export AUDIO2FACE_BUNDLE_PATH
export QWEN_MODEL_PATH
[[ -n "${AUDIO2FACE_IDENTITY:-}" ]] && export AUDIO2FACE_IDENTITY
[[ -n "${AVATAR_WIDTH:-}" ]] && export AVATAR_WIDTH
[[ -n "${AVATAR_HEIGHT:-}" ]] && export AVATAR_HEIGHT
[[ -n "${KOKORO_VOICE:-}" ]] && export KOKORO_VOICE

# ── Parse args ───────────────────────────────────────────────────────────────
BUILD_ONLY=0
PROFILE_FLAG=""
EXTRA_ARGS=()
while [[ $# -gt 0 ]]; do
    case "$1" in
        --port|-p)
            PORT="$2"
            shift 2
            ;;
        --build-only)
            BUILD_ONLY=1
            shift
            ;;
        --release)
            PROFILE_FLAG="--release"
            shift
            ;;
        -h|--help)
            sed -n '2,30p' "$0"
            exit 0
            ;;
        *)
            EXTRA_ARGS+=("$1")
            shift
            ;;
    esac
done

# ── Sanity-check assets ──────────────────────────────────────────────────────
fail=0
check_path() {
    local label="$1" path="$2" hint="$3"
    if [[ ! -e "${path}" ]]; then
        echo "ERROR: ${label} missing: ${path}" >&2
        echo "       ${hint}" >&2
        fail=1
    fi
}
check_path "Cubism SDK"       "${LIVE2D_CUBISM_CORE_DIR}/Core/include/Live2DCubismCore.h" \
                              "Unpack CubismSdkForNative-*.zip into sdk/ (license-gated)."
check_path "Aria model3.json" "${LIVE2D_AVATAR_MODEL_PATH}" \
                              "Run scripts/install-live2d-aria.sh, or set LIVE2D_AVATAR_MODEL_PATH."
check_path "Audio2Face bundle" "${AUDIO2FACE_BUNDLE_PATH}/network.onnx" \
                              "Run scripts/install-audio2face.sh, or set AUDIO2FACE_BUNDLE_PATH."
[[ ${fail} -ne 0 ]] && exit 1

# ── Build ────────────────────────────────────────────────────────────────────
echo "[run-avatar-s2s] building (${PROFILE_FLAG:-debug})…"
echo "  LIVE2D_CUBISM_CORE_DIR    = ${LIVE2D_CUBISM_CORE_DIR}"
echo "  LIVE2D_AVATAR_MODEL_PATH  = ${LIVE2D_AVATAR_MODEL_PATH}"
echo "  AUDIO2FACE_BUNDLE_PATH    = ${AUDIO2FACE_BUNDLE_PATH}"
echo "  QWEN_MODEL_PATH           = ${QWEN_MODEL_PATH}"
echo

cargo build \
    -p remotemedia-webrtc \
    --example qwen_s2s_avatar_webrtc_server \
    --features ws-signaling,avatar \
    ${PROFILE_FLAG}

if [[ ${BUILD_ONLY} -eq 1 ]]; then
    echo "[run-avatar-s2s] --build-only → done."
    exit 0
fi

# ── Run ──────────────────────────────────────────────────────────────────────
echo
echo "[run-avatar-s2s] launching on port ${PORT}…"
exec cargo run \
    -p remotemedia-webrtc \
    --example qwen_s2s_avatar_webrtc_server \
    --features ws-signaling,avatar \
    ${PROFILE_FLAG} \
    -- --port "${PORT}" "${EXTRA_ARGS[@]}"
