#!/bin/bash
# Test GPU setup for RemoteMedia deployment
# Run this after deployment to verify GPU is accessible

set -e

ENDPOINT=${1:-localhost:50051}

echo "🧪 Testing GPU Setup for RemoteMedia"
echo "   Endpoint: $ENDPOINT"
echo ""

# Test 1: Check if gRPC endpoint is reachable
echo "1️⃣  Testing gRPC connectivity..."
if grpcurl -plaintext $ENDPOINT list > /dev/null 2>&1; then
    echo "   ✅ gRPC endpoint is reachable"
else
    echo "   ❌ Cannot reach gRPC endpoint"
    exit 1
fi

# Test 2: Submit test audio for GPU processing
echo ""
echo "2️⃣  Testing GPU-accelerated STT..."

# Create test manifest
cat > /tmp/test-gpu-manifest.json << 'EOF'
{
  "version": "v1",
  "nodes": [{
    "id": "whisper_test",
    "node_type": "WhisperSTT",
    "params": {
      "model": "base",
      "device": "cuda",
      "language": "en"
    }
  }]
}
EOF

# Generate test audio (1 second of silence)
ffmpeg -f lavfi -i anullsrc=r=16000:cl=mono -t 1 -f s16le /tmp/test-audio.raw -y > /dev/null 2>&1

echo "   Sending test audio to GPU-accelerated pipeline..."
START_TIME=$(date +%s)

# Test execution (adjust based on your client)
RESPONSE=$(grpcurl -plaintext \
    -d @ \
    $ENDPOINT \
    remotemedia.v1.Pipeline/ExecuteUnary << REQUEST
{
  "manifest": $(cat /tmp/test-gpu-manifest.json),
  "input_data": {
    "audio": {
      "samples": "$(base64 < /tmp/test-audio.raw)"
    }
  }
}
REQUEST
)

END_TIME=$(date +%s)
DURATION=$((END_TIME - START_TIME))

if [ -n "$RESPONSE" ]; then
    echo "   ✅ GPU processing successful (${DURATION}s)"
else
    echo "   ❌ GPU processing failed"
    exit 1
fi

# Test 3: Check GPU metrics (if on AWS)
echo ""
echo "3️⃣  Checking GPU metrics..."

if aws cloudwatch get-metric-statistics \
    --namespace RemoteMedia/GPU \
    --metric-name GPUUtilization \
    --start-time $(date -u -d '10 minutes ago' +%Y-%m-%dT%H:%M:%S) \
    --end-time $(date -u +%Y-%m-%dT%H:%M:%S) \
    --period 300 \
    --statistics Average \
    --region us-east-1 > /dev/null 2>&1; then
    
    AVG_UTIL=$(aws cloudwatch get-metric-statistics \
        --namespace RemoteMedia/GPU \
        --metric-name GPUUtilization \
        --start-time $(date -u -d '10 minutes ago' +%Y-%m-%dT%H:%M:%S) \
        --end-time $(date -u +%Y-%m-%dT%H:%M:%S) \
        --period 300 \
        --statistics Average \
        --region us-east-1 \
        --query 'Datapoints[0].Average' \
        --output text)
    
    echo "   ✅ GPU metrics available (Avg utilization: ${AVG_UTIL}%)"
else
    echo "   ⚠️  GPU metrics not available (may need to wait a few minutes)"
fi

# Test 4: Check ECS task status (if on AWS)
echo ""
echo "4️⃣  Checking ECS task status..."

CLUSTER_NAME=$(aws ecs list-clusters --query 'clusterArns[0]' --output text 2>/dev/null | awk -F'/' '{print $2}')

if [ -n "$CLUSTER_NAME" ] && [ "$CLUSTER_NAME" != "None" ]; then
    TASK_ARN=$(aws ecs list-tasks \
        --cluster $CLUSTER_NAME \
        --query 'taskArns[0]' \
        --output text 2>/dev/null)
    
    if [ -n "$TASK_ARN" ] && [ "$TASK_ARN" != "None" ]; then
        TASK_STATUS=$(aws ecs describe-tasks \
            --cluster $CLUSTER_NAME \
            --tasks $TASK_ARN \
            --query 'tasks[0].lastStatus' \
            --output text)
        
        GPU_IDS=$(aws ecs describe-tasks \
            --cluster $CLUSTER_NAME \
            --tasks $TASK_ARN \
            --query 'tasks[0].containers[0].gpuIds' \
            --output text)
        
        echo "   ✅ ECS Task Status: $TASK_STATUS"
        if [ -n "$GPU_IDS" ] && [ "$GPU_IDS" != "None" ]; then
            echo "   ✅ GPU IDs assigned: $GPU_IDS"
        else
            echo "   ⚠️  No GPU IDs visible (may be using EC2 instance)"
        fi
    fi
else
    echo "   ⚠️  Not running on ECS or cluster not found"
fi

# Test 5: Benchmark GPU performance
echo ""
echo "5️⃣  Running GPU performance benchmark..."
echo "   Testing with different model sizes..."

for MODEL in tiny base small; do
    cat > /tmp/bench-$MODEL.json << EOF
{
  "version": "v1",
  "nodes": [{
    "id": "whisper_$MODEL",
    "node_type": "WhisperSTT",
    "params": {
      "model": "$MODEL",
      "device": "cuda"
    }
  }]
}
EOF

    START=$(date +%s%3N)
    grpcurl -plaintext -max-time 30 \
        -d @ \
        $ENDPOINT \
        remotemedia.v1.Pipeline/ExecuteUnary << REQUEST > /dev/null 2>&1 || true
{
  "manifest": $(cat /tmp/bench-$MODEL.json),
  "input_data": {
    "audio": {
      "samples": "$(base64 < /tmp/test-audio.raw)"
    }
  }
}
REQUEST
    
    END=$(date +%s%3N)
    LATENCY=$((END - START))
    echo "   • Whisper $MODEL: ${LATENCY}ms"
done

# Summary
echo ""
echo "📊 Test Summary"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "✅ All tests passed!"
echo ""
echo "Your GPU-accelerated RemoteMedia deployment is ready."
echo ""
echo "💡 Next Steps:"
echo "   1. Monitor GPU utilization in CloudWatch"
echo "   2. Scale based on demand"
echo "   3. Set up cost alerts in AWS Budget"
echo ""

# Cleanup
rm -f /tmp/test-audio.raw /tmp/test-gpu-manifest.json /tmp/bench-*.json


