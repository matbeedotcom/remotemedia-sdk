"""
Configuration management for the Remote Execution Service.
"""

import os
from dataclasses import dataclass
from typing import Dict, Any, Optional


@dataclass
class ServiceConfig:
    """Configuration for the Remote Execution Service."""
    
    # Server configuration
    grpc_port: int = 50052
    metrics_port: int = 8080
    max_workers: int = 4
    log_level: str = "INFO"
    
    # Sandbox configuration
    sandbox_enabled: bool = True
    sandbox_type: str = "bubblewrap"  # bubblewrap, firejail, docker
    sandbox_timeout: int = 30
    
    # Resource limits
    max_memory_mb: int = 512
    max_cpu_percent: int = 100
    max_execution_time: int = 30
    max_output_size_mb: int = 100
    
    # Security settings
    allow_networking: bool = False
    allow_filesystem: bool = False
    blocked_modules: list = None
    
    # Service metadata
    version: str = "0.1.0"
    service_name: str = "remotemedia-execution-service"
    
    def __post_init__(self):
        """Initialize configuration from environment variables."""
        # Load from environment variables
        self.grpc_port = int(os.getenv("GRPC_PORT", self.grpc_port))
        self.metrics_port = int(os.getenv("METRICS_PORT", self.metrics_port))
        self.max_workers = int(os.getenv("MAX_WORKERS", self.max_workers))
        self.log_level = os.getenv("LOG_LEVEL", self.log_level)
        
        self.sandbox_enabled = os.getenv("SANDBOX_ENABLED", "true").lower() == "true"
        self.sandbox_type = os.getenv("SANDBOX_TYPE", self.sandbox_type)
        self.sandbox_timeout = int(os.getenv("SANDBOX_TIMEOUT", self.sandbox_timeout))
        
        self.max_memory_mb = int(os.getenv("MAX_MEMORY_MB", self.max_memory_mb))
        self.max_cpu_percent = int(os.getenv("MAX_CPU_PERCENT", self.max_cpu_percent))
        self.max_execution_time = int(os.getenv("MAX_EXECUTION_TIME", self.max_execution_time))
        
        self.allow_networking = os.getenv("ALLOW_NETWORKING", "false").lower() == "true"
        self.allow_filesystem = os.getenv("ALLOW_FILESYSTEM", "false").lower() == "true"
        
        # Default blocked modules for security
        if self.blocked_modules is None:
            self.blocked_modules = [
                "os", "subprocess", "sys", "importlib",
                "socket", "urllib", "requests", "http",
                "ftplib", "smtplib", "telnetlib",
                "__builtin__", "builtins"
            ]
    
    def to_dict(self) -> Dict[str, Any]:
        """Convert configuration to dictionary."""
        return {
            "grpc_port": self.grpc_port,
            "metrics_port": self.metrics_port,
            "max_workers": self.max_workers,
            "log_level": self.log_level,
            "sandbox_enabled": self.sandbox_enabled,
            "sandbox_type": self.sandbox_type,
            "sandbox_timeout": self.sandbox_timeout,
            "max_memory_mb": self.max_memory_mb,
            "max_cpu_percent": self.max_cpu_percent,
            "max_execution_time": self.max_execution_time,
            "max_output_size_mb": self.max_output_size_mb,
            "allow_networking": self.allow_networking,
            "allow_filesystem": self.allow_filesystem,
            "blocked_modules": self.blocked_modules,
            "version": self.version,
            "service_name": self.service_name,
        }
    
    @classmethod
    def from_dict(cls, config_dict: Dict[str, Any]) -> "ServiceConfig":
        """Create configuration from dictionary."""
        return cls(**config_dict)
    
    @classmethod
    def from_file(cls, config_path: str) -> "ServiceConfig":
        """Load configuration from YAML file."""
        import yaml
        
        with open(config_path, 'r') as f:
            config_dict = yaml.safe_load(f)
        
        return cls.from_dict(config_dict)
    
    def validate(self) -> None:
        """Validate configuration values."""
        if self.grpc_port <= 0 or self.grpc_port > 65535:
            raise ValueError(f"Invalid gRPC port: {self.grpc_port}")
        
        if self.max_workers <= 0:
            raise ValueError(f"Invalid max_workers: {self.max_workers}")
        
        if self.sandbox_timeout <= 0:
            raise ValueError(f"Invalid sandbox_timeout: {self.sandbox_timeout}")
        
        if self.max_memory_mb <= 0:
            raise ValueError(f"Invalid max_memory_mb: {self.max_memory_mb}")
        
        if self.sandbox_type not in ["bubblewrap", "firejail", "docker", "none"]:
            raise ValueError(f"Invalid sandbox_type: {self.sandbox_type}")
        
        if self.log_level not in ["DEBUG", "INFO", "WARNING", "ERROR", "CRITICAL"]:
            raise ValueError(f"Invalid log_level: {self.log_level}")


# Global configuration instance
_config: Optional[ServiceConfig] = None


def get_config() -> ServiceConfig:
    """Get the global configuration instance."""
    global _config
    if _config is None:
        _config = ServiceConfig()
        _config.validate()
    return _config


def set_config(config: ServiceConfig) -> None:
    """Set the global configuration instance."""
    global _config
    config.validate()
    _config = config 