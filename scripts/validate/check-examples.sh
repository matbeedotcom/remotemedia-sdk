#!/bin/bash
# Example Structure Validation Script
# Checks example documentation and categorization
# Usage: ./scripts/validate/check-examples.sh

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Track failures
FAILURES=0

echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "Example Structure Validation"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo ""

# R2.1: All examples have README.md
echo "Checking R2.1: All examples have README.md..."
missing_count=0

# Check examples in complexity tiers
for example_dir in examples/0*-*/*/ examples/by-transport/*/ examples/by-feature/*/; do
  if [ -d "$example_dir" ]; then
    if [ ! -f "$example_dir/README.md" ]; then
      echo -e "${RED}  ❌ Missing README in: $example_dir${NC}"
      ((missing_count++))
    fi
  fi
done

if [ $missing_count -gt 0 ]; then
  echo -e "${RED}  ❌ FAIL: $missing_count examples missing README.md${NC}"
  echo "  All examples must have README.md following the template"
  ((FAILURES++))
else
  echo -e "${GREEN}  ✅ PASS: All examples have README.md${NC}"
fi
echo ""

# R2.2: README template compliance
echo "Checking R2.2: README template compliance..."
violations=0
required_sections=("Prerequisites" "Quick Start" "Expected Output")

for readme in examples/0*-*/*/README.md; do
  if [ -f "$readme" ]; then
    for section in "${required_sections[@]}"; do
      if ! grep -q "## $section" "$readme"; then
        echo -e "${RED}  ❌ $readme missing section: ## $section${NC}"
        ((violations++))
      fi
    done
  fi
done

if [ $violations -gt 0 ]; then
  echo -e "${RED}  ❌ FAIL: $violations template violations found${NC}"
  echo "  Example READMEs must include all required sections"
  ((FAILURES++))
else
  echo -e "${GREEN}  ✅ PASS: All example READMEs follow template${NC}"
fi
echo ""

# R2.3: Example categorization
echo "Checking R2.3: Example categorization..."
valid_tiers="00-getting-started 01-advanced 02-applications by-transport by-feature assets audio_examples grpc_examples multiprocess nextjs-tts-app proxy_examples remote_class_execution_demo rust_runtime custom-transport"
invalid_count=0

for tier_dir in examples/*/; do
  if [ -d "$tier_dir" ]; then
    tier_name=$(basename "$tier_dir")
    if ! echo "$valid_tiers" | grep -qw "$tier_name"; then
      echo -e "${YELLOW}  ⚠️  Unexpected category directory: $tier_name${NC}"
      echo "     (This will be addressed during migration)"
      ((invalid_count++))
    fi
  fi
done

if [ $invalid_count -gt 0 ]; then
  echo -e "${YELLOW}  ⚠️  $invalid_count categories not yet migrated (warning only)${NC}"
else
  echo -e "${GREEN}  ✅ PASS: Examples in valid categories${NC}"
fi
echo ""

# R2.4: Shared assets location
echo "Checking R2.4: Shared assets location..."
if [ -d "examples/assets" ]; then
  asset_count=$(find examples/assets -type f | wc -l)
  echo -e "${GREEN}  ✅ PASS: assets/ directory exists with $asset_count files${NC}"
else
  echo -e "${YELLOW}  ⚠️  assets/ directory not yet created${NC}"
fi
echo ""

# Summary
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "Validation Summary"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"

if [ $FAILURES -eq 0 ]; then
  echo -e "${GREEN}✅ All checks passed!${NC}"
  echo ""
  echo "Example structure is compliant."
  exit 0
else
  echo -e "${RED}❌ $FAILURES check(s) failed${NC}"
  echo ""
  echo "Please fix the violations before committing."
  echo "See specs/001-repo-cleanup/contracts/validation-rules.md for details."
  exit 1
fi
