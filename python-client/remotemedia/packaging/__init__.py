"""
Code & Dependency Packager for RemoteMedia SDK.

This module implements the Phase 3 requirement for packaging user-defined Python code
with local dependencies for remote execution, as specified in the development strategy.
"""

from .dependency_analyzer import DependencyAnalyzer
from .code_packager import CodePackager

__all__ = ["DependencyAnalyzer", "CodePackager"] 