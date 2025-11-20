# AWS Deployment Guide for RemoteMedia Pipeline Nodes

This directory contains infrastructure-as-code templates and tools for deploying RemoteMedia pipeline nodes to AWS. This enables you to run lightweight orchestration locally while offloading heavy processing (GPU workloads, ML models) to cloud infrastructure.

## Architecture Overview

```
┌─────────────────────────────────────────────────────────────┐
│                      Local Machine                           │
│  ┌────────────────────────────────────────────────────────┐ │
│  │  Main Pipeline (WebRTC/gRPC Server)                    │ │
│  │  - Audio/Video routing                                 │ │
│  │  - Low-latency VAD                                     │ │
│  │  - Preprocessing                                       │ │
│  │  - RemotePipelineNode clients                         │ │
│  └──────────────┬─────────────────────────────────────────┘ │
└─────────────────┼─────────────────────────────────────────────┘
                  │ gRPC/HTTP
                  │ (encrypted via TLS)
                  │
                  ▼
┌─────────────────────────────────────────────────────────────┐
│                    AWS Cloud (us-east-1)                     │
│  ┌──────────────────────────────────────────────────────┐   │
│  │  Network Load Balancer                               │   │
│  │  (remotemedia-grpc-nlb:50051)                        │   │
│  └───────┬──────────────────────────────────────────────┘   │
│          │                                                   │
│  ┌───────▼──────────┐  ┌─────────────┐  ┌──────────────┐   │
│  │ ECS/Fargate Task │  │ ECS Task    │  │  ECS Task    │   │
│  │ gRPC Server      │  │ gRPC Server │  │  gRPC Server │   │
│  │ - Whisper STT    │  │ - Kokoro TTS│  │  - Custom ML │   │
│  │ - GPU: T4        │  │ - GPU: T4   │  │  - GPU: A10G │   │
│  └──────────────────┘  └─────────────┘  └──────────────┘   │
│                                                               │
│  ┌──────────────────────────────────────────────────────┐   │
│  │  S3 Bucket (remotemedia-manifests)                   │   │
│  │  - Pipeline manifests (JSON/YAML)                    │   │
│  │  - Model weights                                     │   │
│  └──────────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────┘
```

## Why This Architecture?

1. **Cost Optimization**: Pay for GPU compute only when needed
2. **Low Latency**: Keep latency-sensitive nodes (VAD, routing) local
3. **Scalability**: Auto-scale remote nodes based on demand
4. **Flexibility**: Deploy different pipeline configurations to different regions
5. **Development Speed**: Test locally, deploy remotely without code changes

## Deployment Options

### Option 1: AWS ECS/Fargate with Terraform (Recommended)

**Best for**: Production deployments, infrastructure versioning, multi-environment

```bash
# 1. Build and push Docker image
cd deploy-cli
cargo run -- build --push --region us-east-1

# 2. Deploy infrastructure
cargo run -- deploy --terraform --region us-east-1

# 3. Upload your pipeline manifest
cargo run -- upload-manifest --file ../../../examples/stt-pipeline.json

# 4. Get the endpoint
cd ../terraform
terraform output load_balancer_endpoint
# Output: remotemedia-grpc-nlb-abc123.elb.us-east-1.amazonaws.com:50051

# 5. Generate local config with remote node
cd ../deploy-cli
cargo run -- generate-manifest \
  --endpoint remotemedia-grpc-nlb-abc123.elb.us-east-1.amazonaws.com:50051 \
  --output local-with-remote.json

# 6. Run locally
cd ../../../../
cargo run --bin grpc-server -- --manifest examples/aws-deployment/deploy-cli/local-with-remote.json
```

**Infrastructure includes**:
- VPC with public/private subnets
- Network Load Balancer (TCP/gRPC)
- ECS Fargate cluster with auto-scaling (2-10 tasks)
- CloudWatch logging
- S3 bucket for manifests
- IAM roles with least-privilege

**Estimated cost**: ~$150-300/month (3x t4g.medium equivalent Fargate tasks)

---

### Option 2: AWS CDK (TypeScript)

**Best for**: Teams familiar with AWS CDK, programmatic infrastructure

```bash
# 1. Install dependencies
cd cdk-deployment
npm install

# 2. Bootstrap CDK (first time only)
cdk bootstrap

# 3. Deploy
cdk deploy

# 4. Use the output endpoint in your local manifest
```

**Pros**: Type-safe infrastructure, L3 constructs, faster iteration  
**Cons**: Requires Node.js, less declarative than Terraform

---

### Option 3: AWS Lambda + API Gateway

**Best for**: Sporadic workloads, cost-sensitive deployments

```bash
# 1. Install cargo-lambda
cargo install cargo-lambda

# 2. Build for Lambda
cargo lambda build --release --arm64 -p remotemedia-grpc

# 3. Deploy (using AWS SAM or Serverless Framework)
sam deploy --template-file lambda-template.yaml
```

**Example SAM template** (`lambda-template.yaml`):

```yaml
AWSTemplateFormatVersion: '2010-09-09'
Transform: AWS::Serverless-2016-10-31

Resources:
  RemoteMediaFunction:
    Type: AWS::Serverless::Function
    Properties:
      FunctionName: remotemedia-pipeline
      Runtime: provided.al2
      Handler: bootstrap
      CodeUri: ../../../target/lambda/remotemedia-grpc/
      MemorySize: 3008
      Timeout: 900
      Environment:
        Variables:
          RUST_LOG: info
      Events:
        ApiEvent:
          Type: Api
          Properties:
            Path: /pipeline
            Method: POST
```

**Pros**: Auto-scaling, pay-per-invocation, no infrastructure management  
**Cons**: Cold starts (2-5s), 15min max timeout, no persistent connections

---

### Option 4: Kubernetes (EKS) with GPU Nodes

**Best for**: Complex orchestration, multi-tenant, GPU-intensive workloads

```bash
# 1. Create EKS cluster with GPU node group
eksctl create cluster \
  --name remotemedia-cluster \
  --region us-east-1 \
  --nodegroup-name gpu-nodes \
  --node-type g4dn.xlarge \
  --nodes 2 \
  --nodes-min 1 \
  --nodes-max 5

# 2. Install NVIDIA device plugin
kubectl apply -f https://raw.githubusercontent.com/NVIDIA/k8s-device-plugin/main/nvidia-device-plugin.yml

# 3. Deploy RemoteMedia service
kubectl apply -f k8s-remote-tts.yaml

# 4. Get load balancer endpoint
kubectl get svc remotemedia-tts -n remotemedia
```

**Pros**: Advanced scheduling, GPU sharing, HPA/VPA, multi-region  
**Cons**: Operational complexity, higher costs

---

## Integration Patterns

### Pattern 1: Local Orchestration + Remote GPU Compute

**Use case**: Develop locally, offload STT/TTS to cloud GPU

```json
{
  "version": "v1",
  "nodes": [
    {
      "id": "local_vad",
      "node_type": "SileroVAD",
      "params": {"threshold": 0.5}
    },
    {
      "id": "remote_stt",
      "node_type": "RemotePipelineNode",
      "params": {
        "transport": "grpc",
        "endpoint": "lb-xyz.elb.us-east-1.amazonaws.com:50051",
        "manifest_url": "https://remotemedia-manifests.s3.amazonaws.com/whisper-large.json",
        "timeout_ms": 30000
      }
    }
  ],
  "connections": [
    {"from": "local_vad", "to": "remote_stt"}
  ]
}
```

**Latency**: ~100-200ms (network overhead + processing)  
**Cost**: Pay only when audio is detected (VAD filters silence)

---

### Pattern 2: Multi-Region Failover

**Use case**: Geographic redundancy, disaster recovery

```json
{
  "id": "multi_region_stt",
  "node_type": "RemotePipelineNode",
  "params": {
    "transport": "grpc",
    "endpoints": [
      "us-east-1.remotemedia.com:50051",
      "eu-west-1.remotemedia.com:50051",
      "ap-southeast-1.remotemedia.com:50051"
    ],
    "load_balance_strategy": "round_robin",
    "circuit_breaker": {
      "failure_threshold": 5,
      "reset_timeout_ms": 60000
    },
    "health_check": {
      "interval_ms": 5000
    }
  }
}
```

**Benefits**:
- Automatic failover on regional outage
- Lower latency via geographic proximity
- Circuit breaker prevents cascading failures

---

### Pattern 3: Hybrid On-Prem + Cloud

**Use case**: Regulated industries, data sovereignty

```json
{
  "id": "hybrid_pipeline",
  "node_type": "RemotePipelineNode",
  "params": {
    "transport": "grpc",
    "endpoints": [
      "on-prem-datacenter.internal:50051",  // Primary
      "backup.aws.remotemedia.com:50051"   // Fallback
    ],
    "load_balance_strategy": "least_connections",
    "retry": {
      "max_retries": 2,
      "backoff_ms": 500
    }
  }
}
```

**Benefits**:
- Keep sensitive data on-premises
- Cloud backup for availability
- Transparent to application logic

---

## Security Best Practices

### 1. Network Security

**Use TLS for all remote connections**:

```rust
// In your gRPC server configuration
let tls_config = ServerTlsConfig::new()
    .identity(Identity::from_pem(cert_pem, key_pem));

Server::builder()
    .tls_config(tls_config)?
    .add_service(pipeline_service)
    .serve(addr)
    .await?;
```

**Configure security groups** (Terraform):

```hcl
resource "aws_security_group_rule" "grpc_ingress" {
  type              = "ingress"
  from_port         = 50051
  to_port           = 50051
  protocol          = "tcp"
  cidr_blocks       = ["YOUR_IP/32"]  # Restrict to known IPs
  security_group_id = aws_security_group.ecs_tasks.id
}
```

### 2. Authentication

**Use API tokens** in your RemotePipelineNode config:

```json
{
  "id": "secure_remote",
  "node_type": "RemotePipelineNode",
  "params": {
    "endpoint": "secure.remotemedia.com:50051",
    "auth_token": "${REMOTEMEDIA_API_TOKEN}",
    "manifest_url": "https://..."
  }
}
```

**Set environment variable locally**:

```bash
export REMOTEMEDIA_API_TOKEN="your-secret-token"
```

**Validate in gRPC server** (interceptor):

```rust
use tonic::service::Interceptor;

pub fn check_auth(req: Request<()>) -> Result<Request<()>, Status> {
    match req.metadata().get("authorization") {
        Some(token) if token == "Bearer your-secret-token" => Ok(req),
        _ => Err(Status::unauthenticated("Invalid token")),
    }
}
```

### 3. Secrets Management

**Use AWS Secrets Manager** for production:

```hcl
resource "aws_secretsmanager_secret" "api_token" {
  name = "remotemedia/api-token"
}

resource "aws_ecs_task_definition" "grpc_server" {
  # ...
  container_definitions = jsonencode([{
    secrets = [{
      name      = "API_TOKEN"
      valueFrom = aws_secretsmanager_secret.api_token.arn
    }]
  }])
}
```

---

## Cost Optimization Strategies

### 1. Use Fargate Spot (70% cost savings)

Already configured in Terraform:

```hcl
capacity_provider_strategy {
  capacity_provider = "FARGATE_SPOT"
  weight           = 70
}
```

### 2. Scale to Zero During Off-Hours

```hcl
resource "aws_appautoscaling_scheduled_action" "scale_down_night" {
  name               = "scale-down-night"
  service_namespace  = "ecs"
  resource_id        = aws_appautoscaling_target.ecs.resource_id
  scalable_dimension = "ecs:service:DesiredCount"
  schedule           = "cron(0 22 * * ? *)"  # 10 PM

  scalable_target_action {
    min_capacity = 0
    max_capacity = 0
  }
}

resource "aws_appautoscaling_scheduled_action" "scale_up_morning" {
  name               = "scale-up-morning"
  service_namespace  = "ecs"
  resource_id        = aws_appautoscaling_target.ecs.resource_id
  scalable_dimension = "ecs:service:DesiredCount"
  schedule           = "cron(0 6 * * ? *)"  # 6 AM

  scalable_target_action {
    min_capacity = 2
    max_capacity = 10
  }
}
```

### 3. Use Reserved Capacity for Base Load

For predictable workloads, purchase Savings Plans:

```bash
# Save up to 50% for 1-year commitment
aws savingsplans create-savings-plan \
  --savings-plan-type Compute \
  --commitment 100 \
  --term OneYear
```

---

## Monitoring & Observability

### CloudWatch Metrics

The Terraform deployment automatically enables:
- ECS Container Insights
- Application logs in CloudWatch Logs
- Custom metrics (via `remotemedia_metrics`)

**View metrics**:

```bash
aws cloudwatch get-metric-statistics \
  --namespace ECS/ContainerInsights \
  --metric-name CPUUtilization \
  --dimensions Name=ServiceName,Value=remotemedia-grpc-server \
  --start-time 2024-01-01T00:00:00Z \
  --end-time 2024-01-01T23:59:59Z \
  --period 3600 \
  --statistics Average
```

### Distributed Tracing

Integrate with AWS X-Ray:

```rust
use aws_xray_sdk::tracing::{Middleware, XRayLayer};

let tracer = opentelemetry_xray::new_pipeline()
    .with_service_name("remotemedia-grpc")
    .install()?;

Server::builder()
    .layer(XRayLayer::new(tracer))
    .add_service(pipeline_service)
    .serve(addr)
    .await?;
```

---

## Troubleshooting

### Issue: Connection Timeout

**Symptoms**: `RemoteExecutionFailed: timeout after 30000ms`

**Diagnosis**:

```bash
# 1. Check service health
aws ecs describe-services \
  --cluster remotemedia-cluster \
  --services remotemedia-grpc-server

# 2. Check task logs
aws logs tail /ecs/remotemedia-grpc-server --follow

# 3. Test connectivity
grpcurl -plaintext lb-xyz.elb.us-east-1.amazonaws.com:50051 list
```

**Solutions**:
- Increase `timeout_ms` in RemotePipelineNode config
- Check security group allows your IP
- Verify NLB health checks are passing

### Issue: High Latency

**Symptoms**: P99 latency >500ms

**Diagnosis**:

```bash
# Check auto-scaling metrics
aws cloudwatch get-metric-statistics \
  --metric-name CPUUtilization \
  --namespace AWS/ECS \
  --dimensions Name=ServiceName,Value=remotemedia-grpc-server
```

**Solutions**:
- Enable circuit breaker to fail fast
- Add more endpoints for load balancing
- Increase task count or CPU allocation
- Use Fargate instead of Fargate Spot for consistency

### Issue: Cost Overruns

**Diagnosis**:

```bash
# Check current costs
aws ce get-cost-and-usage \
  --time-period Start=2024-01-01,End=2024-01-31 \
  --granularity DAILY \
  --metrics UnblendedCost \
  --filter file://filter.json
```

**Solutions**:
- Review auto-scaling policies (scale down faster)
- Use scheduled scaling for predictable patterns
- Switch to Lambda for sporadic workloads
- Enable Fargate Spot for non-critical workloads

---

## Next Steps

1. **Start with local testing**:
   ```bash
   cargo run --bin grpc-server
   ```

2. **Deploy to AWS** using Terraform:
   ```bash
   cd deploy-cli
   cargo run -- deploy --terraform
   ```

3. **Generate local config** with remote nodes:
   ```bash
   cargo run -- generate-manifest --endpoint YOUR_LB_ENDPOINT
   ```

4. **Test end-to-end**:
   ```bash
   cargo run --bin grpc-server -- --manifest local-with-remote.json
   ```

5. **Monitor and optimize**:
   - Set up CloudWatch alarms
   - Enable X-Ray tracing
   - Review cost reports weekly

---

## GPU Acceleration

For GPU-accelerated deployments (STT, TTS, ML models), see:
- **[GPU Selection Guide](./GPU-SELECTION-GUIDE.md)** - Comprehensive guide to choosing GPUs
- **Quick deploy**: `./deploy-gpu.sh t4` (T4), `./deploy-gpu.sh a10g` (A10G), etc.
- **Test setup**: `./test-gpu-setup.sh YOUR_ENDPOINT`

## Additional Resources

- [AWS ECS Best Practices](https://docs.aws.amazon.com/AmazonECS/latest/bestpracticesguide/)
- [gRPC Load Balancing](https://grpc.io/blog/grpc-load-balancing/)
- [RemoteMedia SDK Documentation](../../README.md)
- [Pipeline Manifest Reference](../../docs/manifest-format.md)
- [GPU Selection Guide](./GPU-SELECTION-GUIDE.md)

## Support

- **Issues**: [GitHub Issues](https://github.com/your-repo/issues)
- **Discussions**: [GitHub Discussions](https://github.com/your-repo/discussions)
- **Email**: support@remotemedia.dev

