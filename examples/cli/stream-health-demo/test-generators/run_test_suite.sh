#!/usr/bin/env bash
#
# Stream Health Monitor - Test Suite Runner
#
# Runs the demo binary against synthetic test audio and validates
# that the correct fault events are emitted.
#
# Usage:
#   ./run_test_suite.sh              # Generate and run all tests
#   ./run_test_suite.sh --test-only  # Run tests without regenerating
#   ./run_test_suite.sh --generate   # Only generate test files
#

set -euo pipefail

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Configuration
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
WORKSPACE_ROOT="$(cd "$SCRIPT_DIR/../../../../" && pwd)"
DEMO_BINARY="$WORKSPACE_ROOT/examples/target/release/remotemedia-demo"
TEST_SUITE_DIR="$SCRIPT_DIR/test_suite"
PARSER="$SCRIPT_DIR/parse_events.py"
TIMEOUT_SECS=30

# Test counters
PASSED=0
FAILED=0
SKIPPED=0

# Print banner
print_banner() {
    echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
    echo -e "${BLUE}  Stream Health Monitor - Test Suite${NC}"
    echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
    echo ""
}

# Check prerequisites
check_prerequisites() {
    local missing=()
    
    if ! command -v python3 &> /dev/null; then
        missing+=("python3")
    fi
    
    if [ ! -f "$DEMO_BINARY" ]; then
        echo -e "${YELLOW}Demo binary not found. Building...${NC}"
        (cd "$WORKSPACE_ROOT/examples" && cargo build --release -p stream-health-demo) || {
            echo -e "${RED}ERROR: Failed to build demo binary${NC}"
            exit 1
        }
    fi
    
    if [ ! -f "$PARSER" ]; then
        echo -e "${RED}ERROR: Event parser not found: $PARSER${NC}"
        exit 1
    fi
    
    if [ ${#missing[@]} -gt 0 ]; then
        echo -e "${RED}ERROR: Missing prerequisites: ${missing[*]}${NC}"
        exit 1
    fi
}

# Generate test files
generate_tests() {
    echo -e "${BLUE}Generating test audio files...${NC}"
    python3 "$SCRIPT_DIR/fault_generator.py" --output "$TEST_SUITE_DIR" --duration 10 || {
        echo -e "${RED}ERROR: Failed to generate test files${NC}"
        exit 1
    }
    echo ""
}

# Clear demo state to avoid session limits
clear_demo_state() {
    local state_file="$HOME/.config/remotemedia/demo.json"
    if [ -f "$state_file" ]; then
        rm -f "$state_file"
    fi
}

# Run a single test
# Arguments: $1 = test file name, $2 = expected fault type
run_test() {
    local file="$1"
    local expected_fault="$2"
    local file_path="$TEST_SUITE_DIR/$file"
    
    echo -e "${YELLOW}Testing: $file${NC}"
    echo "  Fault type: $expected_fault"
    
    # Check if test should be skipped (timing-based faults)
    if [[ "$expected_fault" == "drift" ]] || [[ "$expected_fault" == "jitter" ]]; then
        echo -e "  ${YELLOW}⊘ SKIP - WAV files cannot test timing-based faults${NC}"
        SKIPPED=$((SKIPPED + 1))
        echo ""
        return 0
    fi
    
    if [ ! -f "$file_path" ]; then
        echo -e "  ${RED}✗ FAIL - Test file not found: $file_path${NC}"
        FAILED=$((FAILED + 1))
        echo ""
        return 1
    fi
    
    # Clear demo state before each test
    clear_demo_state
    
    # Run the demo binary with timeout
    local output
    if ! output=$(timeout "$TIMEOUT_SECS" "$DEMO_BINARY" -i "$file_path" --json 2>&1); then
        local exit_code=$?
        if [ $exit_code -eq 124 ]; then
            echo -e "  ${RED}✗ FAIL - Timeout after ${TIMEOUT_SECS}s${NC}"
        else
            echo -e "  ${RED}✗ FAIL - Demo binary exited with code $exit_code${NC}"
        fi
        FAILED=$((FAILED + 1))
        echo ""
        return 1
    fi
    
    # Parse events using Python parser
    local parser_output
    if ! parser_output=$(echo "$output" | python3 "$PARSER" --fault "$expected_fault" --json 2>&1); then
        # Parser returned non-zero - validation failed
        local summary errors
        summary=$(echo "$parser_output" | python3 -c "import sys,json; d=json.load(sys.stdin); print(d.get('summary', {}))" 2>/dev/null || echo "{}")
        errors=$(echo "$parser_output" | python3 -c "import sys,json; d=json.load(sys.stdin); print('\\n'.join(d.get('errors', [])))" 2>/dev/null || echo "Unknown error")
        
        # Get event counts for display
        local alert_silence alert_clipping alert_low_volume alert_channel alert_dropouts
        alert_silence=$(echo "$summary" | python3 -c "import sys,json; print(json.load(sys.stdin).get('alert.silence', 0))" 2>/dev/null || echo "0")
        alert_clipping=$(echo "$summary" | python3 -c "import sys,json; print(json.load(sys.stdin).get('alert.clipping', 0))" 2>/dev/null || echo "0")
        alert_low_volume=$(echo "$summary" | python3 -c "import sys,json; print(json.load(sys.stdin).get('alert.low_volume', 0))" 2>/dev/null || echo "0")
        alert_channel=$(echo "$summary" | python3 -c "import sys,json; print(json.load(sys.stdin).get('alert.channel_imbalance', 0))" 2>/dev/null || echo "0")
        alert_dropouts=$(echo "$summary" | python3 -c "import sys,json; print(json.load(sys.stdin).get('alert.dropouts', 0))" 2>/dev/null || echo "0")
        
        echo "  Events: sil:$alert_silence clip:$alert_clipping vol:$alert_low_volume ch:$alert_channel drop:$alert_dropouts"
        echo -e "  ${RED}✗ FAIL${NC}"
        echo "$errors" | while read -r line; do
            echo "    - $line"
        done
        FAILED=$((FAILED + 1))
        echo ""
        return 1
    fi
    
    # Success - extract summary from JSON output
    local health_count alert_silence alert_clipping alert_low_volume alert_channel alert_dropouts
    health_count=$(echo "$parser_output" | python3 -c "import sys,json; d=json.load(sys.stdin); print(d.get('summary', {}).get('health', 0))" 2>/dev/null || echo "0")
    alert_silence=$(echo "$parser_output" | python3 -c "import sys,json; d=json.load(sys.stdin); print(d.get('summary', {}).get('alert.silence', 0))" 2>/dev/null || echo "0")
    alert_clipping=$(echo "$parser_output" | python3 -c "import sys,json; d=json.load(sys.stdin); print(d.get('summary', {}).get('alert.clipping', 0))" 2>/dev/null || echo "0")
    alert_low_volume=$(echo "$parser_output" | python3 -c "import sys,json; d=json.load(sys.stdin); print(d.get('summary', {}).get('alert.low_volume', 0))" 2>/dev/null || echo "0")
    alert_channel=$(echo "$parser_output" | python3 -c "import sys,json; d=json.load(sys.stdin); print(d.get('summary', {}).get('alert.channel_imbalance', 0))" 2>/dev/null || echo "0")
    alert_dropouts=$(echo "$parser_output" | python3 -c "import sys,json; d=json.load(sys.stdin); print(d.get('summary', {}).get('alert.dropouts', 0))" 2>/dev/null || echo "0")
    
    echo "  Events: health:$health_count sil:$alert_silence clip:$alert_clipping vol:$alert_low_volume ch:$alert_channel drop:$alert_dropouts"
    echo -e "  ${GREEN}✓ PASS${NC}"
    PASSED=$((PASSED + 1))
    echo ""
}

# Run all tests
run_all_tests() {
    echo -e "${BLUE}Running health monitor tests...${NC}"
    echo ""
    
    # Define tests: filename -> expected fault type
    run_test "clean.wav" "none"
    run_test "fault_silence.wav" "silence"
    run_test "fault_low_volume.wav" "low_volume"
    run_test "fault_clipping.wav" "clipping"
    run_test "fault_one_sided.wav" "channel_imbalance"
    run_test "fault_dropouts.wav" "dropouts"
    run_test "fault_drift.wav" "drift"
    run_test "fault_jitter.wav" "jitter"
    run_test "fault_combined.wav" "combined"
}

# Print summary
print_summary() {
    echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
    echo -e "${BLUE}  Test Summary${NC}"
    echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
    echo -e "  ${GREEN}Passed:  $PASSED${NC}"
    echo -e "  ${RED}Failed:  $FAILED${NC}"
    echo -e "  ${YELLOW}Skipped: $SKIPPED${NC}"
    echo ""
    
    if [ $FAILED -gt 0 ]; then
        echo -e "${RED}Some tests failed!${NC}"
        exit 1
    else
        echo -e "${GREEN}All tests passed!${NC}"
    fi
}

# Main
main() {
    local generate_only=false
    local test_only=false
    
    # Parse arguments
    while [[ $# -gt 0 ]]; do
        case $1 in
            --generate|-g)
                generate_only=true
                shift
                ;;
            --test-only|-t)
                test_only=true
                shift
                ;;
            --help|-h)
                echo "Usage: $0 [OPTIONS]"
                echo ""
                echo "Options:"
                echo "  --generate, -g    Only generate test files"
                echo "  --test-only, -t   Run tests without regenerating"
                echo "  --help, -h        Show this help"
                exit 0
                ;;
            *)
                echo "Unknown option: $1"
                exit 1
                ;;
        esac
    done
    
    print_banner
    check_prerequisites
    
    if [ "$test_only" = false ]; then
        generate_tests
    fi
    
    if [ "$generate_only" = false ]; then
        echo -e "${YELLOW}Clearing demo state for testing...${NC}"
        clear_demo_state
        
        run_all_tests
        print_summary
    fi
}

main "$@"
