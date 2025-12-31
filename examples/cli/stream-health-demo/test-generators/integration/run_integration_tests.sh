#!/usr/bin/env bash
#
# Stream Health Monitor - Integration Test Suite
#
# Tests the full wedge path:
#   FFmpeg → Demo Binary (streaming) → Webhook Receiver
#
# Prerequisites:
#   - ffmpeg installed
#   - python3 with http.server support
#   - Demo binary built
#
# Usage:
#   ./run_integration_tests.sh
#

set -euo pipefail

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

# Configuration
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
WORKSPACE_ROOT="$(cd "$SCRIPT_DIR/../../../../../" && pwd)"
DEMO_BINARY="$WORKSPACE_ROOT/examples/target/release/remotemedia-demo"
TEST_SUITE_DIR="$SCRIPT_DIR/../test_suite"
WEBHOOK_RECEIVER="$SCRIPT_DIR/webhook_receiver.py"
WEBHOOK_PORT=8765
WEBHOOK_URL="http://127.0.0.1:$WEBHOOK_PORT"

# Cleanup function
cleanup() {
    echo -e "\n${YELLOW}Cleaning up...${NC}"
    
    # Kill webhook receiver if running
    if [ -n "${WEBHOOK_PID:-}" ] && kill -0 "$WEBHOOK_PID" 2>/dev/null; then
        kill "$WEBHOOK_PID" 2>/dev/null || true
    fi
}

trap cleanup EXIT

# Print banner
print_banner() {
    echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
    echo -e "${BLUE}  Stream Health Monitor - Integration Tests${NC}"
    echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
    echo ""
}

# Check prerequisites
check_prerequisites() {
    local missing=()
    
    if ! command -v ffmpeg &> /dev/null; then
        missing+=("ffmpeg")
    fi
    
    if ! command -v python3 &> /dev/null; then
        missing+=("python3")
    fi
    
    if ! command -v curl &> /dev/null; then
        missing+=("curl")
    fi
    
    if [ ! -f "$DEMO_BINARY" ]; then
        echo -e "${YELLOW}Demo binary not found. Building...${NC}"
        (cd "$WORKSPACE_ROOT/examples" && cargo build --release -p stream-health-demo) || {
            echo -e "${RED}ERROR: Failed to build demo binary${NC}"
            exit 1
        }
    fi
    
    if [ ${#missing[@]} -gt 0 ]; then
        echo -e "${RED}ERROR: Missing prerequisites: ${missing[*]}${NC}"
        echo ""
        echo "Install with:"
        echo "  apt install ffmpeg curl python3"
        exit 1
    fi
    
    echo -e "${GREEN}✓ Prerequisites satisfied${NC}"
}

# Start webhook receiver
start_webhook_receiver() {
    echo -e "${BLUE}Starting webhook receiver on port $WEBHOOK_PORT...${NC}"
    
    python3 "$WEBHOOK_RECEIVER" --port "$WEBHOOK_PORT" &
    WEBHOOK_PID=$!
    
    # Wait for it to be ready
    local retries=10
    while [ $retries -gt 0 ]; do
        if curl -s "$WEBHOOK_URL/health" > /dev/null 2>&1; then
            echo -e "${GREEN}✓ Webhook receiver ready${NC}"
            return 0
        fi
        sleep 0.5
        retries=$((retries - 1))
    done
    
    echo -e "${RED}ERROR: Webhook receiver failed to start${NC}"
    exit 1
}

# Clear webhook events
clear_webhooks() {
    curl -s -X POST "$WEBHOOK_URL/clear" > /dev/null
}

# Get webhook event count by type
get_webhook_count() {
    local event_type="$1"
    curl -s "$WEBHOOK_URL/summary" | python3 -c "
import sys, json
d = json.load(sys.stdin)
print(d.get('by_type', {}).get('$event_type', 0))
" 2>/dev/null || echo "0"
}

# Get total webhook count
get_total_webhooks() {
    curl -s "$WEBHOOK_URL/summary" | python3 -c "
import sys, json
print(json.load(sys.stdin).get('total', 0))
" 2>/dev/null || echo "0"
}

# Run streaming test with FFmpeg (direct pipe)
# Arguments: $1 = test file, $2 = expected event type, $3 = min expected count
run_streaming_test() {
    local test_file="$1"
    local expected_type="$2"
    local min_count="${3:-1}"
    local file_path="$TEST_SUITE_DIR/$test_file"
    
    echo -e "\n${YELLOW}Testing: $test_file (streaming)${NC}"
    echo "  Expected: $expected_type (at least $min_count)"
    
    if [ ! -f "$file_path" ]; then
        echo -e "  ${RED}✗ FAIL - Test file not found${NC}"
        return 1
    fi
    
    # Clear demo state
    rm -f "$HOME/.config/remotemedia/demo.json"
    
    # Stream test file through FFmpeg directly to demo via pipe
    # FFmpeg outputs raw PCM WAV to stdout, demo reads from stdin
    local output
    output=$(ffmpeg -i "$file_path" -f wav -acodec pcm_s16le -ar 16000 -ac 1 pipe:1 2>/dev/null | \
        "$DEMO_BINARY" -i - --json 2>&1) || true
    
    # Count events of expected type (grep -c returns 0 if no matches, with exit code 1)
    local event_count
    event_count=$(echo "$output" | grep -c "\"type\":\"$expected_type\"" 2>/dev/null) || event_count=0
    
    # Ensure event_count is a number
    event_count=${event_count//[^0-9]/}
    event_count=${event_count:-0}
    
    echo "  Detected: $event_count events"
    
    if [ "$event_count" -ge "$min_count" ]; then
        echo -e "  ${GREEN}✓ PASS${NC}"
        return 0
    else
        echo -e "  ${RED}✗ FAIL - Expected at least $min_count, got $event_count${NC}"
        # Show last few lines of output for debugging
        echo "  Debug (last 5 lines):"
        echo "$output" | tail -5 | while read -r line; do echo "    $line"; done
        return 1
    fi
}

# Run tests
run_tests() {
    local passed=0
    local failed=0
    
    echo -e "\n${BLUE}Running streaming tests...${NC}"
    echo "(FFmpeg pushes audio at real-time rate through named pipe)"
    
    # Test 1: Silence detection
    if run_streaming_test "fault_silence.wav" "silence" 1; then
        passed=$((passed + 1))
    else
        failed=$((failed + 1))
    fi
    
    # Test 2: Clipping detection
    if run_streaming_test "fault_clipping.wav" "clipping" 1; then
        passed=$((passed + 1))
    else
        failed=$((failed + 1))
    fi
    
    # Test 3: Low volume detection
    if run_streaming_test "fault_low_volume.wav" "low_volume" 1; then
        passed=$((passed + 1))
    else
        failed=$((failed + 1))
    fi
    
    # Print summary
    echo -e "\n${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
    echo -e "${BLUE}  Integration Test Summary${NC}"
    echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
    echo -e "  ${GREEN}Passed: $passed${NC}"
    echo -e "  ${RED}Failed: $failed${NC}"
    echo ""
    
    if [ $failed -gt 0 ]; then
        echo -e "${RED}Some integration tests failed${NC}"
        exit 1
    else
        echo -e "${GREEN}All integration tests passed!${NC}"
    fi
}

# Main
main() {
    print_banner
    check_prerequisites
    
    # Check if test files exist
    if [ ! -d "$TEST_SUITE_DIR" ]; then
        echo -e "${YELLOW}Test files not found. Generating...${NC}"
        (cd "$SCRIPT_DIR/.." && python3 fault_generator.py --output "$TEST_SUITE_DIR")
    fi
    
    run_tests
}

main "$@"
