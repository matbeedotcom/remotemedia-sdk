"""
Sandbox Manager for secure code execution.

This module provides sandboxing capabilities for executing untrusted code
in a secure, isolated environment with resource limits.
"""

import logging
import subprocess
import tempfile
import os
import shutil
import sys
from typing import Dict, Any, Optional, List
from dataclasses import dataclass

from config import ServiceConfig


@dataclass
class SandboxResult:
    """Result of sandboxed execution."""
    exit_code: int
    stdout: bytes
    stderr: bytes
    execution_time: float
    memory_peak: int


class SandboxManager:
    """
    Manages sandboxed execution environments for user-defined code.
    
    This class is responsible for creating, managing, and cleaning up
    sandboxed environments using technologies like bubblewrap on Linux.
    """
    
    def __init__(self, config: ServiceConfig):
        """
        Initialize the SandboxManager.
        
        Args:
            config: The service configuration.
        """
        self.config = config
        self.logger = logging.getLogger(__name__)
        
        self._validate_sandbox_config()
        
        if self.config.sandbox_enabled:
            self.logger.info(f"SandboxManager initialized in '{self.config.sandbox_type}' mode.")
        else:
            self.logger.info("SandboxManager is disabled.")
    
    def _validate_sandbox_config(self) -> None:
        """
        Validate the configuration required for the chosen sandbox mode.
        """
        if not self.config.sandbox_enabled:
            self.logger.warning("Sandboxing is disabled by config.")
            return

        if sys.platform != "linux":
            self.logger.warning(
                f"Sandbox type '{self.config.sandbox_type}' is not supported on "
                f"'{sys.platform}'. Disabling sandboxing."
            )
            self.config.sandbox_enabled = False
            return

        if self.config.sandbox_type == "bubblewrap":
            if not shutil.which("bwrap"):
                self.logger.error("bubblewrap (bwrap) not found in PATH, but sandbox mode is enabled.")
                raise RuntimeError("bubblewrap (bwrap) not found in PATH")
        # Add checks for other sandbox types if needed, e.g., firejail, docker
    
    async def execute_in_sandbox(
        self,
        command: List[str],
        working_dir: Optional[str] = None,
        environment: Optional[Dict[str, str]] = None,
        timeout: Optional[int] = None
    ) -> SandboxResult:
        """
        Execute a command in a sandboxed environment.
        
        Args:
            command: Command and arguments to execute
            working_dir: Working directory for execution
            environment: Environment variables
            timeout: Execution timeout in seconds
            
        Returns:
            Sandbox execution result
        """
        if not self.config.sandbox_enabled:
            return await self._execute_unsandboxed(
                command, working_dir, environment, timeout
            )
        
        if self.config.sandbox_type == "bubblewrap":
            return await self._execute_bubblewrap(
                command, working_dir, environment, timeout
            )
        elif self.config.sandbox_type == "firejail":
            return await self._execute_firejail(
                command, working_dir, environment, timeout
            )
        elif self.config.sandbox_type == "docker":
            return await self._execute_docker(
                command, working_dir, environment, timeout
            )
        else:
            return await self._execute_unsandboxed(
                command, working_dir, environment, timeout
            )
    
    async def _execute_unsandboxed(
        self,
        command: List[str],
        working_dir: Optional[str],
        environment: Optional[Dict[str, str]],
        timeout: Optional[int]
    ) -> SandboxResult:
        """Execute command without sandboxing (not secure)."""
        self.logger.warning("Executing without sandbox - not secure!")
        
        import time
        start_time = time.time()
        
        try:
            process = await asyncio.create_subprocess_exec(
                *command,
                cwd=working_dir,
                env=environment,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE
            )
            
            stdout, stderr = await asyncio.wait_for(
                process.communicate(),
                timeout=timeout or self.config.sandbox_timeout
            )
            
            execution_time = time.time() - start_time
            
            return SandboxResult(
                exit_code=process.returncode,
                stdout=stdout,
                stderr=stderr,
                execution_time=execution_time,
                memory_peak=0  # Not tracked in unsandboxed mode
            )
            
        except asyncio.TimeoutError:
            self.logger.error("Command execution timed out")
            return SandboxResult(
                exit_code=-1,
                stdout=b"",
                stderr=b"Execution timed out",
                execution_time=timeout or self.config.sandbox_timeout,
                memory_peak=0
            )
    
    async def _execute_bubblewrap(
        self,
        command: List[str],
        working_dir: Optional[str],
        environment: Optional[Dict[str, str]],
        timeout: Optional[int]
    ) -> SandboxResult:
        """Execute command using bubblewrap sandbox."""
        # Build bubblewrap command
        bwrap_cmd = [
            "bwrap",
            "--ro-bind", "/usr", "/usr",
            "--ro-bind", "/lib", "/lib",
            "--ro-bind", "/lib64", "/lib64",
            "--ro-bind", "/bin", "/bin",
            "--ro-bind", "/sbin", "/sbin",
            "--proc", "/proc",
            "--dev", "/dev",
            "--tmpfs", "/tmp",
            "--unshare-all",
            "--share-net" if self.config.allow_networking else "--unshare-net",
            "--die-with-parent",
        ]
        
        # Add working directory
        if working_dir:
            bwrap_cmd.extend(["--bind", working_dir, working_dir])
            bwrap_cmd.extend(["--chdir", working_dir])
        
        # Add the actual command
        bwrap_cmd.extend(command)
        
        return await self._run_sandboxed_command(
            bwrap_cmd, environment, timeout
        )
    
    async def _execute_firejail(
        self,
        command: List[str],
        working_dir: Optional[str],
        environment: Optional[Dict[str, str]],
        timeout: Optional[int]
    ) -> SandboxResult:
        """Execute command using firejail sandbox."""
        # Build firejail command
        firejail_cmd = [
            "firejail",
            "--quiet",
            "--noprofile",
            "--seccomp",
            "--caps.drop=all",
            "--nonewprivs",
            "--noroot",
        ]
        
        if not self.config.allow_networking:
            firejail_cmd.append("--net=none")
        
        if not self.config.allow_filesystem:
            firejail_cmd.extend(["--private", "--read-only=/"])
        
        # Add working directory
        if working_dir:
            firejail_cmd.extend(["--chroot", working_dir])
        
        # Add the actual command
        firejail_cmd.extend(command)
        
        return await self._run_sandboxed_command(
            firejail_cmd, environment, timeout
        )
    
    async def _execute_docker(
        self,
        command: List[str],
        working_dir: Optional[str],
        environment: Optional[Dict[str, str]],
        timeout: Optional[int]
    ) -> SandboxResult:
        """Execute command using Docker container sandbox."""
        # Build docker command
        docker_cmd = [
            "docker", "run",
            "--rm",
            "--read-only",
            "--tmpfs", "/tmp",
            "--security-opt", "no-new-privileges",
            "--cap-drop", "ALL",
            f"--memory={self.config.max_memory_mb}m",
            f"--cpus={self.config.max_cpu_percent / 100.0}",
        ]
        
        if not self.config.allow_networking:
            docker_cmd.extend(["--network", "none"])
        
        # Add environment variables
        if environment:
            for key, value in environment.items():
                docker_cmd.extend(["-e", f"{key}={value}"])
        
        # Add working directory
        if working_dir:
            docker_cmd.extend(["-w", working_dir])
            docker_cmd.extend(["-v", f"{working_dir}:{working_dir}:ro"])
        
        # Use a minimal Python image
        docker_cmd.append("python:3.11-alpine")
        
        # Add the actual command
        docker_cmd.extend(command)
        
        return await self._run_sandboxed_command(
            docker_cmd, environment, timeout
        )
    
    async def _run_sandboxed_command(
        self,
        command: List[str],
        environment: Optional[Dict[str, str]],
        timeout: Optional[int]
    ) -> SandboxResult:
        """Run a sandboxed command and return results."""
        import time
        import asyncio
        
        start_time = time.time()
        
        try:
            process = await asyncio.create_subprocess_exec(
                *command,
                env=environment,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE
            )
            
            stdout, stderr = await asyncio.wait_for(
                process.communicate(),
                timeout=timeout or self.config.sandbox_timeout
            )
            
            execution_time = time.time() - start_time
            
            return SandboxResult(
                exit_code=process.returncode,
                stdout=stdout,
                stderr=stderr,
                execution_time=execution_time,
                memory_peak=0  # TODO: Implement memory tracking
            )
            
        except asyncio.TimeoutError:
            self.logger.error("Sandboxed command execution timed out")
            return SandboxResult(
                exit_code=-1,
                stdout=b"",
                stderr=b"Execution timed out",
                execution_time=timeout or self.config.sandbox_timeout,
                memory_peak=0
            )
        except Exception as e:
            self.logger.error(f"Error executing sandboxed command: {e}")
            return SandboxResult(
                exit_code=-1,
                stdout=b"",
                stderr=str(e).encode(),
                execution_time=time.time() - start_time,
                memory_peak=0
            )
    
    def create_temp_workspace(self) -> str:
        """
        Create a temporary workspace for code execution.
        
        Returns:
            Path to temporary workspace
        """
        workspace = tempfile.mkdtemp(prefix="remotemedia_sandbox_")
        self.logger.debug(f"Created temporary workspace: {workspace}")
        return workspace
    
    def cleanup_workspace(self, workspace_path: str) -> None:
        """
        Clean up a temporary workspace.
        
        Args:
            workspace_path: Path to workspace to clean up
        """
        try:
            shutil.rmtree(workspace_path)
            self.logger.debug(f"Cleaned up workspace: {workspace_path}")
        except Exception as e:
            self.logger.warning(f"Failed to clean up workspace {workspace_path}: {e}") 