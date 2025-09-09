"""
Code packager for creating deployable archives with dependencies.

This implements the Phase 3 Code & Dependency Packager from the development strategy:
"Use cloudpickle to serialize the user's Python object instance. Detect and package 
local Python file dependencies (via AST analysis or explicit user declaration). 
Optionally package pip requirements. Create a deployable archive (e.g., zip)."
"""

import zipfile
import tempfile
import base64
import json
from pathlib import Path
from typing import Set, List, Dict, Optional, Union, Any
import logging
import io

try:
    import cloudpickle
except ImportError:
    cloudpickle = None

from .dependency_analyzer import DependencyAnalyzer

logger = logging.getLogger(__name__)


class CodePackager:
    """
    Creates deployable archives containing user code and dependencies.
    
    This class implements the complete Code & Dependency Packager as specified
    in the development strategy document.
    """
    
    def __init__(self, project_root: Optional[Union[str, Path]] = None):
        """
        Initialize the code packager.
        
        Args:
            project_root: Root directory of the project
        """
        self.project_root = Path(project_root) if project_root else Path.cwd()
        self.analyzer = DependencyAnalyzer(self.project_root)
        
        if cloudpickle is None:
            raise ImportError("cloudpickle is required for code packaging")
    
    def package_object(
        self,
        obj: object,
        additional_files: Optional[List[Union[str, Path]]] = None,
        pip_requirements: Optional[List[str]] = None,
        exclude_patterns: Optional[List[str]] = None
    ) -> bytes:
        """
        Package a Python object with its dependencies into a deployable archive.
        
        Args:
            obj: Python object to serialize and package
            additional_files: Additional Python files to include
            pip_requirements: List of pip requirements (optional)
            exclude_patterns: File patterns to exclude from packaging
            
        Returns:
            Bytes of the created zip archive
        """
        logger.info(f"Packaging object {type(obj).__name__}")
        
        # Serialize the object with cloudpickle
        serialized_obj = base64.b64encode(cloudpickle.dumps(obj)).decode('ascii')
        
        # Analyze dependencies
        dependencies = self.analyzer.analyze_object(obj)
        
        # Also include the object's own source file
        try:
            import inspect
            source_file = inspect.getfile(obj.__class__)
            source_path = Path(source_file).resolve()
            
            # Always include the source file, even if it's outside project root
            dependencies.add(source_path)
            
            # If the source is outside project root, analyze it for dependencies
            if not self.analyzer._is_local_dependency(source_path):
                logger.info(f"Object source {source_path} is outside project root, analyzing its dependencies")
                # Create a temporary analyzer with the source file's directory as root
                source_dir = source_path.parent
                temp_analyzer = DependencyAnalyzer(project_root=source_dir)
                source_deps = temp_analyzer.analyze_file(source_path)
                # Add dependencies that are in the same directory as the source
                for dep in source_deps:
                    if dep.parent == source_dir:
                        dependencies.add(dep)
        except (TypeError, OSError):
            logger.debug(f"Could not get source for object {obj}, it may be dynamically defined.")
        
        # Add additional files if specified
        if additional_files:
            for file_path in additional_files:
                file_path = Path(file_path).resolve()
                if file_path.exists() and self.analyzer._is_local_dependency(file_path):
                    deps = self.analyzer.analyze_file(file_path)
                    dependencies.update(deps)
                    dependencies.add(file_path)
        
        # Apply exclusion patterns
        if exclude_patterns:
            dependencies = self._apply_exclusions(dependencies, exclude_patterns)
        
        logger.info(f"Found {len(dependencies)} local dependencies")
        for dep in dependencies:
            logger.info(f"  - {dep}")
        
        # Create the archive
        return self._create_archive(
            serialized_obj=serialized_obj,
            dependencies=dependencies,
            pip_requirements=pip_requirements or []
        )
    
    def package_files(
        self,
        entry_files: List[Union[str, Path]],
        pip_requirements: Optional[List[str]] = None,
        exclude_patterns: Optional[List[str]] = None
    ) -> bytes:
        """
        Package Python files with their dependencies into a deployable archive.
        
        Args:
            entry_files: List of Python files to package
            pip_requirements: List of pip requirements (optional)
            exclude_patterns: File patterns to exclude from packaging
            
        Returns:
            Bytes of the created zip archive
        """
        logger.info(f"Packaging {len(entry_files)} entry files")
        
        # Analyze dependencies for all entry files
        dependencies = self.analyzer.get_all_dependencies(entry_files)
        
        # Add entry files to dependencies
        for file_path in entry_files:
            file_path = Path(file_path).resolve()
            if file_path.exists() and self.analyzer._is_local_dependency(file_path):
                dependencies.add(file_path)
        
        # Apply exclusion patterns
        if exclude_patterns:
            dependencies = self._apply_exclusions(dependencies, exclude_patterns)
        
        logger.info(f"Found {len(dependencies)} total dependencies")
        
        # Create the archive
        return self._create_archive(
            serialized_obj=None,
            dependencies=dependencies,
            pip_requirements=pip_requirements or [],
            entry_files=[Path(f).resolve() for f in entry_files]
        )
    
    def _create_archive(
        self,
        serialized_obj: Optional[str],
        dependencies: Set[Path],
        pip_requirements: List[str],
        entry_files: Optional[List[Path]] = None
    ) -> bytes:
        """
        Create a zip archive with the packaged code and dependencies.
        
        Args:
            serialized_obj: Base64-encoded cloudpickle object (optional)
            dependencies: Set of dependency file paths
            pip_requirements: List of pip requirements
            entry_files: List of entry files (optional)
            
        Returns:
            Bytes of the created zip archive
        """
        # Use BytesIO instead of temporary file to avoid Windows permission issues
        archive_buffer = io.BytesIO()
        
        with zipfile.ZipFile(archive_buffer, 'w', zipfile.ZIP_DEFLATED) as zf:
            
            # Create manifest
            def safe_relative_path(file_path: Path) -> str:
                """Get relative path or just filename if outside project root."""
                try:
                    return str(file_path.relative_to(self.project_root))
                except ValueError:
                    return file_path.name
            
            manifest = {
                "version": "1.0",
                "type": "code_package",
                "has_serialized_object": serialized_obj is not None,
                "pip_requirements": pip_requirements,
                "entry_files": [safe_relative_path(f) for f in entry_files] if entry_files else [],
                "dependencies": [safe_relative_path(f) for f in dependencies]
            }
            
            # Add manifest
            zf.writestr("manifest.json", json.dumps(manifest, indent=2))
            
            # Add serialized object if present
            if serialized_obj:
                zf.writestr("serialized_object.pkl", serialized_obj)
            
            # Add pip requirements if present
            if pip_requirements:
                zf.writestr("requirements.txt", "\n".join(pip_requirements))
            
            # Add dependency files
            for dep_file in dependencies:
                try:
                    # Try to calculate relative path from project root
                    try:
                        rel_path = dep_file.relative_to(self.project_root)
                        archive_path = f"code/{rel_path}"
                    except ValueError:
                        # File is outside project root
                        # We need to preserve the module structure for proper imports
                        # Get the module name from the file
                        if dep_file.suffix == '.py':
                            # For Python files, the module name is the stem
                            module_name = dep_file.stem
                            # Check if this is part of a package by looking for __init__.py
                            parent_init = dep_file.parent / '__init__.py'
                            if parent_init.exists():
                                # It's part of a package, preserve package structure
                                package_name = dep_file.parent.name
                                archive_path = f"code/{package_name}/{dep_file.name}"
                            else:
                                # It's a standalone module
                                archive_path = f"code/{dep_file.name}"
                        else:
                            # Non-Python files, just use the filename
                            archive_path = f"code/{dep_file.name}"
                        logger.info(f"Dependency {dep_file} is outside project root, adding as {archive_path}")
                    
                    # Add to archive
                    zf.write(dep_file, archive_path)
                    logger.debug(f"Added dependency: {archive_path}")
                    
                except Exception as e:
                    logger.warning(f"Failed to add dependency {dep_file}: {e}")
            
            # Add entry files (if different from dependencies)
            if entry_files:
                for entry_file in entry_files:
                    if entry_file not in dependencies:
                        try:
                            rel_path = entry_file.relative_to(self.project_root)
                            zf.write(entry_file, f"code/{rel_path}")
                            logger.debug(f"Added entry file: {rel_path}")
                        except Exception as e:
                            logger.warning(f"Failed to add entry file {entry_file}: {e}")
        
        # Return the archive bytes
        return archive_buffer.getvalue()
    
    def _apply_exclusions(self, dependencies: Set[Path], exclude_patterns: List[str]) -> Set[Path]:
        """
        Apply exclusion patterns to filter out unwanted dependencies.
        
        Args:
            dependencies: Set of dependency paths
            exclude_patterns: List of patterns to exclude
            
        Returns:
            Filtered set of dependencies
        """
        import fnmatch
        
        filtered = set()
        for dep in dependencies:
            rel_path = str(dep.relative_to(self.project_root))
            
            # Check if file matches any exclusion pattern
            excluded = False
            for pattern in exclude_patterns:
                if fnmatch.fnmatch(rel_path, pattern) or fnmatch.fnmatch(dep.name, pattern):
                    excluded = True
                    logger.debug(f"Excluding {rel_path} (matches pattern: {pattern})")
                    break
            
            if not excluded:
                filtered.add(dep)
        
        return filtered
    
    def extract_archive_info(self, archive_bytes: bytes) -> Dict[str, Any]:
        """
        Extract information from a code package archive.
        
        Args:
            archive_bytes: Bytes of the zip archive
            
        Returns:
            Dictionary with archive information
        """
        archive_buffer = io.BytesIO(archive_bytes)
        
        with zipfile.ZipFile(archive_buffer, 'r') as zf:
            # Read manifest
            manifest_data = zf.read("manifest.json").decode('utf-8')
            manifest = json.loads(manifest_data)
            
            # Add file list
            manifest["archive_files"] = zf.namelist()
            
            return manifest 