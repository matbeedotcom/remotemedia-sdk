# Quick Start Guide: Deploy RemoteMedia to AWS in 15 Minutes

This guide will get you from zero to a working RemoteMedia deployment on AWS with local orchestration and remote GPU compute.

## Prerequisites

- AWS Account with CLI configured (`aws configure`)
- Docker installed locally
- Rust toolchain (`curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh`)
- Terraform installed (`brew install terraform` or [download](https://www.terraform.io/downloads))

## Step 1: Build the gRPC Server Image (5 min)

```bash
# Navigate to project root
cd remotemedia-sdk-webrtc

# Build the Docker image
docker build -t remotemedia-grpc:latest -f examples/aws-deployment/Dockerfile .

# Test locally
docker run -p 50051:50051 remotemedia-grpc:latest

# In another terminal, test the connection
grpcurl -plaintext localhost:50051 list
```

## Step 2: Push to AWS ECR (3 min)

```bash
# Get your AWS account ID
AWS_ACCOUNT_ID=$(aws sts get-caller-identity --query Account --output text)
AWS_REGION=us-east-1

# Create ECR repository (first time only)
aws ecr create-repository \
  --repository-name remotemedia-grpc \
  --region $AWS_REGION

# Login to ECR
aws ecr get-login-password --region $AWS_REGION | \
  docker login --username AWS --password-stdin \
  ${AWS_ACCOUNT_ID}.dkr.ecr.${AWS_REGION}.amazonaws.com

# Tag and push
docker tag remotemedia-grpc:latest \
  ${AWS_ACCOUNT_ID}.dkr.ecr.${AWS_REGION}.amazonaws.com/remotemedia-grpc:latest

docker push ${AWS_ACCOUNT_ID}.dkr.ecr.${AWS_REGION}.amazonaws.com/remotemedia-grpc:latest
```

## Step 3: Deploy Infrastructure with Terraform (5 min)

```bash
cd examples/aws-deployment/terraform

# Initialize Terraform
terraform init

# Review the plan
terraform plan -var="aws_region=$AWS_REGION"

# Deploy (takes ~3-4 minutes)
terraform apply -var="aws_region=$AWS_REGION" -auto-approve

# Save the output endpoint
REMOTE_ENDPOINT=$(terraform output -raw load_balancer_endpoint)
echo "Remote endpoint: $REMOTE_ENDPOINT"
```

**Expected infrastructure**:
- VPC with public/private subnets across 3 AZs
- Network Load Balancer (gRPC-compatible)
- ECS Fargate cluster with 3 tasks (auto-scaling 2-10)
- S3 bucket for pipeline manifests
- CloudWatch logging and metrics

**Estimated cost**: ~$150-200/month

## Step 4: Upload Pipeline Manifest to S3 (1 min)

```bash
# Get the S3 bucket name
MANIFEST_BUCKET=$(terraform output -raw manifest_bucket)

# Upload a sample TTS pipeline manifest
cat > whisper-stt.json << 'EOF'
{
  "version": "v1",
  "nodes": [{
    "id": "whisper",
    "node_type": "WhisperSTT",
    "params": {
      "model": "large-v3",
      "language": "en"
    }
  }]
}
EOF

aws s3 cp whisper-stt.json s3://${MANIFEST_BUCKET}/whisper-stt.json
```

## Step 5: Generate Local Configuration (1 min)

```bash
# Create local pipeline manifest with remote node
cd ../deploy-cli

cargo run -- generate-manifest \
  --endpoint $REMOTE_ENDPOINT \
  --template stt-tts \
  --output local-pipeline.json

cat local-pipeline.json
```

**Generated manifest structure**:
```json
{
  "nodes": [
    {
      "id": "vad",
      "node_type": "SileroVAD"  // Runs locally
    },
    {
      "id": "remote_stt",
      "node_type": "RemotePipelineNode",  // Runs on AWS
      "params": {
        "endpoint": "your-nlb-endpoint:50051"
      }
    }
  ]
}
```

## Step 6: Run Locally and Test (2 min)

```bash
# Go back to project root
cd ../../../

# Set your API token (optional, for auth)
export REMOTEMEDIA_API_TOKEN="your-secret-token"

# Run the gRPC server with the generated manifest
cargo run --bin grpc-server --release -- \
  --manifest examples/aws-deployment/deploy-cli/local-pipeline.json \
  --grpc-address 0.0.0.0:50052

# In another terminal, test the pipeline
cargo run --example test-client -- \
  --endpoint localhost:50052 \
  --audio-file examples/audio/test.wav
```

## Verification Checklist

✅ **ECS Service Running**:
```bash
aws ecs describe-services \
  --cluster remotemedia-cluster \
  --services remotemedia-grpc-server \
  --query 'services[0].runningCount'
```

✅ **Load Balancer Healthy**:
```bash
aws elbv2 describe-target-health \
  --target-group-arn $(terraform output -raw target_group_arn)
```

✅ **gRPC Endpoint Responding**:
```bash
grpcurl -plaintext $REMOTE_ENDPOINT list
```

✅ **Logs Available**:
```bash
aws logs tail /ecs/remotemedia-grpc-server --follow
```

## Cost Breakdown

| Resource | Cost/Month (Estimated) |
|----------|------------------------|
| ECS Fargate (3x 4vCPU, 8GB) | $140 |
| Network Load Balancer | $20 |
| Data Transfer (1TB) | $90 |
| CloudWatch Logs (10GB) | $5 |
| S3 Storage | $1 |
| **Total** | **~$256/month** |

**Cost optimization tips**:
- Use Fargate Spot: **save 70%** ($140 → $42)
- Scale to zero during off-hours: **save 50%**
- Use reserved capacity: **save 30%**

## Common Issues

### Issue: "Connection refused"

**Fix**: Wait 2-3 minutes for ECS tasks to start, then verify:
```bash
aws ecs describe-services --cluster remotemedia-cluster --services remotemedia-grpc-server
```

### Issue: "Image not found in ECR"

**Fix**: Ensure you pushed to the correct region:
```bash
aws ecr describe-images --repository-name remotemedia-grpc --region $AWS_REGION
```

### Issue: "Timeout after 30000ms"

**Fix**: Check security group allows your IP:
```bash
# Get your current IP
MY_IP=$(curl -s https://checkip.amazonaws.com)

# Add ingress rule
aws ec2 authorize-security-group-ingress \
  --group-id $(terraform output -raw security_group_id) \
  --protocol tcp \
  --port 50051 \
  --cidr ${MY_IP}/32
```

## Next Steps

1. **Add GPU support**: Change Fargate to ECS with EC2 GPU instances
2. **Multi-region deployment**: Deploy to multiple regions for lower latency
3. **Monitoring**: Set up CloudWatch alarms and dashboards
4. **CI/CD**: Use the GitHub Actions workflow for automated deployments
5. **Production hardening**: Enable TLS, add authentication, configure backups

## Cleanup (Important!)

To avoid ongoing charges:

```bash
cd examples/aws-deployment/terraform
terraform destroy -auto-approve

# Delete ECR images
aws ecr batch-delete-image \
  --repository-name remotemedia-grpc \
  --image-ids imageTag=latest
```

## Support

- 📖 [Full Documentation](./README.md)
- 💬 [GitHub Discussions](https://github.com/your-repo/discussions)
- 🐛 [Report Issues](https://github.com/your-repo/issues)

---

**Deployment time**: ~15 minutes  
**Difficulty**: Beginner  
**Cost**: ~$256/month (or ~$80/month with Fargate Spot)


