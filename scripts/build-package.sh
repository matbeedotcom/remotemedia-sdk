#!/bin/bash
# build-package.sh
# Cross-platform build script for creating .rmpkg packages
#
# Usage:
#   ./scripts/build-package.sh <manifest-path> <output-path> [options]
#
# Examples:
#   ./scripts/build-package.sh browser-demo/examples/calculator.rmpkg.json calculator.rmpkg
#   ./scripts/build-package.sh browser-demo/examples/text-processor.rmpkg.json text-processor.rmpkg --optimize

set -e  # Exit on error

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Parse arguments
MANIFEST=""
OUTPUT=""
OPTIMIZE=false
SKIP_BUILD=false
VERBOSE=false

print_usage() {
  echo ""
  echo "Usage: $0 <manifest-path> <output-path> [options]"
  echo ""
  echo "Arguments:"
  echo "  manifest-path    Path to manifest JSON file (required)"
  echo "  output-path      Output .rmpkg file path (required)"
  echo ""
  echo "Options:"
  echo "  --optimize       Run wasm-opt to reduce binary size (~30-40% reduction)"
  echo "  --skip-build     Skip WASM build, use existing binary"
  echo "  --verbose        Show detailed build output"
  echo "  --help           Show this help message"
  echo ""
  echo "Examples:"
  echo "  $0 browser-demo/examples/calculator.rmpkg.json calculator.rmpkg"
  echo "  $0 browser-demo/examples/text-processor.rmpkg.json text-processor.rmpkg --optimize"
  echo "  $0 examples/custom.json my-pipeline.rmpkg --skip-build"
  echo ""
}

# Parse command line arguments
while [[ $# -gt 0 ]]; do
  case $1 in
    --help|-h)
      print_usage
      exit 0
      ;;
    --optimize)
      OPTIMIZE=true
      shift
      ;;
    --skip-build)
      SKIP_BUILD=true
      shift
      ;;
    --verbose|-v)
      VERBOSE=true
      shift
      ;;
    *)
      if [ -z "$MANIFEST" ]; then
        MANIFEST="$1"
      elif [ -z "$OUTPUT" ]; then
        OUTPUT="$1"
      else
        echo -e "${RED}Error: Unknown argument '$1'${NC}"
        print_usage
        exit 1
      fi
      shift
      ;;
  esac
done

# Validate required arguments
if [ -z "$MANIFEST" ] || [ -z "$OUTPUT" ]; then
  echo -e "${RED}Error: Missing required arguments${NC}"
  print_usage
  exit 1
fi

# Validate manifest file exists
if [ ! -f "$MANIFEST" ]; then
  echo -e "${RED}Error: Manifest file not found: $MANIFEST${NC}"
  exit 1
fi

# Get script directory
SCRIPT_DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" && pwd )"
PROJECT_ROOT="$( cd "$SCRIPT_DIR/.." && pwd )"

echo -e "${BLUE}════════════════════════════════════════════════════════${NC}"
echo -e "${BLUE}  RemoteMedia Package Builder${NC}"
echo -e "${BLUE}════════════════════════════════════════════════════════${NC}"
echo ""
echo -e "  Manifest: ${GREEN}$MANIFEST${NC}"
echo -e "  Output:   ${GREEN}$OUTPUT${NC}"
echo -e "  Optimize: $([ "$OPTIMIZE" = true ] && echo -e "${GREEN}Yes${NC}" || echo -e "${YELLOW}No${NC}")"
echo ""

# Step 1: Build WASM runtime
WASM_PATH="$PROJECT_ROOT/runtime/target/wasm32-wasip1/release/pipeline_executor_wasm.wasm"

if [ "$SKIP_BUILD" = false ]; then
  echo -e "${BLUE}[1/4]${NC} Building WASM runtime..."

  # Check if Rust is installed
  if ! command -v cargo &> /dev/null; then
    echo -e "${RED}Error: cargo not found. Please install Rust from https://rustup.rs/${NC}"
    exit 1
  fi

  # Check if wasm32-wasip1 target is installed
  if ! rustup target list --installed | grep -q "wasm32-wasip1"; then
    echo -e "${YELLOW}Installing wasm32-wasip1 target...${NC}"
    rustup target add wasm32-wasip1
  fi

  # Build WASM
  cd "$PROJECT_ROOT/runtime"

  if [ "$VERBOSE" = true ]; then
    cargo build --target wasm32-wasip1 \
      --bin pipeline_executor_wasm \
      --no-default-features \
      --features wasm \
      --release
  else
    cargo build --target wasm32-wasip1 \
      --bin pipeline_executor_wasm \
      --no-default-features \
      --features wasm \
      --release \
      --quiet
  fi

  cd "$PROJECT_ROOT"

  WASM_SIZE=$(stat -f%z "$WASM_PATH" 2>/dev/null || stat -c%s "$WASM_PATH" 2>/dev/null)
  WASM_SIZE_MB=$(awk "BEGIN {printf \"%.2f\", $WASM_SIZE / 1024 / 1024}")
  echo -e "  ${GREEN}✓${NC} WASM binary built (${WASM_SIZE_MB} MB)"
else
  echo -e "${BLUE}[1/4]${NC} Skipping WASM build (using existing binary)..."

  if [ ! -f "$WASM_PATH" ]; then
    echo -e "${RED}Error: WASM binary not found: $WASM_PATH${NC}"
    echo -e "${YELLOW}Run without --skip-build to build the WASM binary${NC}"
    exit 1
  fi

  WASM_SIZE=$(stat -f%z "$WASM_PATH" 2>/dev/null || stat -c%s "$WASM_PATH" 2>/dev/null)
  WASM_SIZE_MB=$(awk "BEGIN {printf \"%.2f\", $WASM_SIZE / 1024 / 1024}")
  echo -e "  ${GREEN}✓${NC} Using existing WASM binary (${WASM_SIZE_MB} MB)"
fi

# Step 2: Optimize WASM (optional)
FINAL_WASM_PATH="$WASM_PATH"

if [ "$OPTIMIZE" = true ]; then
  echo -e "${BLUE}[2/4]${NC} Optimizing WASM binary..."

  # Check if wasm-opt is installed
  if ! command -v wasm-opt &> /dev/null; then
    echo -e "${YELLOW}Warning: wasm-opt not found. Install Binaryen for optimization:${NC}"
    echo -e "${YELLOW}  macOS:   brew install binaryen${NC}"
    echo -e "${YELLOW}  Linux:   apt install binaryen / pacman -S binaryen${NC}"
    echo -e "${YELLOW}  Or:      npm install -g binaryen${NC}"
    echo -e "${YELLOW}Skipping optimization...${NC}"
  else
    OPTIMIZED_WASM="$PROJECT_ROOT/runtime/target/wasm32-wasip1/release/pipeline_executor_wasm.optimized.wasm"

    if [ "$VERBOSE" = true ]; then
      wasm-opt -O3 -o "$OPTIMIZED_WASM" "$WASM_PATH"
    else
      wasm-opt -O3 -o "$OPTIMIZED_WASM" "$WASM_PATH" 2>&1 | grep -v "^$" || true
    fi

    OPTIMIZED_SIZE=$(stat -f%z "$OPTIMIZED_WASM" 2>/dev/null || stat -c%s "$OPTIMIZED_WASM" 2>/dev/null)
    OPTIMIZED_SIZE_MB=$(awk "BEGIN {printf \"%.2f\", $OPTIMIZED_SIZE / 1024 / 1024}")
    REDUCTION=$(awk "BEGIN {printf \"%.1f\", 100 - ($OPTIMIZED_SIZE * 100 / $WASM_SIZE)}")

    echo -e "  ${GREEN}✓${NC} Optimized: ${WASM_SIZE_MB} MB → ${OPTIMIZED_SIZE_MB} MB (${REDUCTION}% reduction)"

    FINAL_WASM_PATH="$OPTIMIZED_WASM"
  fi
else
  echo -e "${BLUE}[2/4]${NC} Skipping optimization (use --optimize to enable)"
fi

# Step 3: Create package
echo -e "${BLUE}[3/4]${NC} Creating .rmpkg package..."

# Check if Node.js is installed
if ! command -v node &> /dev/null; then
  echo -e "${RED}Error: node not found. Please install Node.js from https://nodejs.org/${NC}"
  exit 1
fi

# Check if npm dependencies are installed
if [ ! -d "$PROJECT_ROOT/browser-demo/node_modules" ]; then
  echo -e "${YELLOW}Installing npm dependencies...${NC}"
  cd "$PROJECT_ROOT/browser-demo"
  npm install --silent
  cd "$PROJECT_ROOT"
fi

# Run packaging script
cd "$PROJECT_ROOT/browser-demo"

if [ "$VERBOSE" = true ]; then
  node scripts/create-package.js \
    --manifest "$PROJECT_ROOT/$MANIFEST" \
    --wasm "$FINAL_WASM_PATH" \
    --output "$PROJECT_ROOT/$OUTPUT"
else
  node scripts/create-package.js \
    --manifest "$PROJECT_ROOT/$MANIFEST" \
    --wasm "$FINAL_WASM_PATH" \
    --output "$PROJECT_ROOT/$OUTPUT" \
    2>&1 | grep -E "(✓|✅|Error|Warning)" || true
fi

cd "$PROJECT_ROOT"

# Step 4: Validate package
echo -e "${BLUE}[4/4]${NC} Validating package..."

cd "$PROJECT_ROOT/browser-demo"

if [ "$VERBOSE" = true ]; then
  node scripts/test-package.js "$PROJECT_ROOT/$OUTPUT"
else
  VALIDATION_OUTPUT=$(node scripts/test-package.js "$PROJECT_ROOT/$OUTPUT" 2>&1)

  if echo "$VALIDATION_OUTPUT" | grep -q "VALIDATION PASSED"; then
    echo -e "  ${GREEN}✓${NC} Package validation passed"

    # Extract key info
    PACKAGE_SIZE=$(echo "$VALIDATION_OUTPUT" | grep "Package file loaded" | sed -E 's/.*\(([^)]+)\).*/\1/')
    NODE_COUNT=$(echo "$VALIDATION_OUTPUT" | grep "Nodes:" | sed -E 's/.*Nodes: ([0-9]+).*/\1/')
    NODE_TYPES=$(echo "$VALIDATION_OUTPUT" | grep "Node types:" | sed -E 's/.*Node types: (.*)/\1/')

    echo -e "  ${GREEN}✓${NC} Package size: $PACKAGE_SIZE"
    echo -e "  ${GREEN}✓${NC} Nodes: $NODE_COUNT ($NODE_TYPES)"
  else
    echo -e "${RED}✗ Package validation failed${NC}"
    echo "$VALIDATION_OUTPUT"
    exit 1
  fi
fi

cd "$PROJECT_ROOT"

# Success summary
echo ""
echo -e "${GREEN}════════════════════════════════════════════════════════${NC}"
echo -e "${GREEN}  ✅ Package created successfully!${NC}"
echo -e "${GREEN}════════════════════════════════════════════════════════${NC}"
echo ""
echo -e "  Output: ${GREEN}$OUTPUT${NC}"
echo ""
echo -e "Next steps:"
echo -e "  1. Test locally:  ${BLUE}cd browser-demo && npm run dev${NC}"
echo -e "  2. Upload package at http://localhost:5173"
echo -e "  3. Click 'Run Pipeline' to execute"
echo ""
