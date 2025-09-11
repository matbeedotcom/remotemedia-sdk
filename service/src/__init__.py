"""
RemoteMedia Processing Service

Backend service for distributed audio/video/data processing with remote offloading.
"""

__version__ = "0.1.0"
__author__ = "Mathieu Gosbee"
__email__ = "mail@matbee.com"

from .server import *
from .base_server import *
from .executor import *
from .sandbox import *
from .config import *
from .health_check import *

__all__ = [
    "__version__",
    "__author__",
    "__email__",
]