terraform {
  required_providers {
    aws = {
      source  = "hashicorp/aws"
      version = "~> 5.0"
    }
  }
}

provider "aws" {
  region = var.aws_region
}

# VPC and Networking
module "vpc" {
  source  = "terraform-aws-modules/vpc/aws"
  version = "~> 5.0"

  name = "remotemedia-vpc"
  cidr = "10.0.0.0/16"

  azs             = ["${var.aws_region}a", "${var.aws_region}b", "${var.aws_region}c"]
  private_subnets = ["10.0.1.0/24", "10.0.2.0/24", "10.0.3.0/24"]
  public_subnets  = ["10.0.101.0/24", "10.0.102.0/24", "10.0.103.0/24"]

  enable_nat_gateway = true
  single_nat_gateway = false
  enable_dns_hostnames = true

  tags = {
    Project = "RemoteMedia"
  }
}

# ECR Repository
resource "aws_ecr_repository" "remotemedia" {
  name                 = "remotemedia-grpc"
  image_tag_mutability = "MUTABLE"

  image_scanning_configuration {
    scan_on_push = true
  }
}

# ECS Cluster
resource "aws_ecs_cluster" "remotemedia" {
  name = "remotemedia-cluster"

  setting {
    name  = "containerInsights"
    value = "enabled"
  }
}

resource "aws_ecs_cluster_capacity_providers" "remotemedia" {
  cluster_name = aws_ecs_cluster.remotemedia.name

  capacity_providers = ["FARGATE", "FARGATE_SPOT"]

  default_capacity_provider_strategy {
    capacity_provider = "FARGATE_SPOT"
    weight           = 70
    base             = 0
  }

  default_capacity_provider_strategy {
    capacity_provider = "FARGATE"
    weight           = 30
    base             = 1
  }
}

# S3 Bucket for Manifests
resource "aws_s3_bucket" "manifests" {
  bucket = "remotemedia-manifests-${data.aws_caller_identity.current.account_id}"
}

resource "aws_s3_bucket_versioning" "manifests" {
  bucket = aws_s3_bucket.manifests.id
  versioning_configuration {
    status = "Enabled"
  }
}

# IAM Roles
resource "aws_iam_role" "ecs_task_execution" {
  name = "remotemedia-ecs-task-execution"

  assume_role_policy = jsonencode({
    Version = "2012-10-17"
    Statement = [{
      Action = "sts:AssumeRole"
      Effect = "Allow"
      Principal = {
        Service = "ecs-tasks.amazonaws.com"
      }
    }]
  })
}

resource "aws_iam_role_policy_attachment" "ecs_task_execution" {
  role       = aws_iam_role.ecs_task_execution.name
  policy_arn = "arn:aws:iam::aws:policy/service-role/AmazonECSTaskExecutionRolePolicy"
}

resource "aws_iam_role" "ecs_task" {
  name = "remotemedia-ecs-task"

  assume_role_policy = jsonencode({
    Version = "2012-10-17"
    Statement = [{
      Action = "sts:AssumeRole"
      Effect = "Allow"
      Principal = {
        Service = "ecs-tasks.amazonaws.com"
      }
    }]
  })
}

resource "aws_iam_role_policy" "ecs_task_s3" {
  name = "s3-access"
  role = aws_iam_role.ecs_task.id

  policy = jsonencode({
    Version = "2012-10-17"
    Statement = [{
      Effect = "Allow"
      Action = [
        "s3:GetObject",
        "s3:ListBucket"
      ]
      Resource = [
        aws_s3_bucket.manifests.arn,
        "${aws_s3_bucket.manifests.arn}/*"
      ]
    }]
  })
}

# Security Group
resource "aws_security_group" "ecs_tasks" {
  name        = "remotemedia-ecs-tasks"
  description = "Allow inbound gRPC traffic"
  vpc_id      = module.vpc.vpc_id

  ingress {
    from_port   = 50051
    to_port     = 50051
    protocol    = "tcp"
    cidr_blocks = ["0.0.0.0/0"]
  }

  egress {
    from_port   = 0
    to_port     = 0
    protocol    = "-1"
    cidr_blocks = ["0.0.0.0/0"]
  }

  tags = {
    Name = "remotemedia-ecs-tasks"
  }
}

# ECS Task Definition
resource "aws_ecs_task_definition" "grpc_server" {
  family                   = "remotemedia-grpc-server"
  network_mode             = "awsvpc"
  requires_compatibilities = ["FARGATE"]
  cpu                      = var.task_cpu
  memory                   = var.task_memory
  execution_role_arn       = aws_iam_role.ecs_task_execution.arn
  task_role_arn            = aws_iam_role.ecs_task.arn

  container_definitions = jsonencode([{
    name  = "grpc-server"
    image = "${aws_ecr_repository.remotemedia.repository_url}:latest"
    
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
        name  = "MANIFEST_BUCKET"
        value = aws_s3_bucket.manifests.bucket
      },
      {
        name  = "AWS_REGION"
        value = var.aws_region
      }
    ]

    logConfiguration = {
      logDriver = "awslogs"
      options = {
        awslogs-group         = aws_cloudwatch_log_group.ecs.name
        awslogs-region        = var.aws_region
        awslogs-stream-prefix = "grpc-server"
      }
    }

    healthCheck = {
      command     = ["CMD-SHELL", "curl -f http://localhost:50051/health || exit 1"]
      interval    = 30
      timeout     = 5
      retries     = 3
      startPeriod = 60
    }
  }])
}

# CloudWatch Log Group
resource "aws_cloudwatch_log_group" "ecs" {
  name              = "/ecs/remotemedia-grpc-server"
  retention_in_days = 7
}

# Network Load Balancer
resource "aws_lb" "grpc" {
  name               = "remotemedia-grpc-nlb"
  internal           = false
  load_balancer_type = "network"
  subnets            = module.vpc.public_subnets

  enable_cross_zone_load_balancing = true

  tags = {
    Name = "remotemedia-grpc-nlb"
  }
}

resource "aws_lb_target_group" "grpc" {
  name        = "remotemedia-grpc-tg"
  port        = 50051
  protocol    = "TCP"
  target_type = "ip"
  vpc_id      = module.vpc.vpc_id

  health_check {
    protocol            = "TCP"
    interval            = 30
    healthy_threshold   = 3
    unhealthy_threshold = 3
  }

  deregistration_delay = 30
}

resource "aws_lb_listener" "grpc" {
  load_balancer_arn = aws_lb.grpc.arn
  port              = "50051"
  protocol          = "TCP"

  default_action {
    type             = "forward"
    target_group_arn = aws_lb_target_group.grpc.arn
  }
}

# ECS Service
resource "aws_ecs_service" "grpc_server" {
  name            = "remotemedia-grpc-server"
  cluster         = aws_ecs_cluster.remotemedia.id
  task_definition = aws_ecs_task_definition.grpc_server.arn
  desired_count   = var.desired_count
  launch_type     = "FARGATE"

  network_configuration {
    subnets          = module.vpc.private_subnets
    security_groups  = [aws_security_group.ecs_tasks.id]
    assign_public_ip = false
  }

  load_balancer {
    target_group_arn = aws_lb_target_group.grpc.arn
    container_name   = "grpc-server"
    container_port   = 50051
  }

  depends_on = [aws_lb_listener.grpc]
}

# Auto Scaling
resource "aws_appautoscaling_target" "ecs" {
  max_capacity       = var.max_capacity
  min_capacity       = var.min_capacity
  resource_id        = "service/${aws_ecs_cluster.remotemedia.name}/${aws_ecs_service.grpc_server.name}"
  scalable_dimension = "ecs:service:DesiredCount"
  service_namespace  = "ecs"
}

resource "aws_appautoscaling_policy" "ecs_cpu" {
  name               = "cpu-scaling"
  policy_type        = "TargetTrackingScaling"
  resource_id        = aws_appautoscaling_target.ecs.resource_id
  scalable_dimension = aws_appautoscaling_target.ecs.scalable_dimension
  service_namespace  = aws_appautoscaling_target.ecs.service_namespace

  target_tracking_scaling_policy_configuration {
    predefined_metric_specification {
      predefined_metric_type = "ECSServiceAverageCPUUtilization"
    }
    target_value = 70.0
  }
}

resource "aws_appautoscaling_policy" "ecs_memory" {
  name               = "memory-scaling"
  policy_type        = "TargetTrackingScaling"
  resource_id        = aws_appautoscaling_target.ecs.resource_id
  scalable_dimension = aws_appautoscaling_target.ecs.scalable_dimension
  service_namespace  = aws_appautoscaling_target.ecs.service_namespace

  target_tracking_scaling_policy_configuration {
    predefined_metric_specification {
      predefined_metric_type = "ECSServiceAverageMemoryUtilization"
    }
    target_value = 80.0
  }
}

# Data sources
data "aws_caller_identity" "current" {}

# Outputs
output "ecr_repository_url" {
  value       = aws_ecr_repository.remotemedia.repository_url
  description = "ECR repository URL for pushing images"
}

output "load_balancer_endpoint" {
  value       = "${aws_lb.grpc.dns_name}:50051"
  description = "gRPC endpoint for RemotePipelineNode configuration"
}

output "manifest_bucket" {
  value       = aws_s3_bucket.manifests.bucket
  description = "S3 bucket for storing pipeline manifests"
}

output "example_node_config" {
  value = jsonencode({
    id        = "remote_gpu_node"
    node_type = "RemotePipelineNode"
    params = {
      transport   = "grpc"
      endpoint    = "${aws_lb.grpc.dns_name}:50051"
      manifest_url = "https://${aws_s3_bucket.manifests.bucket_domain_name}/my-pipeline.json"
      timeout_ms  = 30000
      retry = {
        max_retries = 3
        backoff_ms  = 1000
      }
    }
  })
  description = "Example RemotePipelineNode configuration"
}


