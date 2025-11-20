# GPU Selection Guide for RemoteMedia on AWS

This guide explains how to request and configure specific GPUs for your RemoteMedia pipeline nodes deployed on AWS.

## Quick Reference: GPU Selection by Workload

| Workload | Recommended GPU | Instance Type | Cost/hr | Monthly (24/7) |
|----------|----------------|---------------|---------|----------------|
| **Whisper Base/Small** | NVIDIA T4 | g4dn.xlarge | $0.53 | $382 |
| **Whisper Medium** | NVIDIA T4 | g4dn.2xlarge | $0.75 | $540 |
| **Whisper Large-v3** | NVIDIA A10G | g5.xlarge | $1.01 | $727 |
| **Multiple Models** | NVIDIA A10G | g5.2xlarge | $1.21 | $872 |
| **Training/Fine-tuning** | NVIDIA V100 | p3.2xlarge | $3.06 | $2,203 |
| **Production Scale** | NVIDIA A100 | p4d.24xlarge | $32.77 | $23,594 |

## GPU Instance Type Comparison

### Consumer-Grade GPUs (Cost-Optimized)

#### **g4dn Family (NVIDIA T4)**
- **Best for**: Inference, real-time STT/TTS
- **VRAM**: 16 GB
- **FP16 Performance**: 65 TFLOPS
- **Use cases**: Whisper base/small, Kokoro TTS, small LLMs

```hcl
# Terraform
variable "gpu_instance_type" {
  default = "g4dn.xlarge"  # 1x T4, 4 vCPUs, 16GB RAM
}
```

**Available sizes**:
- `g4dn.xlarge` - 1x T4, 4 vCPUs, 16GB RAM - **$0.526/hr**
- `g4dn.2xlarge` - 1x T4, 8 vCPUs, 32GB RAM - $0.752/hr
- `g4dn.4xlarge` - 1x T4, 16 vCPUs, 64GB RAM - $1.204/hr
- `g4dn.12xlarge` - 4x T4, 48 vCPUs, 192GB RAM - $3.912/hr

---

### Professional GPUs (Balanced)

#### **g5 Family (NVIDIA A10G)**
- **Best for**: Large models, mixed workloads
- **VRAM**: 24 GB
- **FP16 Performance**: 125 TFLOPS (2x faster than T4)
- **Use cases**: Whisper large-v3, SDXL, Llama 7B/13B

```hcl
# Terraform
variable "gpu_instance_type" {
  default = "g5.xlarge"  # 1x A10G, 4 vCPUs, 16GB RAM
}
```

**Available sizes**:
- `g5.xlarge` - 1x A10G, 4 vCPUs, 16GB RAM - **$1.006/hr**
- `g5.2xlarge` - 1x A10G, 8 vCPUs, 32GB RAM - $1.212/hr
- `g5.4xlarge` - 1x A10G, 16 vCPUs, 64GB RAM - $1.624/hr
- `g5.12xlarge` - 4x A10G, 48 vCPUs, 192GB RAM - $5.672/hr
- `g5.48xlarge` - 8x A10G, 192 vCPUs, 768GB RAM - $16.288/hr

---

### Data Center GPUs (High Performance)

#### **p3 Family (NVIDIA V100)**
- **Best for**: Training, fine-tuning, research
- **VRAM**: 16 GB (32 GB on p3dn)
- **FP16 Performance**: 125 TFLOPS
- **Tensor Cores**: Yes
- **NVLink**: Yes (300 GB/s)

```hcl
# Terraform
variable "gpu_instance_type" {
  default = "p3.2xlarge"  # 1x V100, 8 vCPUs, 61GB RAM
}
```

**Available sizes**:
- `p3.2xlarge` - 1x V100 (16GB), 8 vCPUs - **$3.06/hr**
- `p3.8xlarge` - 4x V100 (16GB), 32 vCPUs - $12.24/hr
- `p3.16xlarge` - 8x V100 (16GB), 64 vCPUs - $24.48/hr
- `p3dn.24xlarge` - 8x V100 (32GB), 96 vCPUs - $31.22/hr

---

#### **p4d Family (NVIDIA A100)**
- **Best for**: Massive scale, distributed training
- **VRAM**: 40 GB
- **FP16 Performance**: 312 TFLOPS
- **Tensor Cores**: 3rd Gen
- **NVLink**: 600 GB/s

```hcl
# Terraform
variable "gpu_instance_type" {
  default = "p4d.24xlarge"  # 8x A100, 96 vCPUs
}
```

**Available sizes**:
- `p4d.24xlarge` - 8x A100 (40GB), 96 vCPUs, 1152GB RAM - **$32.77/hr**

---

## Deployment Methods

### Method 1: Terraform (ECS with GPU Instances)

**Step 1**: Configure GPU instance type

```hcl
# terraform/terraform.tfvars
gpu_instance_type  = "g4dn.xlarge"  # T4 GPU
gpu_count          = 1               # GPUs per task
min_gpu_instances  = 1               # Minimum instances
max_gpu_instances  = 5               # Maximum for auto-scaling
```

**Step 2**: Deploy infrastructure

```bash
cd examples/aws-deployment/terraform

# Enable GPU module
terraform apply \
  -var="gpu_instance_type=g5.xlarge" \
  -var="gpu_count=1" \
  -auto-approve
```

**Step 3**: Verify GPU availability

```bash
# Check ECS cluster capacity
aws ecs describe-clusters \
  --clusters remotemedia-cluster \
  --include ATTACHMENTS

# SSH into instance and test
nvidia-smi
```

---

### Method 2: Kubernetes (EKS with GPU Node Groups)

**Step 1**: Create GPU node group

```bash
eksctl create nodegroup \
  --cluster remotemedia-cluster \
  --name gpu-nodes \
  --node-type g4dn.xlarge \
  --nodes 2 \
  --nodes-min 1 \
  --nodes-max 10 \
  --node-labels "workload=gpu,gpu-type=t4" \
  --node-taints "nvidia.com/gpu=present:NoSchedule"

# Install NVIDIA device plugin
kubectl apply -f https://raw.githubusercontent.com/NVIDIA/k8s-device-plugin/main/nvidia-device-plugin.yml
```

**Step 2**: Deploy with GPU requirements

```yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: remotemedia-whisper-large
spec:
  template:
    spec:
      containers:
      - name: grpc-server
        resources:
          requests:
            nvidia.com/gpu: "1"  # Request 1 GPU
          limits:
            nvidia.com/gpu: "1"  # Limit to 1 GPU
        env:
        - name: CUDA_VISIBLE_DEVICES
          value: "0"
      
      # Node selection for specific GPU type
      nodeSelector:
        gpu-type: "t4"  # or "a10g", "v100", "a100"
      
      # Allow scheduling on GPU nodes
      tolerations:
      - key: "nvidia.com/gpu"
        operator: "Exists"
        effect: "NoSchedule"
```

**Step 3**: Verify GPU allocation

```bash
kubectl describe node <gpu-node-name> | grep nvidia.com/gpu
kubectl logs <pod-name> | grep -i cuda
```

---

### Method 3: Multiple GPU Types (Cost Optimization)

Deploy different pipelines to different GPU types based on model requirements:

```yaml
# k8s-multi-gpu-deployment.yaml
---
# Whisper Base on T4 (cheap)
apiVersion: apps/v1
kind: Deployment
metadata:
  name: whisper-base-t4
spec:
  replicas: 3
  template:
    spec:
      nodeSelector:
        gpu-type: "t4"
      containers:
      - name: whisper-base
        resources:
          limits:
            nvidia.com/gpu: "1"
---
# Whisper Large-v3 on A10G (better performance)
apiVersion: apps/v1
kind: Deployment
metadata:
  name: whisper-large-a10g
spec:
  replicas: 2
  template:
    spec:
      nodeSelector:
        gpu-type: "a10g"
      containers:
      - name: whisper-large
        resources:
          limits:
            nvidia.com/gpu: "1"
```

---

## GPU Selection by Use Case

### 1. Real-Time Voice Processing (Low Latency)

**Requirement**: <100ms inference time  
**Recommended**: **g4dn.xlarge** (T4)  
**Cost**: ~$380/month

```json
{
  "id": "realtime_stt",
  "node_type": "RemotePipelineNode",
  "params": {
    "endpoint": "t4-cluster.aws.remotemedia.com:50051",
    "manifest": {
      "nodes": [{
        "id": "whisper_base",
        "node_type": "WhisperSTT",
        "params": {
          "model": "base.en",
          "device": "cuda"
        }
      }]
    }
  }
}
```

---

### 2. High-Quality Transcription (Best Accuracy)

**Requirement**: Whisper large-v3, batch processing  
**Recommended**: **g5.xlarge** (A10G)  
**Cost**: ~$730/month

```json
{
  "id": "highquality_stt",
  "node_type": "RemotePipelineNode",
  "params": {
    "endpoint": "a10g-cluster.aws.remotemedia.com:50051",
    "manifest": {
      "nodes": [{
        "id": "whisper_large",
        "node_type": "WhisperSTT",
        "params": {
          "model": "large-v3",
          "device": "cuda",
          "compute_type": "float16"
        }
      }]
    }
  }
}
```

---

### 3. Multi-Model Pipeline (STT + TTS + Custom)

**Requirement**: Run multiple models concurrently  
**Recommended**: **g5.12xlarge** (4x A10G)  
**Cost**: ~$4,080/month

```hcl
# Terraform configuration for multi-GPU
variable "gpu_instance_type" {
  default = "g5.12xlarge"  # 4x A10G GPUs
}

variable "gpu_count" {
  default = 4  # Allocate all 4 GPUs to task
}
```

**Docker environment variables** for GPU assignment:

```yaml
environment:
  # Assign specific models to specific GPUs
  - name: WHISPER_CUDA_DEVICE
    value: "0"  # GPU 0 for Whisper
  - name: KOKORO_CUDA_DEVICE
    value: "1"  # GPU 1 for TTS
  - name: CUSTOM_MODEL_CUDA_DEVICE
    value: "2"  # GPU 2 for custom model
  - name: CUDA_VISIBLE_DEVICES
    value: "0,1,2"  # Make 3 GPUs visible
```

---

### 4. Model Training / Fine-Tuning

**Requirement**: Train Whisper on custom dataset  
**Recommended**: **p3.8xlarge** (4x V100) or **p4d.24xlarge** (8x A100)  
**Cost**: $8,813/month (p3) or $23,594/month (p4d)

```python
# Training job configuration
{
  "instance_type": "ml.p3.8xlarge",  # SageMaker
  "gpu_count": 4,
  "distributed_strategy": "DataParallel",
  "mixed_precision": True
}
```

---

## Cost Optimization Strategies

### 1. **Use Spot Instances** (70% savings)

```hcl
# Terraform
resource "aws_spot_fleet_request" "gpu_spot" {
  allocation_strategy      = "lowestPrice"
  target_capacity          = 4
  spot_price              = "0.30"  # Max price (30% of g4dn.xlarge on-demand)
  valid_until             = "2025-12-31T00:00:00Z"
  
  launch_specification {
    instance_type = "g4dn.xlarge"
    ami           = data.aws_ami.ecs_gpu_optimized.id
  }
  
  launch_specification {
    instance_type = "g4dn.2xlarge"  # Fallback
    ami           = data.aws_ami.ecs_gpu_optimized.id
  }
}
```

**Savings**: $382/month → $115/month (g4dn.xlarge)

---

### 2. **Scale to Zero During Off-Hours**

```hcl
# Scheduled scaling
resource "aws_autoscaling_schedule" "scale_down_night" {
  scheduled_action_name  = "scale-down-night"
  min_size               = 0
  max_size               = 0
  desired_capacity       = 0
  recurrence             = "0 22 * * *"  # 10 PM
  autoscaling_group_name = aws_autoscaling_group.gpu_workers.name
}

resource "aws_autoscaling_schedule" "scale_up_morning" {
  scheduled_action_name  = "scale-up-morning"
  min_size               = 1
  max_size               = 5
  desired_capacity       = 2
  recurrence             = "0 6 * * *"  # 6 AM
  autoscaling_group_name = aws_autoscaling_group.gpu_workers.name
}
```

**Savings**: 50% (12 hrs/day off)

---

### 3. **Right-Size Your GPU**

Don't overpay for GPUs you don't need:

| Model | Min VRAM | Optimal GPU | Overkill |
|-------|----------|-------------|----------|
| Whisper Tiny | 1 GB | T4 (16GB) | ❌ |
| Whisper Base | 1 GB | T4 (16GB) | ❌ |
| Whisper Small | 2 GB | T4 (16GB) | ❌ |
| Whisper Medium | 5 GB | T4 (16GB) | A10G |
| Whisper Large-v2 | 10 GB | T4 (16GB) | A10G |
| Whisper Large-v3 | 10 GB | A10G (24GB) | V100, A100 |
| Llama 7B (FP16) | 14 GB | T4 / A10G | V100 |
| Llama 13B (FP16) | 26 GB | A10G (need 2x) | V100 |

---

### 4. **Mixed GPU Fleet**

Use cheap GPUs for simple models, expensive for complex:

```bash
# Deploy multiple ECS services with different GPU types
terraform apply \
  -target=module.gpu_t4_cluster \
  -var="gpu_instance_type=g4dn.xlarge"

terraform apply \
  -target=module.gpu_a10g_cluster \
  -var="gpu_instance_type=g5.xlarge"
```

**Load balancing configuration**:

```json
{
  "id": "smart_stt",
  "node_type": "RemotePipelineNode",
  "params": {
    "endpoints": [
      "t4-whisper-base.aws.com:50051",      // Cheap, fast
      "a10g-whisper-large.aws.com:50051"    // Expensive, accurate
    ],
    "routing_strategy": "least_busy"
  }
}
```

---

## Monitoring GPU Utilization

### CloudWatch Metrics

```bash
# View GPU utilization
aws cloudwatch get-metric-statistics \
  --namespace RemoteMedia/GPU \
  --metric-name GPUUtilization \
  --dimensions Name=InstanceId,Value=i-1234567890abcdef0 \
  --start-time 2024-01-01T00:00:00Z \
  --end-time 2024-01-01T23:59:59Z \
  --period 300 \
  --statistics Average,Maximum
```

### Inside Container

```bash
# Install nvidia-smi in your Docker image
RUN apt-get update && apt-get install -y nvidia-utils

# Monitor in real-time
docker exec -it <container-id> nvidia-smi -l 1

# Log GPU metrics to CloudWatch
nvidia-smi --query-gpu=utilization.gpu,utilization.memory,temperature.gpu \
  --format=csv,noheader,nounits -l 60 | \
  while IFS=',' read gpu_util mem_util temp; do
    aws cloudwatch put-metric-data \
      --namespace RemoteMedia/GPU \
      --metric-name GPUUtilization \
      --value $gpu_util
  done
```

---

## Troubleshooting

### Issue: "nvidia-smi not found"

**Solution**: Ensure you're using GPU-optimized AMI

```bash
# Verify AMI has NVIDIA drivers
aws ec2 describe-images \
  --image-ids ami-xxxxx \
  --query 'Images[0].Name'

# Should contain "gpu" or "nvidia"
```

### Issue: "No GPU devices found"

**Solution**: Check task definition has GPU resource requirements

```json
"resourceRequirements": [{
  "type": "GPU",
  "value": "1"
}]
```

### Issue: "CUDA out of memory"

**Solutions**:
1. Reduce batch size
2. Use smaller model
3. Upgrade to larger GPU (more VRAM)
4. Enable model quantization

```python
# Enable INT8 quantization (4x less VRAM)
whisper_model = load_model("large-v3", device="cuda", compute_type="int8")
```

---

## Summary

| Priority | GPU Choice | Instance | Cost/mo | Use Case |
|----------|-----------|----------|---------|----------|
| 🥇 **Best Value** | T4 | g4dn.xlarge | $382 | Whisper base/small, real-time |
| 🥈 **Best Performance** | A10G | g5.xlarge | $727 | Whisper large, production |
| 🥉 **Best for Training** | V100 | p3.2xlarge | $2,203 | Fine-tuning, research |

**Pro tip**: Start with **g4dn.xlarge (T4)** for 90% of use cases. Only upgrade when you hit performance bottlenecks or need larger models.

---

## Next Steps

1. **Test locally** with Docker GPU support:
   ```bash
   docker run --gpus all remotemedia-grpc:latest
   ```

2. **Deploy to AWS** with GPU configuration:
   ```bash
   cd examples/aws-deployment/terraform
   terraform apply -var="gpu_instance_type=g4dn.xlarge"
   ```

3. **Monitor costs** in AWS Cost Explorer
4. **Optimize** based on utilization metrics


