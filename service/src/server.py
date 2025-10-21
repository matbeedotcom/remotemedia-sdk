#!/usr/bin/env python3
"""
RemoteMedia Remote Execution Service - Main gRPC Server

This module implements the main gRPC server for the remote execution service.
It handles incoming requests for executing SDK nodes and user-defined code
in a secure, sandboxed environment.
"""

import asyncio
import logging
import os
import signal
import sys
import time
from concurrent import futures
from typing import Dict, Any, AsyncIterable, AsyncGenerator, List, Optional
import inspect
from concurrent.futures import ThreadPoolExecutor
import uuid
import ast
from enum import Enum
import traceback
import psutil
import gc

import grpc
from grpc_health.v1 import health_pb2_grpc
from grpc_health.v1.health_pb2 import HealthCheckResponse

# Import generated gRPC code from remotemedia.protos
from remotemedia.protos import execution_pb2, execution_pb2_grpc, types_pb2
import zipfile
import io
import tempfile
import base64
import subprocess
import json
from typing import Type

# Import service components
from config import ServiceConfig
from executor import TaskExecutor
from sandbox import SandboxManager
from remotemedia.core.node import Node
from remotemedia.serialization import PickleSerializer, JSONSerializer
import cloudpickle
import numpy as np


class ErrorCategory(Enum):
    """Categories of errors for better handling and reporting."""
    UNKNOWN = "unknown"
    MODEL_LOADING = "model_loading"
    MEMORY_ERROR = "memory_error"
    TIMEOUT = "timeout"
    RESOURCE_LIMIT = "resource_limit"
    SERIALIZATION = "serialization"
    NETWORK = "network"
    SECURITY = "security"
    DEPENDENCY = "dependency"
    VALIDATION = "validation"
    CUDA_ERROR = "cuda_error"


class ErrorHandler:
    """Centralized error handling with categorization and recovery strategies."""
    
    def __init__(self, logger: logging.Logger):
        self.logger = logger
        
        # Error patterns for categorization
        self.error_patterns = {
            ErrorCategory.MODEL_LOADING: [
                "Cannot copy out of meta tensor",
                "Failed to load transformers pipeline",
                "torch.nn.Module.to_empty",
                "model loading failed",
                "checkpoint loading",
                "HuggingFace",
                "transformers.models",
                "gated repo",
                "401 Client Error",
                "Access to model",
                "is restricted",
                "must have access",
                "be authenticated"
            ],
            ErrorCategory.MEMORY_ERROR: [
                "out of memory",
                "CUDA out of memory",
                "MemoryError",
                "allocation failed",
                "insufficient memory"
            ],
            ErrorCategory.CUDA_ERROR: [
                "CUDA error",
                "device-side assert",
                "CUDA runtime",
                "GPU memory",
                "device unavailable"
            ],
            ErrorCategory.SERIALIZATION: [
                "pickle",
                "serialization",
                "deserialization",
                "json.JSONDecodeError",
                "encoding error"
            ],
            ErrorCategory.DEPENDENCY: [
                "ImportError",
                "ModuleNotFoundError", 
                "No module named",
                "package not found",
                "dependency"
            ],
            ErrorCategory.NETWORK: [
                "connection",
                "network",
                "timeout",
                "socket",
                "DNS"
            ],
            ErrorCategory.VALIDATION: [
                "ValueError",
                "invalid",
                "validation",
                "parameter",
                "configuration"
            ]
        }
    
    def categorize_error(self, error: Exception) -> ErrorCategory:
        """Categorize an error based on its type and message."""
        error_str = str(error).lower()
        error_type = type(error).__name__.lower()
        
        # Check patterns
        for category, patterns in self.error_patterns.items():
            for pattern in patterns:
                if pattern.lower() in error_str or pattern.lower() in error_type:
                    return category
        
        return ErrorCategory.UNKNOWN
    
    def get_status_for_category(self, category: ErrorCategory) -> int:
        """Map error categories to appropriate gRPC status codes."""
        status_mapping = {
            ErrorCategory.MODEL_LOADING: types_pb2.EXECUTION_STATUS_ERROR,
            ErrorCategory.MEMORY_ERROR: types_pb2.EXECUTION_STATUS_RESOURCE_LIMIT,
            ErrorCategory.TIMEOUT: types_pb2.EXECUTION_STATUS_TIMEOUT,
            ErrorCategory.RESOURCE_LIMIT: types_pb2.EXECUTION_STATUS_RESOURCE_LIMIT,
            ErrorCategory.SECURITY: types_pb2.EXECUTION_STATUS_SECURITY_VIOLATION,
            ErrorCategory.CUDA_ERROR: types_pb2.EXECUTION_STATUS_RESOURCE_LIMIT,
            ErrorCategory.SERIALIZATION: types_pb2.EXECUTION_STATUS_ERROR,
            ErrorCategory.NETWORK: types_pb2.EXECUTION_STATUS_ERROR,
            ErrorCategory.DEPENDENCY: types_pb2.EXECUTION_STATUS_ERROR,
            ErrorCategory.VALIDATION: types_pb2.EXECUTION_STATUS_ERROR,
            ErrorCategory.UNKNOWN: types_pb2.EXECUTION_STATUS_ERROR
        }
        return status_mapping.get(category, types_pb2.EXECUTION_STATUS_ERROR)
    
    def is_retryable(self, category: ErrorCategory) -> bool:
        """Determine if an error category is potentially retryable."""
        retryable_categories = {
            ErrorCategory.NETWORK,
            ErrorCategory.TIMEOUT,
            ErrorCategory.MEMORY_ERROR,  # Sometimes transient
            ErrorCategory.CUDA_ERROR     # Sometimes transient
        }
        return category in retryable_categories
    
    def get_error_context(self, error: Exception, operation: str = "", **kwargs) -> Dict[str, str]:
        """Generate detailed error context for debugging."""
        context = {
            "operation": operation,
            "error_type": type(error).__name__,
            "error_message": str(error),
            "timestamp": time.strftime('%Y-%m-%d %H:%M:%S'),
        }
        
        # Add memory info if available
        try:
            process = psutil.Process()
            context["memory_mb"] = f"{process.memory_info().rss / 1024 / 1024:.1f}"
            context["cpu_percent"] = f"{process.cpu_percent():.1f}"
        except:
            pass
        
        # Add CUDA info if available
        try:
            import torch
            if torch.cuda.is_available():
                context["cuda_memory_allocated"] = f"{torch.cuda.memory_allocated() / 1024**2:.1f}MB"
                context["cuda_memory_cached"] = f"{torch.cuda.memory_reserved() / 1024**2:.1f}MB"
        except:
            pass
        
        # Add custom context
        context.update(kwargs)
        
        return context
    
    def format_error_message(self, error: Exception, category: ErrorCategory, context: Dict[str, str]) -> str:
        """Format a comprehensive error message."""
        base_msg = f"[{category.value.upper()}] {type(error).__name__}: {str(error)}"
        
        if category == ErrorCategory.MODEL_LOADING:
            base_msg += "\n\nThis appears to be a PyTorch model loading issue. Common solutions:"
            base_msg += "\n- Try using torch.nn.Module.to_empty() instead of torch.nn.Module.to()"
            base_msg += "\n- Check if the model was saved with the correct PyTorch version"
            base_msg += "\n- Verify CUDA compatibility if using GPU"
        elif category == ErrorCategory.MEMORY_ERROR:
            base_msg += "\n\nMemory limit exceeded. Consider:"
            base_msg += "\n- Reducing batch size or model size"
            base_msg += "\n- Clearing CUDA cache if using GPU"
            base_msg += "\n- Checking for memory leaks"
        elif category == ErrorCategory.CUDA_ERROR:
            base_msg += "\n\nCUDA error detected. Try:"
            base_msg += "\n- Clearing CUDA cache: torch.cuda.empty_cache()"
            base_msg += "\n- Checking GPU memory availability"
            base_msg += "\n- Verifying CUDA installation"
        
        # Add context if helpful
        if context:
            base_msg += f"\n\nContext: {context}"
        
        return base_msg
    
    async def handle_error(self, error: Exception, operation: str, **context) -> Dict[str, Any]:
        """
        Comprehensive error handling that categorizes, logs, and potentially recovers.
        
        Returns error details for response construction.
        """
        category = self.categorize_error(error)
        error_context = self.get_error_context(error, operation, **context)
        
        # Log with appropriate level based on category
        if category in [ErrorCategory.MEMORY_ERROR, ErrorCategory.CUDA_ERROR]:
            self.logger.warning(f"Resource issue in {operation}: {error}", exc_info=True)
        elif category == ErrorCategory.MODEL_LOADING:
            self.logger.error(f"Model loading failed in {operation}: {error}", exc_info=True)
        else:
            self.logger.error(f"Error in {operation}: {error}", exc_info=True)
        
        # Attempt recovery for certain error types
        if category == ErrorCategory.MEMORY_ERROR or category == ErrorCategory.CUDA_ERROR:
            await self._attempt_memory_recovery()
        
        # Format comprehensive error message
        formatted_message = self.format_error_message(error, category, error_context)
        
        return {
            "status": self.get_status_for_category(category),
            "error_message": formatted_message,
            "error_category": category.value,
            "is_retryable": self.is_retryable(category),
            "context": error_context,
            "traceback": traceback.format_exc()
        }
    
    async def _attempt_memory_recovery(self):
        """Attempt to recover from memory issues."""
        try:
            # Force garbage collection
            gc.collect()
            
            # Clear CUDA cache if available
            try:
                import torch
                if torch.cuda.is_available():
                    torch.cuda.empty_cache()
                    self.logger.info("Cleared CUDA cache for memory recovery")
            except ImportError:
                pass
            
            self.logger.info("Attempted memory recovery")
        except Exception as e:
            self.logger.warning(f"Memory recovery failed: {e}")


class GeneratorSession:
    """Manages a generator's state for streaming."""
    def __init__(self, generator, session_id: str):
        self.generator = generator
        self.session_id = session_id
        self.created_at = time.time()
        self.last_accessed = time.time()
        self.is_exhausted = False
        self.lock = asyncio.Lock()


class RemoteExecutionServicer(execution_pb2_grpc.RemoteExecutionServiceServicer):
    """
    gRPC servicer implementation for remote execution.
    """
    
    def __init__(self, config: ServiceConfig, custom_executor: TaskExecutor = None):
        """
        Initialize the remote execution servicer.
        
        Args:
            config: Service configuration
            custom_executor: Optional custom TaskExecutor instance (if None, creates default)
        """
        self.config = config
        self.executor = custom_executor or TaskExecutor(config)
        self.sandbox_manager = SandboxManager(config)
        self.start_time = time.time()
        self.request_count = 0
        self.success_count = 0
        self.error_count = 0
        self.active_sessions: Dict[str, Any] = {}
        self.object_sessions: Dict[str, Any] = {}
        self.generator_sessions: Dict[str, GeneratorSession] = {}  # Track generator sessions
        self.connection_objects: Dict[str, Dict[str, Any]] = {}  # Track objects per connection
        self._cleanup_lock = asyncio.Lock()
        
        self.logger = logging.getLogger(__name__)
        self.error_handler = ErrorHandler(self.logger)  # Initialize error handler
        self.logger.info("RemoteExecutionServicer initialized")
        
        # Start periodic cleanup task
        asyncio.create_task(self._periodic_cleanup())
    
    def _get_serializer(self, serialization_format: str):
        """Get the appropriate serializer based on format."""
        if serialization_format == 'pickle':
            return PickleSerializer()
        elif serialization_format == 'json':
            return JSONSerializer()
        else:
            raise ValueError(f"Unsupported serialization format: {serialization_format}")
    
    def _get_peer_info(self, context: grpc.aio.ServicerContext) -> str:
        """Get a unique identifier for the peer connection."""
        peer = context.peer() if hasattr(context, 'peer') else 'unknown'
        return str(peer)
    
    async def _cleanup_connection_resources(self, connection_id: str) -> None:
        """Clean up all resources associated with a connection."""
        async with self._cleanup_lock:
            if connection_id in self.connection_objects:
                self.logger.info(f"Cleaning up resources for connection: {connection_id}")
                connection_data = self.connection_objects[connection_id]
                
                # Clean up all objects associated with this connection
                for session_id, session_data in list(connection_data.get('sessions', {}).items()):
                    await self._cleanup_session(session_id, session_data)
                
                # Remove connection tracking
                del self.connection_objects[connection_id]
                self.logger.info(f"Completed cleanup for connection: {connection_id}")
    
    async def _cleanup_session(self, session_id: str, session_data: Dict[str, Any]) -> None:
        """Clean up a specific session and its resources."""
        try:
            obj = session_data.get('object')
            if obj:
                # Call cleanup on the object if it has the method
                if hasattr(obj, 'cleanup') and callable(getattr(obj, 'cleanup')):
                    self.logger.info(f"Calling cleanup on object {type(obj).__name__} for session {session_id}")
                    if asyncio.iscoroutinefunction(obj.cleanup):
                        await obj.cleanup()
                    else:
                        obj.cleanup()
                
                # For ML models, explicitly free VRAM
                if hasattr(obj, 'llm_pipeline'):
                    obj.llm_pipeline = None
                if hasattr(obj, '_serve_engine'):
                    obj._serve_engine = None
                if hasattr(obj, 'model'):
                    obj.model = None
                
                # Force garbage collection to free VRAM
                import gc
                gc.collect()
                
                # If torch is available, clear cache
                try:
                    import torch
                    if torch.cuda.is_available():
                        torch.cuda.empty_cache()
                        self.logger.info(f"Cleared CUDA cache after cleaning up session {session_id}")
                except ImportError:
                    pass
            
            # Clean up sandbox if exists
            sandbox_path = session_data.get('sandbox_path')
            if sandbox_path:
                code_root = os.path.join(sandbox_path, "code")
                if code_root in sys.path:
                    sys.path.remove(code_root)
                try:
                    import shutil
                    shutil.rmtree(sandbox_path)
                    self.logger.info(f"Removed sandbox directory: {sandbox_path}")
                except Exception as e:
                    self.logger.error(f"Failed to cleanup sandbox {sandbox_path}: {e}")
            
            # Remove from object_sessions if present
            if session_id in self.object_sessions:
                del self.object_sessions[session_id]
                
        except Exception as e:
            self.logger.error(f"Error during session cleanup for {session_id}: {e}", exc_info=True)
    
    async def _install_pip_packages(self, packages: List[str], sandbox_path: str) -> List[str]:
        """
        Install pip packages in the sandbox environment.
        
        Args:
            packages: List of package names to install
            sandbox_path: Path to the sandbox directory
            
        Returns:
            List of successfully installed packages
        """
        if not packages:
            return []
        
        installed = []
        venv_path = os.path.join(sandbox_path, "venv")
        
        try:
            # Create virtual environment
            self.logger.info(f"Creating virtual environment in {venv_path}")
            proc = await asyncio.create_subprocess_exec(
                sys.executable, "-m", "venv", venv_path,
                stdout=asyncio.subprocess.PIPE,
                stderr=asyncio.subprocess.PIPE
            )
            stdout, stderr = await proc.communicate()
            
            if proc.returncode != 0:
                self.logger.error(f"Failed to create venv: {stderr.decode()}")
                return []
            
            # Wait a moment for venv creation to complete
            await asyncio.sleep(0.1)
            
            # Get pip path in venv
            pip_path = os.path.join(venv_path, "bin", "pip") if os.name != "nt" else os.path.join(venv_path, "Scripts", "pip.exe")
            
            # Verify pip exists
            if not os.path.exists(pip_path):
                self.logger.error(f"Pip not found at {pip_path}")
                # Try python -m pip instead
                python_path = os.path.join(venv_path, "bin", "python") if os.name != "nt" else os.path.join(venv_path, "Scripts", "python.exe")
                
                # Install packages using python -m pip
                for package in packages:
                    try:
                        self.logger.info(f"Installing package: {package}")
                        proc = await asyncio.create_subprocess_exec(
                            python_path, "-m", "pip", "install", package,
                            stdout=asyncio.subprocess.PIPE,
                            stderr=asyncio.subprocess.PIPE
                        )
                        stdout, stderr = await proc.communicate()
                        
                        if proc.returncode == 0:
                            installed.append(package)
                            self.logger.info(f"Successfully installed: {package}")
                        else:
                            self.logger.error(f"Failed to install {package}: {stderr.decode()}")
                    except Exception as e:
                        self.logger.error(f"Error installing {package}: {e}")
            else:
                # Install packages one by one
                for package in packages:
                    try:
                        self.logger.info(f"Installing package: {package}")
                        proc = await asyncio.create_subprocess_exec(
                            pip_path, "install", package,
                            stdout=asyncio.subprocess.PIPE,
                            stderr=asyncio.subprocess.PIPE
                        )
                        stdout, stderr = await proc.communicate()
                        
                        if proc.returncode == 0:
                            installed.append(package)
                            self.logger.info(f"Successfully installed: {package}")
                        else:
                            self.logger.error(f"Failed to install {package}: {stderr.decode()}")
                    except Exception as e:
                        self.logger.error(f"Error installing {package}: {e}")
            
            # Add venv site-packages to Python path
            if installed:
                site_packages = os.path.join(venv_path, "lib", f"python{sys.version_info.major}.{sys.version_info.minor}", "site-packages")
                if os.path.exists(site_packages):
                    sys.path.insert(0, site_packages)
                    self.logger.info(f"Added {site_packages} to Python path")
                else:
                    # Try alternative site-packages location
                    site_packages = os.path.join(venv_path, "Lib", "site-packages")
                    if os.path.exists(site_packages):
                        sys.path.insert(0, site_packages)
                        self.logger.info(f"Added {site_packages} to Python path")
                
        except Exception as e:
            self.logger.error(f"Error setting up virtual environment: {e}")
        
        return installed
    
    async def _periodic_cleanup(self) -> None:
        """Periodically clean up orphaned sessions and free resources."""
        while True:
            try:
                await asyncio.sleep(300)  # Run every 5 minutes
                
                async with self._cleanup_lock:
                    # Clean up orphaned sessions (sessions without connections)
                    orphaned_sessions = []
                    tracked_sessions = set()
                    
                    # Collect all tracked sessions
                    for conn_data in self.connection_objects.values():
                        tracked_sessions.update(conn_data.get('sessions', {}).keys())
                    
                    # Find orphaned sessions
                    for session_id in list(self.object_sessions.keys()):
                        if session_id not in tracked_sessions:
                            orphaned_sessions.append(session_id)
                    
                    # Clean up orphaned sessions
                    if orphaned_sessions:
                        self.logger.info(f"Found {len(orphaned_sessions)} orphaned sessions, cleaning up...")
                        for session_id in orphaned_sessions:
                            session_data = self.object_sessions.get(session_id, {})
                            await self._cleanup_session(session_id, session_data)
                    
                    # Clean up old generator sessions (older than 10 minutes)
                    old_generators = []
                    current_time = time.time()
                    for gen_id, gen_session in self.generator_sessions.items():
                        if current_time - gen_session.last_accessed > 600:  # 10 minutes
                            old_generators.append(gen_id)
                    
                    if old_generators:
                        self.logger.info(f"Cleaning up {len(old_generators)} old generator sessions")
                        for gen_id in old_generators:
                            session = self.generator_sessions[gen_id]
                            # Close generator if possible
                            if hasattr(session.generator, 'aclose'):
                                try:
                                    await session.generator.aclose()
                                except:
                                    pass
                            elif hasattr(session.generator, 'close'):
                                try:
                                    session.generator.close()
                                except:
                                    pass
                            del self.generator_sessions[gen_id]
                    
                    # Force garbage collection and clear CUDA cache
                    import gc
                    gc.collect()
                    
                    try:
                        import torch
                        if torch.cuda.is_available():
                            torch.cuda.empty_cache()
                            self.logger.info("Periodic CUDA cache cleanup completed")
                    except ImportError:
                        pass
                        
            except Exception as e:
                self.logger.error(f"Error during periodic cleanup: {e}", exc_info=True)
    
    async def ExecuteNode(
        self, 
        request: execution_pb2.ExecuteNodeRequest, 
        context: grpc.aio.ServicerContext
    ) -> execution_pb2.ExecuteNodeResponse:
        """
        Execute a predefined SDK node.
        
        Args:
            request: Node execution request
            context: gRPC context
            
        Returns:
            Node execution response
        """
        self.request_count += 1
        start_time = time.time()
        operation = f"ExecuteNode[{request.node_type}]"
        
        self.logger.info(f"Executing SDK node: {request.node_type}")
        
        try:
            # Execute the node using the task executor
            result = await self.executor.execute_sdk_node(
                node_type=request.node_type,
                config=dict(request.config),
                input_data=request.input_data,
                serialization_format=request.serialization_format,
                options=request.options
            )
            
            self.success_count += 1
            
            # Build response
            response = execution_pb2.ExecuteNodeResponse(
                status=types_pb2.EXECUTION_STATUS_SUCCESS,
                output_data=result.output_data,
                metrics=self._build_metrics(start_time, result)
            )
            
            self.logger.info(f"Successfully executed node: {request.node_type}")
            return response
            
        except Exception as e:
            self.error_count += 1
            
            # Use enhanced error handling
            error_details = await self.error_handler.handle_error(
                e, operation,
                node_type=request.node_type,
                config=dict(request.config),
                serialization_format=request.serialization_format,
                input_size=len(request.input_data)
            )
            
            return execution_pb2.ExecuteNodeResponse(
                status=error_details["status"],
                error_message=error_details["error_message"],
                error_traceback=error_details["traceback"],
                metrics=self._build_error_metrics(start_time)
            )
    
    async def ExecuteCustomTask(
        self,
        request: execution_pb2.ExecuteCustomTaskRequest,
        context: grpc.aio.ServicerContext
    ) -> execution_pb2.ExecuteCustomTaskResponse:
        """
        Execute user-defined code (Phase 3 feature).
        
        Args:
            request: Custom task execution request
            context: gRPC context
            
        Returns:
            Custom task execution response
        """
        self.request_count += 1
        start_time = time.time()
        operation = f"ExecuteCustomTask[{request.entry_point}]"
        
        self.logger.info("Executing custom task")
        
        try:
            # This will be implemented in Phase 3
            result = await self.executor.execute_custom_task(
                code_package=request.code_package,
                entry_point=request.entry_point,
                input_data=request.input_data,
                serialization_format=request.serialization_format,
                dependencies=list(request.dependencies),
                options=request.options
            )
            
            self.success_count += 1
            
            response = execution_pb2.ExecuteCustomTaskResponse(
                status=types_pb2.EXECUTION_STATUS_SUCCESS,
                output_data=result.output_data,
                metrics=self._build_metrics(start_time, result),
                installed_deps=result.installed_dependencies
            )
            
            self.logger.info("Successfully executed custom task")
            return response
            
        except NotImplementedError:
            self.error_count += 1
            return execution_pb2.ExecuteCustomTaskResponse(
                status=types_pb2.EXECUTION_STATUS_ERROR,
                error_message="Custom task execution not yet implemented (Phase 3)",
                metrics=self._build_error_metrics(start_time)
            )
        except Exception as e:
            self.error_count += 1
            
            # Use enhanced error handling
            error_details = await self.error_handler.handle_error(
                e, operation,
                entry_point=request.entry_point,
                dependencies=list(request.dependencies),
                serialization_format=request.serialization_format,
                input_size=len(request.input_data)
            )
            
            return execution_pb2.ExecuteCustomTaskResponse(
                status=error_details["status"],
                error_message=error_details["error_message"],
                error_traceback=error_details["traceback"],
                metrics=self._build_error_metrics(start_time)
            )
    
    async def ExecuteObjectMethod(
        self,
        request: execution_pb2.ExecuteObjectMethodRequest,
        context: grpc.aio.ServicerContext
    ) -> execution_pb2.ExecuteObjectMethodResponse:
        """Execute a method on a serialized object, using session management."""
        self.logger.info("Executing ExecuteObjectMethod")
        
        session_id = request.session_id
        obj = None
        sandbox_path = None
        connection_id = self._get_peer_info(context)
        
        # Track this connection
        if connection_id not in self.connection_objects:
            self.connection_objects[connection_id] = {'sessions': {}}
        
        try:
            if session_id:
                if session_id in self.object_sessions:
                    # Use existing object from session
                    obj = self.object_sessions[session_id]['object']
                    self.logger.info(f"Using existing object from session {session_id}")
                else:
                    raise ValueError("Session not found")
            else:
                # Create new object and session
                session_id = str(uuid.uuid4())
                self.logger.info(f"Creating new session {session_id}")
                
                # Setup sandbox and load object
                sandbox_path = tempfile.mkdtemp(prefix="remotemedia_")
                with io.BytesIO(request.code_package) as bio:
                    with zipfile.ZipFile(bio, 'r') as zf:
                        zf.extractall(sandbox_path)
                
                code_path = os.path.join(sandbox_path, "code")
                sys.path.insert(0, code_path)
                
                # Debug: List what was extracted
                self.logger.info(f"Sandbox path: {sandbox_path}")
                self.logger.info(f"Code path added to sys.path: {code_path}")
                
                # Recursively list all files in sandbox
                self.logger.info("All files in sandbox:")
                for root, dirs, files in os.walk(sandbox_path):
                    level = root.replace(sandbox_path, '').count(os.sep)
                    indent = ' ' * 2 * level
                    self.logger.info(f"{indent}{os.path.basename(root)}/")
                    subindent = ' ' * 2 * (level + 1)
                    for file in files:
                        self.logger.info(f"{subindent}{file}")
                
                # Install dependencies if provided
                if request.dependencies:
                    self.logger.info(f"Installing dependencies: {request.dependencies}")
                    await self._install_pip_packages(list(request.dependencies), sandbox_path)
                
                object_pkl_path = os.path.join(sandbox_path, "serialized_object.pkl")
                with open(object_pkl_path, 'r') as f:
                    encoded_obj = f.read()
                
                # Before unpickling, ensure all Python files in code directory are importable
                # This handles cases where modules are imported directly
                if os.path.exists(code_path):
                    # Walk through all subdirectories to find Python files
                    for root, dirs, files in os.walk(code_path):
                        for file in files:
                            if file.endswith('.py') and file != '__init__.py':
                                module_name = file[:-3]  # Remove .py extension
                                module_path = os.path.join(root, file)
                                
                                # Check if it's in a subdirectory and adjust module name
                                rel_path = os.path.relpath(module_path, code_path)
                                if os.path.dirname(rel_path):
                                    # It's in a subdirectory, use the full path as module name
                                    module_parts = rel_path.replace(os.sep, '.')[:-3]  # Remove .py
                                    full_module_name = module_parts
                                else:
                                    full_module_name = module_name
                                
                                # Add the module to sys.modules if it's not already there
                                if module_name not in sys.modules:
                                    import importlib.util
                                    spec = importlib.util.spec_from_file_location(module_name, module_path)
                                    if spec and spec.loader:
                                        module = importlib.util.module_from_spec(spec)
                                        sys.modules[module_name] = module
                                        try:
                                            spec.loader.exec_module(module)
                                            self.logger.info(f"Pre-loaded module '{module_name}' from {module_path}")
                                        except Exception as e:
                                            self.logger.warning(f"Failed to pre-load module '{module_name}': {e}")
                
                obj = cloudpickle.loads(base64.b64decode(encoded_obj))
                
                self.object_sessions[session_id] = {
                    "object": obj,
                    "sandbox_path": sandbox_path
                }
                
                # Track this session under the connection
                self.connection_objects[connection_id]['sessions'][session_id] = {
                    "object": obj,
                    "sandbox_path": sandbox_path
                }

                # Initialize object if it has an initialize method
                if hasattr(obj, 'initialize') and callable(getattr(obj, 'initialize')):
                    try:
                        await obj.initialize()
                    except Exception as init_error:
                        # Log initialization error but don't crash
                        self.logger.error(f"Failed to initialize object: {init_error}")
                        # Re-raise to be caught by outer exception handler
                        raise RuntimeError(f"Failed to initialize {obj.__class__.__name__}: {str(init_error)}") from init_error

            # Deserialize arguments
            serializer = PickleSerializer() if request.serialization_format == 'pickle' else JSONSerializer()
            method_args = serializer.deserialize(request.method_args_data)
            
            # Deserialize keyword arguments if provided
            method_kwargs = {}
            if request.method_kwargs_data:
                method_kwargs = serializer.deserialize(request.method_kwargs_data)

            # Handle special proxy initialization
            if request.method_name == "__init__":
                # No-op for proxy initialization
                result = None
            else:
                # Get attribute/method
                attr = getattr(obj, request.method_name)
                
                # Check if it's callable (method) or not (property/attribute)
                if callable(attr):
                    # It's a method - call it
                    if asyncio.iscoroutinefunction(attr):
                        result = await attr(*method_args, **method_kwargs)
                    else:
                        result = attr(*method_args, **method_kwargs)
                    
                    # Handle generators by creating a session instead of materializing
                    if inspect.isgenerator(result) or inspect.isasyncgen(result):
                        # Create generator session
                        generator_id = str(uuid.uuid4())
                        self.generator_sessions[generator_id] = GeneratorSession(
                            generator=result,
                            session_id=generator_id
                        )
                        # Return special marker
                        result = {"__generator__": True, "generator_id": generator_id, 
                                 "is_async": inspect.isasyncgen(result)}
                else:
                    # It's a property or attribute - just return its value
                    result = attr

            result_data = serializer.serialize(result)

            return execution_pb2.ExecuteObjectMethodResponse(
                status=types_pb2.EXECUTION_STATUS_SUCCESS,
                result_data=result_data,
                session_id=session_id
            )
        except Exception as e:
            # Use enhanced error handling
            operation = f"ExecuteObjectMethod[{request.method_name}]"
            error_details = await self.error_handler.handle_error(
                e, operation,
                session_id=session_id,
                method_name=request.method_name,
                serialization_format=request.serialization_format,
                has_session=bool(session_id),
                connection_id=connection_id
            )
            
            return execution_pb2.ExecuteObjectMethodResponse(
                status=error_details["status"],
                error_message=error_details["error_message"],
                error_traceback=error_details["traceback"]
            )
        # Note: We are not cleaning up the session here. A separate mechanism
        # for session timeout/cleanup would be needed in a production system.
    
    async def StreamObject(
        self,
        request_iterator: AsyncIterable[execution_pb2.StreamObjectRequest],
        context: grpc.aio.ServicerContext
    ) -> AsyncGenerator[execution_pb2.StreamObjectResponse, None]:
        """
        Handle bidirectional streaming for a serialized object.
        """
        logger = logging.getLogger(__name__)
        logger.info("New StreamObject connection opened.")
        obj = None
        sandbox_path = None
        session_id = None
        connection_id = self._get_peer_info(context)
        
        # Track this connection
        if connection_id not in self.connection_objects:
            self.connection_objects[connection_id] = {'sessions': {}}
        
        try:
            # The first message from the client MUST be the initialization message.
            logger.debug("Waiting for initialization message...")
            init_request = None
            async for request in request_iterator:
                if request.HasField("init"):
                    init_request = request
                    break
                else:
                    logger.warning("Skipping non-init message while waiting for initialization")
            
            if not init_request:
                logger.error("No initialization message received")
                yield execution_pb2.StreamObjectResponse(error="Stream must be initialized with a StreamObjectInit message.")
                return

            init_request_data = init_request.init
            logger.debug(f"Received init request with session_id: {init_request_data.session_id}")
            
            # If a session ID is provided, use the existing object
            session_id = init_request_data.session_id
            if session_id and session_id in self.object_sessions:
                logger.info(f"StreamObject: Using existing object from session {session_id}")
                obj = self.object_sessions[session_id]['object']
            elif session_id:
                logger.error(f"StreamObject error: Session ID {session_id} not found.")
                yield execution_pb2.StreamObjectResponse(error=f"Session ID {session_id} not found.")
                return
            else:
                # No session ID, create a temporary object for this stream
                logger.info("StreamObject: No session ID, creating temporary object.")
                try:
                    # Create a temporary directory to act as a sandbox
                    sandbox_path = tempfile.mkdtemp(prefix="remotemedia_")
                    logger.debug(f"Created sandbox at {sandbox_path}")
                    
                    # Extract the code package
                    with io.BytesIO(init_request_data.code_package) as bio:
                        with zipfile.ZipFile(bio, 'r') as zf:
                            zf.extractall(sandbox_path)
                            logger.debug("Code package extracted successfully")
                    
                    # Add the code path to sys.path
                    code_root = os.path.join(sandbox_path, "code")
                    sys.path.insert(0, code_root)
                    logger.debug(f"Added {code_root} to sys.path")
                    
                    # Install dependencies if provided
                    if init_request_data.dependencies:
                        logger.info(f"Installing dependencies: {init_request_data.dependencies}")
                        await self._install_pip_packages(list(init_request_data.dependencies), sandbox_path)
                    
                    # Load the serialized object
                    object_pkl_path = os.path.join(sandbox_path, "serialized_object.pkl")
                    logger.debug(f"Loading object from {object_pkl_path}")
                    with open(object_pkl_path, 'r') as f:
                        encoded_obj = f.read()
                    
                    decoded_obj = base64.b64decode(encoded_obj)
                    obj = cloudpickle.loads(decoded_obj)
                    logger.info(f"Successfully loaded object of type {type(obj).__name__}")

                except Exception as e:
                    logger.error(f"Failed to deserialize object: {e}", exc_info=True)
                    yield execution_pb2.StreamObjectResponse(error=f"Failed to deserialize object: {e}")
                    return

            # Check for required methods
            if not hasattr(obj, 'process'):
                 logger.error("StreamObject error: object is missing process method.")
                 yield execution_pb2.StreamObjectResponse(error="Serialized object must have a process method.")
                 return

            logger.info(f"StreamObject: Successfully got object of type {type(obj).__name__}.")

            # Initialization is now handled by the client's initialize() call.
            serialization_format = init_request_data.serialization_format
            if serialization_format == 'pickle':
                serializer = PickleSerializer()
            elif serialization_format == 'json':
                serializer = JSONSerializer()
            else:
                 logger.error(f"StreamObject error: unsupported serialization format '{serialization_format}'.")
                 yield execution_pb2.StreamObjectResponse(error=f"Unsupported serialization format: {serialization_format}")
                 return

            async def input_stream_generator():
                logger.info("StreamObject: Starting input stream processing.")
                chunk_count = 0
                try:
                    async for req in request_iterator:
                        if req.HasField("data"):
                            chunk_count += 1
                            logger.debug(f"Server: Received chunk {chunk_count} for remote object.")
                            yield serializer.deserialize(req.data)
                except Exception as e:
                    logger.error(f"Error in input stream generator: {e}", exc_info=True)
                    raise
                logger.info(f"StreamObject: Input stream finished after {chunk_count} chunks.")

            # Pass the async generator directly to the process method
            logger.info("StreamObject: Calling process() on remote object.")
            try:
                async for result in obj.process(input_stream_generator()):
                    logger.debug("StreamObject: Sending result chunk to client.")
                    serialized_result = serializer.serialize(result)
                    yield execution_pb2.StreamObjectResponse(data=serialized_result)
                logger.info("StreamObject: process() method finished.")
                
                # After the stream is done, flush the object if possible
                if hasattr(obj, 'flush') and callable(getattr(obj, 'flush')):
                    logger.info("StreamObject: Calling flush() on remote object.")
                    if inspect.iscoroutinefunction(obj.flush):
                        flushed_result = await obj.flush()
                    else:
                        flushed_result = obj.flush()
                    if flushed_result is not None:
                        logger.info("StreamObject: Sending flushed result to client.")
                        serialized_result = serializer.serialize(flushed_result)
                        yield execution_pb2.StreamObjectResponse(data=serialized_result)
                
            except Exception as e:
                logger.error(f"Error during object processing: {e}", exc_info=True)
                yield execution_pb2.StreamObjectResponse(error=f"Error during processing: {e}")

        except StopAsyncIteration:
            logger.error("StreamObject error: Client disconnected before sending initialization message.")
            yield execution_pb2.StreamObjectResponse(error="Client disconnected before initialization.")
        except Exception as e:
            # Use enhanced error handling for better diagnostics
            operation = f"StreamObject[{type(obj).__name__ if obj else 'Unknown'}]"
            error_details = await self.error_handler.handle_error(
                e, operation,
                object_type=type(obj).__name__ if obj else 'Unknown',
                session_id=session_id,
                connection_id=connection_id,
                has_init_request=init_request is not None
            )
            
            yield execution_pb2.StreamObjectResponse(
                error=f"[{error_details['error_category'].upper()}] {error_details['error_message']}"
            )
        finally:
            # Clean up all resources for this connection
            await self._cleanup_connection_resources(connection_id)
            
            # Additional cleanup for temporary objects (no session_id)
            if not session_id and obj and hasattr(obj, 'cleanup'):
                await obj.cleanup()
            
            # Clean up sandbox if it was created and not tracked in sessions
            if sandbox_path and not session_id:
                code_root = os.path.join(sandbox_path, "code")
                if code_root in sys.path:
                    sys.path.remove(code_root)
                try:
                    import shutil
                    shutil.rmtree(sandbox_path)
                except Exception as e:
                    logger.error(f"Failed to cleanup sandbox {sandbox_path}: {e}")

            logger.info(f"StreamObject connection closed for {connection_id}")
    
    async def _get_node_definitions_json(self) -> str:
        """
        Get raw node definitions as JSON for TypeScript generation.
        
        Returns:
            JSON string containing all node definitions
        """
        import json
        from datetime import datetime
        
        # Get all available nodes dynamically
        available_nodes = await self.executor.get_available_nodes()
        
        # Structure the data for TypeScript generation
        node_definitions = {
            "generated_at": datetime.now().isoformat(),
            "service_version": self.config.version,
            "nodes": available_nodes
        }
        
        return json.dumps(node_definitions, indent=2, default=str)
    
    async def ExportTypeScriptDefinitions(
        self,
        request: execution_pb2.ExportTypeScriptRequest,
        context: grpc.aio.ServicerContext
    ) -> execution_pb2.ExportTypeScriptResponse:
        """
        Export TypeScript interface definitions.
        
        Args:
            request: Export request
            context: gRPC context
            
        Returns:
            TypeScript definitions response
        """
        try:
            # Return raw node definitions as JSON for Node.js processing
            node_definitions_json = await self._get_node_definitions_json()
            
            return execution_pb2.ExportTypeScriptResponse(
                status=types_pb2.EXECUTION_STATUS_SUCCESS,
                typescript_definitions=node_definitions_json,
                version=self.config.version
            )
            
        except Exception as e:
            self.logger.error(f"Error exporting TypeScript definitions: {e}", exc_info=True)
            return execution_pb2.ExportTypeScriptResponse(
                status=types_pb2.EXECUTION_STATUS_ERROR,
                error_message=str(e)
            )
    
    def _python_to_typescript_type(self, python_type: str) -> str:
        """
        Convert Python type hints to TypeScript types.
        
        Args:
            python_type: Python type as string
            
        Returns:
            Corresponding TypeScript type
        """
        type_mapping = {
            "str": "string",
            "int": "number",
            "float": "number",
            "bool": "boolean",
            "None": "null",
            "Any": "any",
            "Dict": "Record<string, any>",
            "List": "any[]",
            "Tuple": "any[]",
            "Optional": "| undefined",
            "Union": "|",
            "bytes": "Uint8Array",
            "numpy.ndarray": "number[] | Float32Array",
        }
        
        for py_type, ts_type in type_mapping.items():
            if py_type in python_type:
                return ts_type
        
        # Default to any for unknown types
        return "any"
    
    async def StreamNode(
        self,
        request_iterator: AsyncIterable[execution_pb2.StreamData],
        context: grpc.aio.ServicerContext
    ) -> AsyncGenerator[execution_pb2.StreamData, None]:
        """
        Handle bidirectional streaming for a single node.
        
        The first message from the client must contain the `init` payload
        to configure the node for the stream.
        """
        self.logger.info("New StreamNode connection opened.")
        node = None
        loop = asyncio.get_running_loop()
        executor = ThreadPoolExecutor()
        connection_id = self._get_peer_info(context)
        session_id = str(uuid.uuid4())
        
        # Track this connection
        if connection_id not in self.connection_objects:
            self.connection_objects[connection_id] = {'sessions': {}}
        
        try:
            # The first message is the initialization message
            init_request_data = await request_iterator.__anext__()
            if not init_request_data.HasField("init"):
                context.set_code(grpc.StatusCode.INVALID_ARGUMENT)
                context.set_details("Stream must be initialized with a StreamInit message.")
                return

            init_request = init_request_data.init
            node_type = init_request.node_type
            
            # Convert config values back to their likely types
            config = {}
            for k, v in init_request.config.items():
                try:
                    # Safely evaluate literals like lists, dicts, booleans, numbers
                    config[k] = ast.literal_eval(v)
                except (ValueError, SyntaxError):
                    # Keep it as a string if it's not a literal
                    config[k] = v

            serialization_format = init_request.serialization_format

            self.logger.info(f"Stream initialized for node type '{node_type}' with config {config}")

            # Dynamically create the node instance from the client library
            # This is a simplification; a real service would have a more robust
            # and secure way of mapping node_type to a class.
            from remotemedia.nodes import __all__ as all_nodes
            from remotemedia import nodes

            if node_type not in all_nodes:
                raise ValueError(f"Node type '{node_type}' is not supported for remote execution.")
            
            NodeClass = getattr(nodes, node_type)
            node = NodeClass(**config)
            await node.initialize()
            
            # Track this node under the connection
            self.connection_objects[connection_id]['sessions'][session_id] = {
                "object": node,
                "node_type": node_type
            }

            # Get the correct serializer
            if serialization_format == 'pickle':
                serializer = PickleSerializer()
            elif serialization_format == 'json':
                serializer = JSONSerializer()
            else:
                raise ValueError(f"Unsupported serialization format: {serialization_format}")

            async def input_stream_generator():
                """Reads from the client stream and yields deserialized data."""
                async for req in request_iterator:
                    if req.HasField("data"):
                        yield serializer.deserialize(req.data)
                    else:
                        self.logger.warning("Received non-data message in stream, ignoring.")

            # Check if the node's process method is a streaming one
            if inspect.isasyncgenfunction(node.process):
                # It's a streaming node, so we pass the generator
                async for result in node.process(input_stream_generator()):
                    serialized_result = serializer.serialize(result)
                    yield execution_pb2.StreamData(data=serialized_result)
            else:
                # It's a standard node, process item by item
                async for item in input_stream_generator():
                    if inspect.iscoroutinefunction(node.process):
                        result = await node.process(item)
                    else:
                        result = await loop.run_in_executor(executor, node.process, item)
                    if result is not None:
                        serialized_result = serializer.serialize(result)
                        yield execution_pb2.StreamData(data=serialized_result)

            # After the stream is done, flush the node if possible
            if hasattr(node, 'flush') and callable(getattr(node, 'flush')):
                if inspect.iscoroutinefunction(node.flush):
                    flushed_result = await node.flush()
                else:
                    flushed_result = node.flush()
                if flushed_result is not None:
                    serialized_result = serializer.serialize(flushed_result)
                    yield execution_pb2.StreamData(data=serialized_result)

        except Exception as e:
            # Use enhanced error handling for better diagnostics
            operation = f"StreamNode[{node_type if 'node_type' in locals() else 'unknown'}]"
            error_details = await self.error_handler.handle_error(
                e, operation,
                node_type=node_type if 'node_type' in locals() else 'unknown',
                connection_id=connection_id,
                session_id=session_id
            )
            
            # Send detailed error message back to the client
            yield execution_pb2.StreamData(
                error_message=f"[{error_details['error_category'].upper()}] {error_details['error_message']}"
            )
        
        finally:
            # Clean up all resources for this connection
            await self._cleanup_connection_resources(connection_id)
            self.logger.info(f"StreamNode connection closed for {connection_id}")
    
    async def GetStatus(
        self,
        request: execution_pb2.StatusRequest,
        context: grpc.aio.ServicerContext
    ) -> execution_pb2.StatusResponse:
        """
        Get service status and health information.
        
        Args:
            request: Status request
            context: gRPC context
            
        Returns:
            Service status response
        """
        uptime = int(time.time() - self.start_time)
        
        metrics = types_pb2.ServiceMetrics(
            total_requests=self.request_count,
            successful_requests=self.success_count,
            failed_requests=self.error_count,
            active_sessions=len(self.active_sessions),
            available_workers=self.config.max_workers,
            busy_workers=0  # TODO: Implement worker tracking
        )
        
        return execution_pb2.StatusResponse(
            status=types_pb2.SERVICE_STATUS_HEALTHY,
            metrics=metrics if request.include_metrics else None,
            version=self.config.version,
            uptime_seconds=uptime
        )
    
    async def ListNodes(
        self,
        request: execution_pb2.ListNodesRequest,
        context: grpc.aio.ServicerContext
    ) -> execution_pb2.ListNodesResponse:
        """
        List available SDK nodes.
        
        Args:
            request: List nodes request
            context: gRPC context
            
        Returns:
            List of available nodes
        """
        available_nodes = await self.executor.get_available_nodes(request.category)
        
        # Convert dictionary list to NodeInfo messages
        node_info_list = []
        for node_data in available_nodes:
            node_info = execution_pb2.NodeInfo(
                node_type=node_data.get('node_type', ''),
                category=node_data.get('category', ''),
                description=node_data.get('description', ''),
                # Note: parameters field would need proper conversion if used
            )
            node_info_list.append(node_info)
        
        return execution_pb2.ListNodesResponse(
            available_nodes=node_info_list
        )
    
    def _build_metrics(self, start_time: float, result: Any) -> types_pb2.ExecutionMetrics:
        """Build execution metrics from result."""
        end_time = time.time()
        return types_pb2.ExecutionMetrics(
            start_timestamp=int(start_time * 1000),
            end_timestamp=int(end_time * 1000),
            duration_ms=int((end_time - start_time) * 1000),
            input_size_bytes=getattr(result, 'input_size', 0),
            output_size_bytes=getattr(result, 'output_size', 0),
            memory_peak_mb=getattr(result, 'memory_peak', 0),
            cpu_time_ms=getattr(result, 'cpu_time', 0)
        )
    
    def _build_error_metrics(self, start_time: float) -> types_pb2.ExecutionMetrics:
        """Build metrics for failed execution."""
        end_time = time.time()
        return types_pb2.ExecutionMetrics(
            start_timestamp=int(start_time * 1000),
            end_timestamp=int(end_time * 1000),
            duration_ms=int((end_time - start_time) * 1000),
            exit_code=-1
        )
    

    
    async def _handle_stream_init(self, init_request) -> str:
        """Handle streaming session initialization."""
        # TODO: Implement streaming session management
        session_id = f"session_{int(time.time() * 1000)}"
        self.active_sessions[session_id] = {
            'created': time.time(),
            'node_type': init_request.node_type,
            'processed_items': 0
        }
        return session_id
    
    async def _handle_stream_data(self, data_request):
        """Handle streaming data processing."""
        # TODO: Implement streaming data processing
        return execution_pb2.StreamDataResponse(
            session_id=data_request.session_id,
            output_data=data_request.input_data  # Echo for now
        )
    
    async def _handle_stream_close(self, close_request):
        """Handle streaming session closure."""
        session_id = close_request.session_id
        if session_id in self.active_sessions:
            session = self.active_sessions[session_id]
            return execution_pb2.StreamCloseResponse(
                session_id=session_id,
                total_metrics=types_pb2.ExecutionMetrics(
                    duration_ms=int((time.time() - session['created']) * 1000)
                )
            )
        return execution_pb2.StreamCloseResponse(session_id=session_id)
    
    async def InitGenerator(
        self,
        request: execution_pb2.InitGeneratorRequest,
        context: grpc.aio.ServicerContext
    ) -> execution_pb2.InitGeneratorResponse:
        """Initialize a new generator from an object method."""
        try:
            # Get object from session
            if request.session_id not in self.object_sessions:
                raise ValueError("Session not found")
            
            obj = self.object_sessions[request.session_id]['object']
            
            # Deserialize arguments
            serializer = PickleSerializer() if request.serialization_format == 'pickle' else JSONSerializer()
            method_args = serializer.deserialize(request.method_args_data)
            
            # Deserialize keyword arguments if provided
            method_kwargs = {}
            if request.method_kwargs_data:
                method_kwargs = serializer.deserialize(request.method_kwargs_data)
            
            # Call method to get generator
            attr = getattr(obj, request.method_name)
            if asyncio.iscoroutinefunction(attr):
                result = await attr(*method_args, **method_kwargs)
            else:
                result = attr(*method_args, **method_kwargs)
            
            # Verify it's a generator
            if not (inspect.isgenerator(result) or inspect.isasyncgen(result)):
                raise ValueError(f"Method {request.method_name} did not return a generator")
            
            # Create generator session
            generator_id = str(uuid.uuid4())
            self.generator_sessions[generator_id] = GeneratorSession(
                generator=result,
                session_id=generator_id
            )
            
            return execution_pb2.InitGeneratorResponse(
                status=types_pb2.EXECUTION_STATUS_SUCCESS,
                generator_id=generator_id
            )
        except Exception as e:
            # Use enhanced error handling
            operation = f"InitGenerator[{request.method_name}]"
            error_details = await self.error_handler.handle_error(
                e, operation,
                session_id=request.session_id,
                method_name=request.method_name,
                serialization_format=request.serialization_format
            )
            
            return execution_pb2.InitGeneratorResponse(
                status=error_details["status"],
                error_message=error_details["error_message"]
            )
    
    async def GetNextBatch(
        self,
        request: execution_pb2.GetNextBatchRequest,
        context: grpc.aio.ServicerContext
    ) -> execution_pb2.GetNextBatchResponse:
        """Get next batch of items from a generator."""
        try:
            if request.generator_id not in self.generator_sessions:
                raise ValueError("Generator session not found")
            
            session = self.generator_sessions[request.generator_id]
            session.last_accessed = time.time()
            
            async with session.lock:
                if session.is_exhausted:
                    return execution_pb2.GetNextBatchResponse(
                        status=types_pb2.EXECUTION_STATUS_SUCCESS,
                        items=[],
                        has_more=False
                    )
                
                serializer = PickleSerializer() if request.serialization_format == 'pickle' else JSONSerializer()
                items = []
                
                for _ in range(request.batch_size):
                    try:
                        if inspect.isasyncgen(session.generator):
                            item = await session.generator.__anext__()
                        else:
                            item = next(session.generator)
                        
                        items.append(serializer.serialize(item))
                    except (StopIteration, StopAsyncIteration):
                        session.is_exhausted = True
                        break
                
                return execution_pb2.GetNextBatchResponse(
                    status=types_pb2.EXECUTION_STATUS_SUCCESS,
                    items=items,
                    has_more=not session.is_exhausted
                )
        except Exception as e:
            # Use enhanced error handling
            operation = f"GetNextBatch[{request.generator_id}]"
            error_details = await self.error_handler.handle_error(
                e, operation,
                generator_id=request.generator_id,
                batch_size=request.batch_size,
                serialization_format=request.serialization_format
            )
            
            return execution_pb2.GetNextBatchResponse(
                status=error_details["status"],
                error_message=error_details["error_message"]
            )
    
    async def CloseGenerator(
        self,
        request: execution_pb2.CloseGeneratorRequest,
        context: grpc.aio.ServicerContext
    ) -> execution_pb2.CloseGeneratorResponse:
        """Close and cleanup a generator session."""
        try:
            if request.generator_id in self.generator_sessions:
                session = self.generator_sessions[request.generator_id]
                
                # Close generator if it has close method
                if hasattr(session.generator, 'aclose'):
                    await session.generator.aclose()
                elif hasattr(session.generator, 'close'):
                    session.generator.close()
                
                del self.generator_sessions[request.generator_id]
            
            return execution_pb2.CloseGeneratorResponse(
                status=types_pb2.EXECUTION_STATUS_SUCCESS
            )
        except Exception as e:
            # Use enhanced error handling
            operation = f"CloseGenerator[{request.generator_id}]"
            error_details = await self.error_handler.handle_error(
                e, operation,
                generator_id=request.generator_id
            )
            
            return execution_pb2.CloseGeneratorResponse(
                status=error_details["status"]
            )
    
    # Pipeline management methods
    
    async def RegisterPipeline(
        self,
        request: execution_pb2.RegisterPipelineRequest,
        context: grpc.aio.ServicerContext
    ) -> execution_pb2.RegisterPipelineResponse:
        """Register a new pipeline for remote execution."""
        try:
            from remotemedia.core.pipeline_registry import get_global_registry
            
            registry = get_global_registry()
            
            # Convert proto definition to dict
            definition = {
                "name": request.definition.name,
                "nodes": [
                    {
                        "node_id": node.node_id,
                        "node_type": node.node_type,
                        "config": dict(node.config),
                        "is_remote": node.is_remote,
                        "remote_endpoint": node.remote_endpoint,
                        "is_streaming": node.is_streaming,
                        "is_source": node.is_source,
                        "is_sink": node.is_sink
                    }
                    for node in request.definition.nodes
                ],
                "connections": [
                    {
                        "from_node": conn.from_node,
                        "to_node": conn.to_node,
                        "output_port": conn.output_port,
                        "input_port": conn.input_port
                    }
                    for conn in request.definition.connections
                ],
                "config": dict(request.definition.config),
                "metadata": dict(request.definition.metadata)
            }
            
            # Register the pipeline
            pipeline_id = await registry.register_pipeline(
                name=request.pipeline_name,
                definition=definition,
                metadata=dict(request.metadata),
                dependencies=list(request.dependencies),
                category=request.metadata.get("category", "general"),
                description=request.metadata.get("description", "")
            )
            
            self.logger.info(f"Registered pipeline '{request.pipeline_name}' with ID: {pipeline_id}")
            
            return execution_pb2.RegisterPipelineResponse(
                status=types_pb2.EXECUTION_STATUS_SUCCESS,
                pipeline_id=pipeline_id,
                registered_timestamp=int(time.time())
            )
            
        except Exception as e:
            self.logger.error(f"Failed to register pipeline: {e}")
            return execution_pb2.RegisterPipelineResponse(
                status=types_pb2.EXECUTION_STATUS_ERROR,
                error_message=str(e)
            )
    
    async def UnregisterPipeline(
        self,
        request: execution_pb2.UnregisterPipelineRequest,
        context: grpc.aio.ServicerContext
    ) -> execution_pb2.UnregisterPipelineResponse:
        """Unregister a pipeline."""
        try:
            from remotemedia.core.pipeline_registry import get_global_registry
            
            registry = get_global_registry()
            success = await registry.unregister_pipeline(request.pipeline_id)
            
            if success:
                return execution_pb2.UnregisterPipelineResponse(
                    status=types_pb2.EXECUTION_STATUS_SUCCESS
                )
            else:
                return execution_pb2.UnregisterPipelineResponse(
                    status=types_pb2.EXECUTION_STATUS_ERROR,
                    error_message=f"Pipeline not found: {request.pipeline_id}"
                )
                
        except Exception as e:
            self.logger.error(f"Failed to unregister pipeline: {e}")
            return execution_pb2.UnregisterPipelineResponse(
                status=types_pb2.EXECUTION_STATUS_ERROR,
                error_message=str(e)
            )
    
    async def ListPipelines(
        self,
        request: execution_pb2.ListPipelinesRequest,
        context: grpc.aio.ServicerContext
    ) -> execution_pb2.ListPipelinesResponse:
        """List registered pipelines."""
        try:
            from remotemedia.core.pipeline_registry import get_global_registry
            
            registry = get_global_registry()
            pipelines = registry.list_pipelines(
                category=request.category if request.category else None,
                include_definitions=request.include_definitions
            )
            
            # Convert to proto format
            pipeline_infos = []
            for p in pipelines:
                info = execution_pb2.PipelineInfo(
                    pipeline_id=p["pipeline_id"],
                    name=p["name"],
                    category=p["category"],
                    description=p["description"],
                    registered_timestamp=int(p["registered_timestamp"]),
                    usage_count=p["usage_count"]
                )
                
                # Add metadata
                for k, v in p.get("metadata", {}).items():
                    info.metadata[k] = str(v)
                
                # Add definition if requested
                if request.include_definitions and "definition" in p:
                    # Convert definition to proto format
                    definition = p["definition"]
                    info.definition.name = definition["name"]
                    
                    for node in definition.get("nodes", []):
                        node_def = info.definition.nodes.add()
                        node_def.node_id = node["node_id"]
                        node_def.node_type = node["node_type"]
                        for k, v in node.get("config", {}).items():
                            node_def.config[k] = str(v)
                        node_def.is_remote = node.get("is_remote", False)
                        node_def.remote_endpoint = node.get("remote_endpoint", "")
                        node_def.is_streaming = node.get("is_streaming", False)
                        node_def.is_source = node.get("is_source", False)
                        node_def.is_sink = node.get("is_sink", False)
                    
                    for conn in definition.get("connections", []):
                        conn_def = info.definition.connections.add()
                        conn_def.from_node = conn["from_node"]
                        conn_def.to_node = conn["to_node"]
                        conn_def.output_port = conn.get("output_port", "default")
                        conn_def.input_port = conn.get("input_port", "default")
                
                pipeline_infos.append(info)
            
            return execution_pb2.ListPipelinesResponse(pipelines=pipeline_infos)
            
        except Exception as e:
            self.logger.error(f"Failed to list pipelines: {e}")
            return execution_pb2.ListPipelinesResponse(pipelines=[])
    
    async def GetPipelineInfo(
        self,
        request: execution_pb2.GetPipelineInfoRequest,
        context: grpc.aio.ServicerContext
    ) -> execution_pb2.GetPipelineInfoResponse:
        """Get detailed pipeline information."""
        try:
            from remotemedia.core.pipeline_registry import get_global_registry
            
            registry = get_global_registry()
            info = registry.get_pipeline_info(
                request.pipeline_id,
                include_definition=request.include_definition,
                include_metrics=request.include_metrics
            )
            
            if not info:
                return execution_pb2.GetPipelineInfoResponse(
                    status=types_pb2.EXECUTION_STATUS_ERROR,
                    error_message=f"Pipeline not found: {request.pipeline_id}"
                )
            
            # Convert to proto format
            pipeline_info = execution_pb2.PipelineInfo(
                pipeline_id=info["pipeline_id"],
                name=info["name"],
                category=info["category"],
                description=info["description"],
                registered_timestamp=int(info["registered_timestamp"]),
                usage_count=info["usage_count"]
            )
            
            # Add metadata
            for k, v in info.get("metadata", {}).items():
                pipeline_info.metadata[k] = str(v)
            
            response = execution_pb2.GetPipelineInfoResponse(
                status=types_pb2.EXECUTION_STATUS_SUCCESS,
                pipeline_info=pipeline_info
            )
            
            # Add metrics if requested
            if request.include_metrics and "metrics" in info:
                metrics = info["metrics"]
                response.metrics.total_executions = metrics["total_executions"]
                response.metrics.total_errors = metrics["total_errors"]
                response.metrics.average_execution_time_ms = metrics["average_execution_time_ms"]
                if metrics["last_execution_timestamp"]:
                    response.metrics.last_execution_timestamp = int(metrics["last_execution_timestamp"])
            
            return response
            
        except Exception as e:
            self.logger.error(f"Failed to get pipeline info: {e}")
            return execution_pb2.GetPipelineInfoResponse(
                status=types_pb2.EXECUTION_STATUS_ERROR,
                error_message=str(e)
            )
    
    async def ExecutePipeline(
        self,
        request: execution_pb2.ExecutePipelineRequest,
        context: grpc.aio.ServicerContext
    ) -> execution_pb2.ExecutePipelineResponse:
        """Execute a registered pipeline."""
        try:
            from remotemedia.core.pipeline_registry import get_global_registry
            
            registry = get_global_registry()
            
            # Deserialize input data
            serializer = self._get_serializer(request.serialization_format)
            input_data = serializer.deserialize(request.input_data)
            
            # Execute pipeline
            result = await registry.execute_pipeline(
                request.pipeline_id,
                input_data,
                runtime_config=dict(request.runtime_config) if request.runtime_config else None
            )
            
            # Serialize result
            output_data = serializer.serialize(result)
            
            return execution_pb2.ExecutePipelineResponse(
                status=types_pb2.EXECUTION_STATUS_SUCCESS,
                output_data=output_data,
                execution_id=f"exec_{request.pipeline_id}_{int(time.time() * 1000)}"
            )
            
        except Exception as e:
            self.logger.error(f"Failed to execute pipeline: {e}")
            return execution_pb2.ExecutePipelineResponse(
                status=types_pb2.EXECUTION_STATUS_ERROR,
                error_message=str(e),
                error_traceback=traceback.format_exc()
            )
    
    async def StreamPipeline(
        self,
        request_iterator: AsyncIterable[execution_pb2.StreamPipelineRequest],
        context: grpc.aio.ServicerContext
    ) -> AsyncGenerator[execution_pb2.StreamPipelineResponse, None]:
        """Stream data through a registered pipeline using GRPCStreamSource."""
        session_id = None
        pipeline_instance = None
        serializer = None
        grpc_source = None
        pipeline_task = None
        
        try:
            from remotemedia.core.pipeline_registry import get_global_registry
            from remotemedia.nodes.grpc_source import GRPCStreamSource
            from remotemedia.core.pipeline import Pipeline
            
            registry = get_global_registry()
            
            async for request in request_iterator:
                if request.HasField("init"):
                    # Initialize streaming session
                    init = request.init
                    session_id = f"stream_{init.pipeline_id}_{int(time.time() * 1000)}"
                    
                    # Get original pipeline instance
                    original_pipeline = await registry.get_pipeline_instance(init.pipeline_id)
                    if not original_pipeline:
                        yield execution_pb2.StreamPipelineResponse(
                            error=f"Pipeline not found: {init.pipeline_id}"
                        )
                        return
                    
                    # Create a new pipeline with GRPCStreamSource as the first node
                    pipeline_instance = Pipeline(name=f"grpc_streaming_{original_pipeline.name}")
                    
                    # Create and add GRPC source
                    grpc_source = GRPCStreamSource(
                        session_id=session_id,
                        name=f"GRPCSource_{session_id}"
                    )
                    pipeline_instance.add_node(grpc_source)
                    
                    # Add all nodes from the original pipeline
                    for node in original_pipeline.nodes:
                        pipeline_instance.add_node(node)
                    
                    # Initialize the new pipeline
                    await pipeline_instance.initialize()
                    
                    # Set up serializer
                    serializer = self._get_serializer(init.serialization_format)
                    
                    # Start pipeline processing in the background
                    async def pipeline_processor():
                        try:
                            async for result in pipeline_instance.process():
                                if result is not None:
                                    output_data = serializer.serialize(result)
                                    yield execution_pb2.StreamPipelineResponse(data=output_data)
                        except Exception as e:
                            self.logger.error(f"Pipeline processing error: {e}")
                            yield execution_pb2.StreamPipelineResponse(error=str(e))
                    
                    # We'll collect results from the pipeline processor
                    pipeline_generator = pipeline_processor()
                    
                    # Create session
                    registry.create_streaming_session(init.pipeline_id, session_id)
                    
                    # Send acknowledgment
                    ack = execution_pb2.StreamPipelineAck(
                        session_id=session_id,
                        ready=True
                    )
                    yield execution_pb2.StreamPipelineResponse(ack=ack)
                    
                elif request.HasField("data"):
                    # Add data to the GRPC source
                    if not grpc_source or not serializer:
                        yield execution_pb2.StreamPipelineResponse(
                            error="Session not initialized"
                        )
                        return
                    
                    # Deserialize input
                    input_data = serializer.deserialize(request.data)
                    
                    # Add data to the GRPC source node
                    await grpc_source.add_data(input_data)
                    
                    # Try to get results from the pipeline
                    try:
                        result = await asyncio.wait_for(pipeline_generator.__anext__(), timeout=0.1)
                        yield result
                    except (asyncio.TimeoutError, StopAsyncIteration):
                        # No immediate result, that's fine
                        pass
                    
                elif request.HasField("control"):
                    # Handle control messages
                    control = request.control
                    
                    if control.type == execution_pb2.StreamControl.CLOSE:
                        if grpc_source:
                            await grpc_source.end_stream()
                        if session_id:
                            registry.close_streaming_session(session_id)
                        if pipeline_instance and pipeline_instance.is_initialized:
                            await pipeline_instance.cleanup()
                        
                        # Drain any remaining results
                        try:
                            async for result in pipeline_generator:
                                yield result
                        except StopAsyncIteration:
                            pass
                        return
                    
                    elif control.type == execution_pb2.StreamControl.FLUSH:
                        if pipeline_instance:
                            # Flush any buffered data
                            for node in pipeline_instance.nodes:
                                if hasattr(node, 'flush'):
                                    await node.flush()
                        
                        # Try to get any flushed results
                        try:
                            while True:
                                result = await asyncio.wait_for(pipeline_generator.__anext__(), timeout=0.1)
                                yield result
                        except (asyncio.TimeoutError, StopAsyncIteration):
                            pass
                    
                    # Send status update
                    status = execution_pb2.StreamPipelineStatus(
                        session_id=session_id or "",
                        is_active=grpc_source._is_active if grpc_source else False
                    )
                    yield execution_pb2.StreamPipelineResponse(status=status)
                    
        except Exception as e:
            self.logger.error(f"Stream pipeline error: {e}")
            yield execution_pb2.StreamPipelineResponse(
                error=str(e)
            )
        finally:
            # Clean up
            if grpc_source:
                await grpc_source.end_stream()
                await grpc_source.cleanup()
            if session_id:
                registry.close_streaming_session(session_id)
            if pipeline_instance and pipeline_instance.is_initialized:
                await pipeline_instance.cleanup()


class HealthServicer(health_pb2_grpc.HealthServicer):
    """Health check servicer for the gRPC server."""
    
    def Check(self, request, context):
        """Perform health check."""
        return HealthCheckResponse(
            status=HealthCheckResponse.SERVING
        )


async def serve(custom_node_registry: Dict[str, type] = None, custom_executor: TaskExecutor = None):
    """
    Starts the gRPC server.
    
    Args:
        custom_node_registry: Optional dictionary of custom nodes to register
        custom_executor: Optional custom TaskExecutor instance
    """
    # Load configuration
    config = ServiceConfig()
    
    # Set up logging
    logging.basicConfig(
        level=getattr(logging, config.log_level.upper()),
        format='%(asctime)s - %(name)s - %(levelname)s - %(message)s'
    )
    logger = logging.getLogger(__name__)
    
    # Create gRPC server
    server = grpc.aio.server(
        futures.ThreadPoolExecutor(max_workers=config.max_workers),
        options=[
            ('grpc.max_receive_message_length', -1),
            ('grpc.max_send_message_length', -1)
        ]
    )
    
    # Create executor (custom if provided, otherwise create with custom registry)
    if custom_executor is None and custom_node_registry is not None:
        custom_executor = TaskExecutor(config, custom_node_registry)
    
    # Add servicers
    execution_pb2_grpc.add_RemoteExecutionServiceServicer_to_server(
        RemoteExecutionServicer(config, custom_executor), server
    )
    health_pb2_grpc.add_HealthServicer_to_server(HealthServicer(), server)
    
    # Configure server
    listen_addr = f'0.0.0.0:{config.grpc_port}'
    server.add_insecure_port(listen_addr)
    
    # Start server
    logger.info(f"Starting RemoteMedia Execution Service on {listen_addr}")
    await server.start()
    
    # Set up graceful shutdown
    def signal_handler(signum, frame):
        logger.info(f"Received signal {signum}, shutting down...")
        asyncio.create_task(server.stop(grace=10))
    
    signal.signal(signal.SIGINT, signal_handler)
    signal.signal(signal.SIGTERM, signal_handler)
    
    # Wait for server termination
    await server.wait_for_termination()
    logger.info("Server stopped")


if __name__ == '__main__':
    try:
        asyncio.run(serve())
    except KeyboardInterrupt:
        print("Server interrupted")
        sys.exit(0) 