#!/bin/bash
# Repository Structure Validation Script
# Checks root directory structure compliance
# Usage: ./scripts/validate/check-repo-structure.sh

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Track failures
FAILURES=0

echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "Repository Structure Validation"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo ""

# R1.1: Root Directory Item Limit
echo "Checking R1.1: Root directory item limit..."
count=$(ls -1 | grep -v '^\.' | wc -l)
echo "  Root directory items: $count"

if [ $count -gt 15 ]; then
  echo -e "${RED}  ❌ FAIL: Root has $count items (max: 15)${NC}"
  echo "  Items in root:"
  ls -1 | grep -v '^\.' | sed 's/^/    - /'
  ((FAILURES++))
else
  echo -e "${GREEN}  ✅ PASS: Root has $count items${NC}"
fi
echo ""

# R1.2: No Test Files in Root
echo "Checking R1.2: No test files in root..."
test_files=$(find . -maxdepth 1 \( -name 'test_*.py' -o -name '*_test.py' \) || true)

if [ -n "$test_files" ]; then
  echo -e "${RED}  ❌ FAIL: Test files found in root:${NC}"
  echo "$test_files" | sed 's/^/    - /'
  echo ""
  echo "  These files should be moved to:"
  echo "    - Integration tests → tests/integration/"
  echo "    - Crate tests → [crate]/tests/"
  echo "    - Example tests → examples/[category]/[example]/tests/"
  ((FAILURES++))
else
  echo -e "${GREEN}  ✅ PASS: No test files in root${NC}"
fi
echo ""

# R1.4: No Loose Scripts in Root
echo "Checking R1.4: No loose scripts in root..."
scripts=$(find . -maxdepth 1 \( -name '*.sh' -o -name '*_script.py' -o -name 'debug_*.py' -o -name 'build*.sh' \) ! -name 'setup.py' || true)

if [ -n "$scripts" ]; then
  echo -e "${RED}  ❌ FAIL: Loose scripts found in root:${NC}"
  echo "$scripts" | sed 's/^/    - /'
  echo ""
  echo "  Move scripts to:"
  echo "    - Build scripts → scripts/build/"
  echo "    - Debug scripts → scripts/debug/"
  echo "    - Test runners → scripts/test/"
  echo "    - Validation → scripts/validate/"
  ((FAILURES++))
else
  echo -e "${GREEN}  ✅ PASS: No loose scripts in root${NC}"
fi
echo ""

# R1.3: Allowed Files and Directories (Warning only)
echo "Checking R1.3: Unexpected items in root (warning only)..."
allowed_dirs="runtime-core runtime transports python-client python-grpc-client nodejs-client remotemedia service docs scripts specs openspec archive examples models webrtc-example custom_remote_service tests"
allowed_files="README.md INSTALL.md CONTRIBUTING.md CHANGELOG.md CLAUDE.md AGENTS.md PYTHON_CLIENTS.md LICENSE Cargo.toml Cargo.lock pyproject.toml package.json package-lock.json .gitignore runtime.toml.example setup.py"

WARNINGS=0
for item in *; do
  if [ -d "$item" ]; then
    if ! echo "$allowed_dirs" | grep -qw "$item"; then
      echo -e "${YELLOW}  ⚠️  Unexpected directory: $item${NC}"
      ((WARNINGS++))
    fi
  elif [ -f "$item" ]; then
    if ! echo "$allowed_files" | grep -qw "$item"; then
      echo -e "${YELLOW}  ⚠️  Unexpected file: $item${NC}"
      ((WARNINGS++))
    fi
  fi
done

if [ $WARNINGS -eq 0 ]; then
  echo -e "${GREEN}  ✅ PASS: All items in allowed lists${NC}"
else
  echo -e "${YELLOW}  ⚠️  $WARNINGS warnings (non-blocking)${NC}"
fi
echo ""

# Summary
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "Validation Summary"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"

if [ $FAILURES -eq 0 ]; then
  echo -e "${GREEN}✅ All checks passed!${NC}"
  echo ""
  echo "Root directory structure is compliant."
  exit 0
else
  echo -e "${RED}❌ $FAILURES check(s) failed${NC}"
  echo ""
  echo "Please fix the violations before committing."
  echo "See specs/001-repo-cleanup/contracts/validation-rules.md for details."
  exit 1
fi
