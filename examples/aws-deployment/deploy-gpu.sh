#!/bin/bash
# Quick GPU deployment script for RemoteMedia on AWS
# 
# Usage:
#   ./deploy-gpu.sh t4        # Deploy with NVIDIA T4 (cheapest)
#   ./deploy-gpu.sh a10g      # Deploy with NVIDIA A10G (best value)
#   ./deploy-gpu.sh v100      # Deploy with NVIDIA V100 (training)
#   ./deploy-gpu.sh a100      # Deploy with NVIDIA A100 (massive scale)

set -e

GPU_TYPE=${1:-t4}
AWS_REGION=${AWS_REGION:-us-east-1}
CLUSTER_NAME="remotemedia-cluster-${GPU_TYPE}"

# GPU type to instance type mapping
declare -A GPU_INSTANCES=(
    ["t4"]="g4dn.xlarge"
    ["t4-large"]="g4dn.2xlarge"
    ["a10g"]="g5.xlarge"
    ["a10g-large"]="g5.2xlarge"
    ["v100"]="p3.2xlarge"
    ["v100-large"]="p3.8xlarge"
    ["a100"]="p4d.24xlarge"
)

# Cost per hour
declare -A GPU_COSTS=(
    ["t4"]="0.526"
    ["t4-large"]="0.752"
    ["a10g"]="1.006"
    ["a10g-large"]="1.212"
    ["v100"]="3.06"
    ["v100-large"]="12.24"
    ["a100"]="32.77"
)

INSTANCE_TYPE=${GPU_INSTANCES[$GPU_TYPE]}
COST_PER_HOUR=${GPU_COSTS[$GPU_TYPE]}

if [ -z "$INSTANCE_TYPE" ]; then
    echo "❌ Unknown GPU type: $GPU_TYPE"
    echo ""
    echo "Available options:"
    echo "  t4         - NVIDIA T4 (16GB) - \$0.526/hr - Best for inference"
    echo "  t4-large   - NVIDIA T4 (16GB) - \$0.752/hr - More CPU/RAM"
    echo "  a10g       - NVIDIA A10G (24GB) - \$1.006/hr - Best value"
    echo "  a10g-large - NVIDIA A10G (24GB) - \$1.212/hr - More CPU/RAM"
    echo "  v100       - NVIDIA V100 (16GB) - \$3.06/hr - Training/research"
    echo "  v100-large - 4x V100 (16GB) - \$12.24/hr - Multi-GPU training"
    echo "  a100       - 8x A100 (40GB) - \$32.77/hr - Massive scale"
    exit 1
fi

echo "🚀 Deploying RemoteMedia with GPU: $GPU_TYPE"
echo "   Instance Type: $INSTANCE_TYPE"
echo "   Cost: \$$COST_PER_HOUR/hour (~\$$(echo "$COST_PER_HOUR * 730" | bc)/month)"
echo ""

# Build and push Docker image
echo "📦 Building GPU-enabled Docker image..."
cd "$(dirname "$0")"

docker build -t remotemedia-grpc:gpu-$GPU_TYPE -f Dockerfile.gpu ../../

# Get AWS account ID
AWS_ACCOUNT_ID=$(aws sts get-caller-identity --query Account --output text)
ECR_REPO="$AWS_ACCOUNT_ID.dkr.ecr.$AWS_REGION.amazonaws.com/remotemedia-grpc"

# Login to ECR
echo "🔐 Logging into ECR..."
aws ecr get-login-password --region $AWS_REGION | \
    docker login --username AWS --password-stdin $ECR_REPO

# Create repository if it doesn't exist
aws ecr describe-repositories --repository-names remotemedia-grpc --region $AWS_REGION 2>/dev/null || \
    aws ecr create-repository --repository-name remotemedia-grpc --region $AWS_REGION

# Tag and push
docker tag remotemedia-grpc:gpu-$GPU_TYPE $ECR_REPO:gpu-$GPU_TYPE
docker tag remotemedia-grpc:gpu-$GPU_TYPE $ECR_REPO:latest
docker push $ECR_REPO:gpu-$GPU_TYPE
docker push $ECR_REPO:latest

echo "✅ Image pushed to $ECR_REPO:gpu-$GPU_TYPE"

# Deploy with Terraform
echo ""
echo "🏗️  Deploying infrastructure with Terraform..."
cd terraform

terraform init

terraform apply \
    -var="aws_region=$AWS_REGION" \
    -var="gpu_instance_type=$INSTANCE_TYPE" \
    -var="gpu_count=1" \
    -var="cluster_name=$CLUSTER_NAME" \
    -auto-approve

# Get outputs
ENDPOINT=$(terraform output -raw load_balancer_endpoint 2>/dev/null || echo "pending")
MANIFEST_BUCKET=$(terraform output -raw manifest_bucket 2>/dev/null || echo "pending")

echo ""
echo "✅ Deployment complete!"
echo ""
echo "📊 Deployment Details:"
echo "   GPU Type: $GPU_TYPE"
echo "   Instance: $INSTANCE_TYPE"
echo "   Endpoint: $ENDPOINT"
echo "   Manifest Bucket: $MANIFEST_BUCKET"
echo ""
echo "💰 Cost Estimate:"
echo "   Hourly: \$$COST_PER_HOUR"
echo "   Daily (24/7): \$$(echo "$COST_PER_HOUR * 24" | bc)"
echo "   Monthly (730 hrs): \$$(echo "$COST_PER_HOUR * 730" | bc)"
echo ""
echo "🎯 Next Steps:"
echo ""
echo "1. Test the endpoint:"
echo "   grpcurl -plaintext $ENDPOINT list"
echo ""
echo "2. Generate local config:"
echo "   cd deploy-cli"
echo "   cargo run -- generate-manifest --endpoint $ENDPOINT"
echo ""
echo "3. Monitor GPU utilization:"
echo "   aws cloudwatch get-metric-statistics \\"
echo "     --namespace RemoteMedia/GPU \\"
echo "     --metric-name GPUUtilization \\"
echo "     --start-time \$(date -u -d '1 hour ago' +%Y-%m-%dT%H:%M:%S) \\"
echo "     --end-time \$(date -u +%Y-%m-%dT%H:%M:%S) \\"
echo "     --period 300 --statistics Average"
echo ""
echo "4. Scale down when done:"
echo "   terraform apply -var='min_gpu_instances=0' -var='max_gpu_instances=0'"
echo ""
echo "💡 Pro Tips:"
echo "   • Use Spot Instances for 70% savings (add -var='use_spot=true')"
echo "   • Scale to zero during off-hours (saves 50%)"
echo "   • Monitor costs in AWS Cost Explorer"
echo ""

