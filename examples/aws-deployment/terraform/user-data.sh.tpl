#!/bin/bash
# ECS GPU Instance User Data Script
# This script configures GPU instances to join the ECS cluster

set -e

# Configure ECS agent
cat <<'EOF' >> /etc/ecs/ecs.config
ECS_CLUSTER=${cluster_name}
ECS_ENABLE_TASK_IAM_ROLE=true
ECS_ENABLE_TASK_IAM_ROLE_NETWORK_HOST=true
ECS_ENABLE_GPU_SUPPORT=${gpu_enabled}
ECS_LOGLEVEL=info
ECS_AVAILABLE_LOGGING_DRIVERS=["json-file","awslogs"]
ECS_ENABLE_CONTAINER_METADATA=true
EOF

# Install NVIDIA Container Runtime (if GPU enabled)
%{ if gpu_enabled }
echo "Installing NVIDIA Container Runtime..."

# Update package manager
yum update -y

# Install NVIDIA drivers (already in AMI, but ensure latest)
yum install -y nvidia-driver-latest-dkms

# Install NVIDIA Container Toolkit
distribution=$(. /etc/os-release;echo $ID$VERSION_ID)
curl -s -L https://nvidia.github.io/libnvidia-container/$distribution/libnvidia-container.repo | \
  tee /etc/yum.repos.d/nvidia-container-toolkit.repo

yum install -y nvidia-container-toolkit
nvidia-ctk runtime configure --runtime=docker
systemctl restart docker

# Verify GPU is accessible
nvidia-smi

# Configure Docker to use nvidia runtime as default
cat > /etc/docker/daemon.json <<DOCKER
{
  "default-runtime": "nvidia",
  "runtimes": {
    "nvidia": {
      "path": "nvidia-container-runtime",
      "runtimeArgs": []
    }
  }
}
DOCKER

systemctl restart docker
systemctl restart ecs

# Test NVIDIA Docker
docker run --rm --gpus all nvidia/cuda:11.8.0-base-ubuntu22.04 nvidia-smi
%{ endif }

# Install CloudWatch agent for GPU metrics
wget https://s3.amazonaws.com/amazoncloudwatch-agent/amazon_linux/amd64/latest/amazon-cloudwatch-agent.rpm
rpm -U ./amazon-cloudwatch-agent.rpm

# Configure CloudWatch agent for GPU monitoring
cat > /opt/aws/amazon-cloudwatch-agent/etc/config.json <<'CWCONFIG'
{
  "agent": {
    "metrics_collection_interval": 60,
    "run_as_user": "root"
  },
  "metrics": {
    "namespace": "RemoteMedia/GPU",
    "metrics_collected": {
      "nvidia_gpu": {
        "measurement": [
          {
            "name": "utilization_gpu",
            "rename": "GPUUtilization",
            "unit": "Percent"
          },
          {
            "name": "utilization_memory",
            "rename": "GPUMemoryUtilization",
            "unit": "Percent"
          },
          {
            "name": "temperature_gpu",
            "rename": "GPUTemperature",
            "unit": "None"
          },
          {
            "name": "power_draw",
            "rename": "GPUPowerDraw",
            "unit": "None"
          }
        ],
        "metrics_collection_interval": 60
      }
    },
    "append_dimensions": {
      "ClusterName": "${cluster_name}",
      "InstanceId": "$${aws:InstanceId}",
      "InstanceType": "$${aws:InstanceType}"
    }
  }
}
CWCONFIG

# Start CloudWatch agent
/opt/aws/amazon-cloudwatch-agent/bin/amazon-cloudwatch-agent-ctl \
  -a fetch-config \
  -m ec2 \
  -s \
  -c file:/opt/aws/amazon-cloudwatch-agent/etc/config.json

# Enable spot instance termination handling
cat > /usr/local/bin/spot-termination-handler.sh <<'SPOT'
#!/bin/bash
while true; do
  # Check for spot instance termination notice
  if curl -s http://169.254.169.254/latest/meta-data/spot/termination-time | grep -q .*T.*Z; then
    echo "Spot instance termination notice detected. Draining tasks..."
    
    # Drain ECS tasks
    INSTANCE_ID=$(curl -s http://169.254.169.254/latest/meta-data/instance-id)
    CONTAINER_INSTANCE=$(aws ecs list-container-instances \
      --cluster ${cluster_name} \
      --filter "ec2InstanceId==$INSTANCE_ID" \
      --query 'containerInstanceArns[0]' \
      --output text)
    
    aws ecs update-container-instances-state \
      --cluster ${cluster_name} \
      --container-instances $CONTAINER_INSTANCE \
      --status DRAINING
    
    # Wait for tasks to finish
    sleep 120
    break
  fi
  sleep 5
done
SPOT

chmod +x /usr/local/bin/spot-termination-handler.sh

# Create systemd service for spot termination handler
cat > /etc/systemd/system/spot-termination-handler.service <<'SERVICE'
[Unit]
Description=Spot Instance Termination Handler
After=network.target

[Service]
Type=simple
ExecStart=/usr/local/bin/spot-termination-handler.sh
Restart=always

[Install]
WantedBy=multi-user.target
SERVICE

systemctl enable spot-termination-handler
systemctl start spot-termination-handler

echo "ECS GPU instance initialization complete"


