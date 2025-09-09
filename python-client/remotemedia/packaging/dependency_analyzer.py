"""
AST-based dependency analyzer for detecting local Python file dependencies.

This implements the Phase 3 requirement from the development strategy:
"Detect and package local Python file dependencies (via AST analysis or explicit user declaration)."
"""

import ast
import os
import sys
from pathlib import Path
from typing import Set, List, Dict, Optional, Union
import logging

logger = logging.getLogger(__name__)


class DependencyAnalyzer:
    """
    Analyzes Python code to detect local file dependencies using AST analysis.
    
    This class implements the AST analysis component of the Code & Dependency Packager
    as specified in the development strategy document.
    """
    
    def __init__(self, project_root: Optional[Union[str, Path]] = None):
        """
        Initialize the dependency analyzer.
        
        Args:
            project_root: Root directory of the project. If None, uses current working directory.
        """
        self.project_root = Path(project_root) if project_root else Path.cwd()
        self.analyzed_files: Set[Path] = set()
        self.dependencies: Dict[Path, Set[Path]] = {}
        
    def analyze_file(self, file_path: Union[str, Path]) -> Set[Path]:
        """
        Analyze a Python file to find its local dependencies.
        
        Args:
            file_path: Path to the Python file to analyze
            
        Returns:
            Set of local Python file dependencies
        """
        file_path = Path(file_path).resolve()
        
        if file_path in self.analyzed_files:
            return self.dependencies.get(file_path, set())
        
        self.analyzed_files.add(file_path)
        dependencies = set()
        
        try:
            with open(file_path, 'r', encoding='utf-8') as f:
                content = f.read()
            
            tree = ast.parse(content, filename=str(file_path))
            visitor = ImportVisitor(self.project_root, file_path.parent)
            visitor.visit(tree)
            
            # Find local dependencies
            for import_path in visitor.imports:
                local_file = self._resolve_import_to_file(import_path, file_path.parent)
                if local_file and self._is_local_dependency(local_file):
                    dependencies.add(local_file)
                    
                    # If importing from a package, also include the package's __init__.py
                    if import_path and '.' in import_path:
                        package_parts = import_path.split('.')
                        # Add __init__.py files for all package levels
                        current_dir = file_path.parent
                        for i in range(len(package_parts)):
                            package_path = current_dir
                            for part in package_parts[:i+1]:
                                package_path = package_path / part
                            init_file = package_path / '__init__.py'
                            if init_file.exists() and self._is_local_dependency(init_file):
                                dependencies.add(init_file)
                    
                    # Recursively analyze dependencies
                    sub_deps = self.analyze_file(local_file)
                    dependencies.update(sub_deps)
            
            self.dependencies[file_path] = dependencies
            logger.debug(f"Analyzed {file_path}: found {len(dependencies)} local dependencies")
            
        except Exception as e:
            logger.warning(f"Failed to analyze {file_path}: {e}")
            self.dependencies[file_path] = set()
        
        return dependencies
    
    def analyze_object(self, obj: object) -> Set[Path]:
        """
        Analyze a Python object to find dependencies of its source file.
        
        Args:
            obj: Python object (class, function, etc.)
            
        Returns:
            Set of local Python file dependencies
        """
        try:
            # Get the source file of the object
            import inspect
            source_file = inspect.getfile(obj)
            source_path = Path(source_file).resolve()
            
            if self._is_local_dependency(source_path):
                return self.analyze_file(source_path)
            else:
                logger.debug(f"Object {obj} is not from a local file: {source_file}")
                return set()
                
        except (TypeError, OSError) as e:
            logger.debug(f"Could not get source file for object {obj}: {e}")
            return set()
    
    def _resolve_import_to_file(self, import_path: str, current_dir: Path) -> Optional[Path]:
        """
        Resolve an import statement to a file path.
        
        Args:
            import_path: Import path (e.g., 'mymodule.submodule')
            current_dir: Directory of the file containing the import
            
        Returns:
            Path to the imported file, or None if not found
        """
        # Handle relative imports
        if import_path.startswith('.'):
            # Relative import - resolve relative to current directory
            parts = import_path.lstrip('.').split('.')
            if not parts or parts == ['']:
                # Import of current package
                target_dir = current_dir
                target_file = target_dir / '__init__.py'
            else:
                target_dir = current_dir
                for part in parts[:-1]:
                    target_dir = target_dir / part
                
                # Try both module.py and module/__init__.py
                target_file = target_dir / f"{parts[-1]}.py"
                if not target_file.exists():
                    target_file = target_dir / parts[-1] / '__init__.py'
        else:
            # Absolute import - try to resolve from project root and current directory
            parts = import_path.split('.')
            
            # Try from current directory first (local imports)
            target_dir = current_dir
            for part in parts[:-1]:
                target_dir = target_dir / part
            
            target_file = target_dir / f"{parts[-1]}.py"
            if not target_file.exists():
                target_file = target_dir / parts[-1] / '__init__.py'
            
            # If not found, try from project root
            if not target_file.exists():
                target_dir = self.project_root
                for part in parts[:-1]:
                    target_dir = target_dir / part
                
                target_file = target_dir / f"{parts[-1]}.py"
                if not target_file.exists():
                    target_file = target_dir / parts[-1] / '__init__.py'
        
        return target_file if target_file.exists() else None
    
    def _is_local_dependency(self, file_path: Path) -> bool:
        """
        Check if a file is a local dependency (within project root).
        
        Args:
            file_path: Path to check
            
        Returns:
            True if the file is a local dependency
        """
        try:
            file_path.resolve().relative_to(self.project_root.resolve())
            return True
        except ValueError:
            return False
    
    def get_all_dependencies(self, entry_files: List[Union[str, Path]]) -> Set[Path]:
        """
        Get all dependencies for a list of entry files.
        
        Args:
            entry_files: List of Python files to analyze
            
        Returns:
            Set of all local dependencies
        """
        all_deps = set()
        for file_path in entry_files:
            deps = self.analyze_file(file_path)
            all_deps.update(deps)
        return all_deps


class ImportVisitor(ast.NodeVisitor):
    """AST visitor to extract import statements."""
    
    def __init__(self, project_root: Path, current_dir: Path):
        self.project_root = project_root
        self.current_dir = current_dir
        self.imports: Set[str] = set()
    
    def visit_Import(self, node: ast.Import):
        """Visit import statements (import module)."""
        for alias in node.names:
            self.imports.add(alias.name)
        self.generic_visit(node)
    
    def visit_ImportFrom(self, node: ast.ImportFrom):
        """Visit from-import statements (from module import name)."""
        if node.module:
            # Handle relative imports
            if node.level > 0:
                # Relative import
                module_path = '.' * node.level + (node.module or '')
            else:
                # Absolute import
                module_path = node.module
            
            self.imports.add(module_path)
        self.generic_visit(node) 