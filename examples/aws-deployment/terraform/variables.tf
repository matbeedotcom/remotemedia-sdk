variable "aws_region" {
  description = "AWS region for deployment"
  type        = string
  default     = "us-east-1"
}

variable "task_cpu" {
  description = "CPU units for ECS task (1024 = 1 vCPU)"
  type        = number
  default     = 4096
}

variable "task_memory" {
  description = "Memory for ECS task in MiB"
  type        = number
  default     = 8192
}

variable "desired_count" {
  description = "Desired number of ECS tasks"
  type        = number
  default     = 3
}

variable "min_capacity" {
  description = "Minimum number of tasks for auto-scaling"
  type        = number
  default     = 2
}

variable "max_capacity" {
  description = "Maximum number of tasks for auto-scaling"
  type        = number
  default     = 10
}

variable "rust_log_level" {
  description = "Rust log level (trace, debug, info, warn, error)"
  type        = string
  default     = "info"
}

