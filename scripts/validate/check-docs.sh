#!/bin/bash
# Documentation Structure Validation Script
# Checks major directory READMEs and documentation completeness
# Usage: ./scripts/validate/check-docs.sh

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Track failures
FAILURES=0

echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "Documentation Structure Validation"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo ""

# R5.1: Major directory READMEs
echo "Checking R5.1: Major directory READMEs..."
required_readmes=(
  "examples/README.md"
  "runtime-core/README.md"
  "transports/README.md"
  "scripts/README.md"
)

missing_count=0
for readme in "${required_readmes[@]}"; do
  if [ ! -f "$readme" ]; then
    echo -e "${RED}  ❌ Missing required README: $readme${NC}"
    ((missing_count++))
  else
    echo -e "${GREEN}  ✅ Found: $readme${NC}"
  fi
done

if [ $missing_count -gt 0 ]; then
  echo -e "${RED}  ❌ FAIL: $missing_count required READMEs missing${NC}"
  echo "  Major directories must have README.md explaining their purpose"
  ((FAILURES++))
else
  echo -e "${GREEN}  ✅ PASS: All major directories have READMEs${NC}"
fi
echo ""

# R5.2: Broken links check (basic)
echo "Checking R5.2: Basic broken links check..."
broken_count=0

for md_file in README.md CONTRIBUTING.md INSTALL.md; do
  if [ -f "$md_file" ]; then
    # Extract relative links (simplified)
    links=$(grep -oP '\[.*?\]\(\K[^\)]+' "$md_file" | grep -v '^http' || true)

    for link in $links; do
      # Check if file exists (handle anchors by removing #fragment)
      clean_link="${link%#*}"
      if [ -n "$clean_link" ] && [ ! -e "$clean_link" ]; then
        echo -e "${YELLOW}  ⚠️  Possible broken link in $md_file: $link${NC}"
        ((broken_count++))
      fi
    done
  fi
done

# Warning only for now (not blocking)
if [ $broken_count -gt 0 ]; then
  echo -e "${YELLOW}  ⚠️  WARNING: $broken_count potential broken links found${NC}"
  echo "  This is a warning only and will not fail the build"
else
  echo -e "${GREEN}  ✅ PASS: No obvious broken links in main documentation${NC}"
fi
echo ""

# Check for CONTRIBUTING.md
echo "Checking documentation files..."
if [ -f "CONTRIBUTING.md" ]; then
  echo -e "${GREEN}  ✅ CONTRIBUTING.md exists${NC}"
else
  echo -e "${YELLOW}  ⚠️  CONTRIBUTING.md not found${NC}"
fi

if [ -f "CHANGELOG.md" ]; then
  echo -e "${GREEN}  ✅ CHANGELOG.md exists${NC}"
else
  echo -e "${YELLOW}  ⚠️  CHANGELOG.md not found${NC}"
fi
echo ""

# Summary
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "Validation Summary"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"

if [ $FAILURES -eq 0 ]; then
  echo -e "${GREEN}✅ All checks passed!${NC}"
  echo ""
  echo "Documentation structure is compliant."
  exit 0
else
  echo -e "${RED}❌ $FAILURES check(s) failed${NC}"
  echo ""
  echo "Please fix the violations before committing."
  echo "See specs/001-repo-cleanup/contracts/validation-rules.md for details."
  exit 1
fi
