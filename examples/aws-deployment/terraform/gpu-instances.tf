# GPU-enabled ECS infrastructure for RemoteMedia
# 
# This file provides GPU-accelerated compute for ML workloads (STT, TTS, etc.)
# Fargate does NOT support GPUs, so we use ECS with EC2 instances.

#
# GPU Instance Type Reference
#
# | Instance Type  | GPU Type    | GPU Memory | vCPUs | RAM   | Cost/hr | Best For              |
# |----------------|-------------|------------|-------|-------|---------|----------------------|
# | g4dn.xlarge    | NVIDIA T4   | 16 GB      | 4     | 16GB  | $0.526  | Inference (Whisper)  |
# | g4dn.2xlarge   | NVIDIA T4   | 16 GB      | 8     | 32GB  | $0.752  | Multi-model          |
# | g5.xlarge      | NVIDIA A10G | 24 GB      | 4     | 16GB  | $1.006  | Large models         |
# | p3.2xlarge     | NVIDIA V100 | 16 GB      | 8     | 61GB  | $3.06   | Training             |
# | p4d.24xlarge   | NVIDIA A100 | 40 GB      | 96    | 1152GB| $32.77  | Massive scale        |

variable "gpu_instance_type" {
  description = "EC2 instance type with GPU support"
  type        = string
  default     = "g4dn.xlarge"  # T4 GPU, good balance of cost/performance
  
  validation {
    condition = can(regex("^(g4dn|g5|p3|p4d|g4ad)\\.", var.gpu_instance_type))
    error_message = "Must be a GPU instance type (g4dn, g5, p3, p4d, g4ad)."
  }
}

variable "gpu_count" {
  description = "Number of GPUs to allocate per task"
  type        = number
  default     = 1
  
  validation {
    condition     = var.gpu_count > 0 && var.gpu_count <= 8
    error_message = "GPU count must be between 1 and 8."
  }
}

variable "min_gpu_instances" {
  description = "Minimum number of GPU instances in the cluster"
  type        = number
  default     = 1
}

variable "max_gpu_instances" {
  description = "Maximum number of GPU instances for auto-scaling"
  type        = number
  default     = 10
}

#
# Launch Template for GPU Instances
#
resource "aws_launch_template" "gpu_instances" {
  name_prefix   = "remotemedia-gpu-"
  image_id      = data.aws_ami.ecs_gpu_optimized.id
  instance_type = var.gpu_instance_type
  
  iam_instance_profile {
    name = aws_iam_instance_profile.ecs_gpu_instance.name
  }
  
  vpc_security_group_ids = [aws_security_group.ecs_tasks.id]
  
  # ECS agent configuration
  user_data = base64encode(templatefile("${path.module}/user-data.sh.tpl", {
    cluster_name = aws_ecs_cluster.remotemedia.name
    gpu_enabled  = true
  }))
  
  # Enable GPU monitoring
  monitoring {
    enabled = true
  }
  
  metadata_options {
    http_endpoint               = "enabled"
    http_tokens                 = "required"
    http_put_response_hop_limit = 1
  }
  
  tag_specifications {
    resource_type = "instance"
    tags = {
      Name    = "remotemedia-gpu-worker"
      GPU     = var.gpu_instance_type
      Cluster = aws_ecs_cluster.remotemedia.name
    }
  }
}

#
# Auto Scaling Group for GPU Instances
#
resource "aws_autoscaling_group" "gpu_workers" {
  name                = "remotemedia-gpu-workers"
  vpc_zone_identifier = module.vpc.private_subnets
  min_size            = var.min_gpu_instances
  max_size            = var.max_gpu_instances
  desired_capacity    = var.min_gpu_instances
  
  launch_template {
    id      = aws_launch_template.gpu_instances.id
    version = "$Latest"
  }
  
  # Instance protection
  protect_from_scale_in = true
  
  # Health checks
  health_check_type         = "EC2"
  health_check_grace_period = 300
  
  # Tags
  tag {
    key                 = "AmazonECSManaged"
    value               = true
    propagate_at_launch = true
  }
  
  tag {
    key                 = "Name"
    value               = "remotemedia-gpu-worker"
    propagate_at_launch = true
  }
  
  lifecycle {
    create_before_destroy = true
  }
}

#
# Capacity Provider for GPU Instances
#
resource "aws_ecs_capacity_provider" "gpu" {
  name = "remotemedia-gpu-capacity-provider"
  
  auto_scaling_group_provider {
    auto_scaling_group_arn         = aws_autoscaling_group.gpu_workers.arn
    managed_termination_protection = "ENABLED"
    
    managed_scaling {
      status                    = "ENABLED"
      target_capacity           = 80
      minimum_scaling_step_size = 1
      maximum_scaling_step_size = 4
    }
  }
}

#
# Associate Capacity Provider with Cluster
#
resource "aws_ecs_cluster_capacity_providers" "gpu" {
  cluster_name = aws_ecs_cluster.remotemedia.name
  
  capacity_providers = [
    aws_ecs_capacity_provider.gpu.name,
    "FARGATE",      # For non-GPU tasks
    "FARGATE_SPOT"  # For non-GPU tasks
  ]
  
  default_capacity_provider_strategy {
    capacity_provider = aws_ecs_capacity_provider.gpu.name
    weight           = 100
    base             = 1
  }
}

#
# GPU Task Definition with specific GPU requirements
#
resource "aws_ecs_task_definition" "gpu_grpc_server" {
  family                   = "remotemedia-grpc-server-gpu"
  network_mode             = "awsvpc"
  requires_compatibilities = ["EC2"]
  cpu                      = var.task_cpu
  memory                   = var.task_memory
  execution_role_arn       = aws_iam_role.ecs_task_execution.arn
  task_role_arn            = aws_iam_role.ecs_task.arn
  
  container_definitions = jsonencode([{
    name  = "grpc-server"
    image = "${aws_ecr_repository.remotemedia.repository_url}:latest"
    
    # GPU resource requirements
    resourceRequirements = [
      {
        type  = "GPU"
        value = tostring(var.gpu_count)
      }
    ]
    
    portMappings = [{
      containerPort = 50051
      protocol      = "tcp"
    }]
    
    environment = [
      {
        name  = "RUST_LOG"
        value = var.rust_log_level
      },
      {
        name  = "CUDA_VISIBLE_DEVICES"
        value = "0"  # Use first GPU
      },
      {
        name  = "NVIDIA_VISIBLE_DEVICES"
        value = "all"
      },
      {
        name  = "NVIDIA_DRIVER_CAPABILITIES"
        value = "compute,utility"
      },
      {
        name  = "GPU_TYPE"
        value = var.gpu_instance_type
      }
    ]
    
    logConfiguration = {
      logDriver = "awslogs"
      options = {
        awslogs-group         = aws_cloudwatch_log_group.ecs.name
        awslogs-region        = var.aws_region
        awslogs-stream-prefix = "grpc-server-gpu"
      }
    }
    
    # Use nvidia-docker runtime
    linuxParameters = {
      capabilities = {
        add = ["SYS_ADMIN"]
      }
      devices = [{
        hostPath      = "/dev/nvidia0"
        containerPath = "/dev/nvidia0"
        permissions   = ["read", "write", "mknod"]
      }]
    }
  }])
  
  # GPU placement constraints
  placement_constraints {
    type       = "memberOf"
    expression = "attribute:ecs.instance-type =~ ${var.gpu_instance_type}.*"
  }
}

#
# IAM Instance Profile for GPU Instances
#
resource "aws_iam_instance_profile" "ecs_gpu_instance" {
  name = "remotemedia-ecs-gpu-instance"
  role = aws_iam_role.ecs_gpu_instance.name
}

resource "aws_iam_role" "ecs_gpu_instance" {
  name = "remotemedia-ecs-gpu-instance"
  
  assume_role_policy = jsonencode({
    Version = "2012-10-17"
    Statement = [{
      Action = "sts:AssumeRole"
      Effect = "Allow"
      Principal = {
        Service = "ec2.amazonaws.com"
      }
    }]
  })
}

resource "aws_iam_role_policy_attachment" "ecs_gpu_instance" {
  role       = aws_iam_role.ecs_gpu_instance.name
  policy_arn = "arn:aws:iam::aws:policy/service-role/AmazonEC2ContainerServiceforEC2Role"
}

#
# Data source for ECS-optimized GPU AMI
#
data "aws_ami" "ecs_gpu_optimized" {
  most_recent = true
  owners      = ["amazon"]
  
  filter {
    name   = "name"
    values = ["amzn2-ami-ecs-gpu-hvm-*-x86_64-ebs"]
  }
  
  filter {
    name   = "virtualization-type"
    values = ["hvm"]
  }
}

#
# CloudWatch Alarms for GPU Utilization
#
resource "aws_cloudwatch_metric_alarm" "low_gpu_utilization" {
  alarm_name          = "remotemedia-low-gpu-utilization"
  comparison_operator = "LessThanThreshold"
  evaluation_periods  = "2"
  metric_name         = "GPUUtilization"
  namespace           = "AWS/ECS"
  period              = "300"
  statistic           = "Average"
  threshold           = "20"
  alarm_description   = "Scale down when GPU utilization is low"
  
  dimensions = {
    ClusterName = aws_ecs_cluster.remotemedia.name
  }
  
  alarm_actions = [aws_autoscaling_policy.scale_down_gpu.arn]
}

resource "aws_cloudwatch_metric_alarm" "high_gpu_utilization" {
  alarm_name          = "remotemedia-high-gpu-utilization"
  comparison_operator = "GreaterThanThreshold"
  evaluation_periods  = "2"
  metric_name         = "GPUUtilization"
  namespace           = "AWS/ECS"
  period              = "120"
  statistic           = "Average"
  threshold           = "70"
  alarm_description   = "Scale up when GPU utilization is high"
  
  dimensions = {
    ClusterName = aws_ecs_cluster.remotemedia.name
  }
  
  alarm_actions = [aws_autoscaling_policy.scale_up_gpu.arn]
}

#
# Auto Scaling Policies
#
resource "aws_autoscaling_policy" "scale_up_gpu" {
  name                   = "remotemedia-scale-up-gpu"
  scaling_adjustment     = 1
  adjustment_type        = "ChangeInCapacity"
  cooldown               = 300
  autoscaling_group_name = aws_autoscaling_group.gpu_workers.name
}

resource "aws_autoscaling_policy" "scale_down_gpu" {
  name                   = "remotemedia-scale-down-gpu"
  scaling_adjustment     = -1
  adjustment_type        = "ChangeInCapacity"
  cooldown               = 600
  autoscaling_group_name = aws_autoscaling_group.gpu_workers.name
}

#
# Outputs
#
output "gpu_instance_type" {
  value       = var.gpu_instance_type
  description = "GPU instance type used for ECS tasks"
}

output "gpu_capacity_provider" {
  value       = aws_ecs_capacity_provider.gpu.name
  description = "ECS capacity provider name for GPU instances"
}

output "gpu_task_definition_arn" {
  value       = aws_ecs_task_definition.gpu_grpc_server.arn
  description = "Task definition ARN for GPU-accelerated tasks"
}

