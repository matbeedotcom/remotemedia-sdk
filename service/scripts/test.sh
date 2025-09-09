#!/bin/bash
# Test script for RemoteMedia Remote Execution Service

set -e

# Configuration
SERVICE_PORT=50051
TEST_TIMEOUT=60

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

echo -e "${GREEN}Testing RemoteMedia Remote Execution Service${NC}"
echo "=============================================="

# Check if we're in the right directory
if [ ! -f "src/server.py" ]; then
    echo -e "${RED}Error: server.py not found. Are you in the remote_service directory?${NC}"
    exit 1
fi

# Function to check if service is running
check_service() {
    local retries=10
    local count=0
    
    echo -e "${YELLOW}Waiting for service to start...${NC}"
    
    while [ $count -lt $retries ]; do
        if python src/health_check.py 2>/dev/null; then
            echo -e "${GREEN}Service is ready!${NC}"
            return 0
        fi
        
        count=$((count + 1))
        echo "Attempt $count/$retries..."
        sleep 2
    done
    
    echo -e "${RED}Service failed to start within timeout${NC}"
    return 1
}

# Function to run unit tests
run_unit_tests() {
    echo -e "${YELLOW}Running unit tests...${NC}"
    
    if [ -d "tests" ]; then
        cd tests
        python -m pytest -v . || {
            echo -e "${RED}Unit tests failed${NC}"
            cd ..
            return 1
        }
        cd ..
        echo -e "${GREEN}Unit tests passed${NC}"
    else
        echo -e "${YELLOW}No unit tests found, skipping${NC}"
    fi
}

# Function to run integration tests
run_integration_tests() {
    echo -e "${YELLOW}Running integration tests...${NC}"
    
    # Go to project root to run integration tests
    cd ..
    
    if [ -f "tests/test_remote_execution.py" ]; then
        python tests/test_remote_execution.py --manual || {
            echo -e "${RED}Integration tests failed${NC}"
            cd remote_service
            return 1
        }
        echo -e "${GREEN}Integration tests passed${NC}"
    else
        echo -e "${YELLOW}No integration tests found, skipping${NC}"
    fi
    
    cd remote_service
}

# Function to test gRPC endpoints
test_grpc_endpoints() {
    echo -e "${YELLOW}Testing gRPC endpoints...${NC}"
    
    # Test with grpcurl if available
    if command -v grpcurl &> /dev/null; then
        echo "Testing with grpcurl..."
        
        # Test health check
        grpcurl -plaintext localhost:$SERVICE_PORT grpc.health.v1.Health/Check || {
            echo -e "${YELLOW}Health check via grpcurl failed (this might be expected)${NC}"
        }
        
        # Test status endpoint
        grpcurl -plaintext localhost:$SERVICE_PORT remotemedia.execution.RemoteExecutionService/GetStatus || {
            echo -e "${YELLOW}Status endpoint test failed${NC}"
        }
    else
        echo -e "${YELLOW}grpcurl not available, skipping gRPC endpoint tests${NC}"
    fi
}

# Main test execution
main() {
    local service_pid=""
    local exit_code=0
    
    # Generate gRPC code if needed
    if [ ! -f "src/execution_pb2.py" ]; then
        echo -e "${YELLOW}Generating gRPC code...${NC}"
        python -m grpc_tools.protoc \
            --proto_path=protos \
            --python_out=src \
            --grpc_python_out=src \
            protos/*.proto
    fi
    
    # Start the service in background
    echo -e "${YELLOW}Starting service for testing...${NC}"
    export GRPC_PORT=$SERVICE_PORT
    export LOG_LEVEL=INFO
    export SANDBOX_ENABLED=false  # Disable sandboxing for testing
    export MAX_WORKERS=2
    
    cd src
    python server.py &
    service_pid=$!
    cd ..
    
    # Wait for service to be ready
    if ! check_service; then
        echo -e "${RED}Service failed to start${NC}"
        kill $service_pid 2>/dev/null || true
        exit 1
    fi
    
    # Run tests
    echo -e "${GREEN}Service started successfully (PID: $service_pid)${NC}"
    echo ""
    
    # Run unit tests
    run_unit_tests || exit_code=1
    echo ""
    
    # Run integration tests
    run_integration_tests || exit_code=1
    echo ""
    
    # Test gRPC endpoints
    test_grpc_endpoints
    echo ""
    
    # Cleanup
    echo -e "${YELLOW}Stopping service...${NC}"
    kill $service_pid 2>/dev/null || true
    wait $service_pid 2>/dev/null || true
    
    if [ $exit_code -eq 0 ]; then
        echo -e "${GREEN}All tests passed!${NC}"
    else
        echo -e "${RED}Some tests failed${NC}"
    fi
    
    return $exit_code
}

# Handle script interruption
trap 'echo -e "\n${YELLOW}Test interrupted${NC}"; kill $service_pid 2>/dev/null || true; exit 1' INT TERM

# Run main function
main "$@" 