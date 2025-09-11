#!/usr/bin/env python3
"""
Setup script for RemoteMedia Service
"""

from setuptools import setup, find_packages
import os

# Read the README file
readme_path = os.path.join(os.path.dirname(__file__), "README.md")
if os.path.exists(readme_path):
    with open(readme_path, "r", encoding="utf-8") as fh:
        long_description = fh.read()
else:
    long_description = "RemoteMedia Processing Service - Backend service for distributed media processing"

# Read requirements
def read_requirements(filename):
    """Read requirements from file"""
    filepath = os.path.join(os.path.dirname(__file__), filename)
    if os.path.exists(filepath):
        with open(filepath, "r", encoding="utf-8") as f:
            return [
                line.strip() for line in f 
                if line.strip() and not line.startswith("#")
            ]
    return []

setup(
    name="remotemedia-service",
    version="0.1.0",
    author="Mathieu Gosbee",
    author_email="mail@matbee.com",
    description=(
        "RemoteMedia Processing Service - Backend service for distributed "
        "audio/video/data processing with remote offloading"
    ),
    long_description=long_description,
    long_description_content_type="text/markdown",
    url="https://github.com/matbeeDOTcom/remotemedia-sdk",
    packages=["remotemedia.service"] + ["remotemedia.service." + p for p in find_packages(where="src")],
    package_dir={"remotemedia.service": "src"},
    classifiers=[
        "Development Status :: 3 - Alpha",
        "Intended Audience :: Developers",
        "Topic :: Multimedia :: Sound/Audio",
        "Topic :: Multimedia :: Video",
        "Topic :: Software Development :: Libraries :: Python Modules",
        "License :: OSI Approved :: MIT License",
        "Programming Language :: Python :: 3",
        "Programming Language :: Python :: 3.9",
        "Programming Language :: Python :: 3.10",
        "Programming Language :: Python :: 3.11",
        "Programming Language :: Python :: 3.12",
    ],
    python_requires=">=3.9",
    install_requires=[
        "grpcio>=1.50.0",
        "grpcio-tools>=1.50.0",
        "grpcio-health-checking>=1.50.0",
        "protobuf>=4.21.0",
        "cloudpickle>=2.2.0",
        "numpy>=1.21.0",
        "aiortc>=1.13.0",
        "aiohttp>=3.10.0",
        "av>=14.0.0",
        "psutil>=5.9.0",
        "pyyaml>=6.0",
        "structlog>=22.0.0",
        "asyncio-mqtt>=0.11.0",
        "aiohttp-cors>=0.7.0",
        "librosa>=0.11.0",
    ],
    extras_require={
        "dev": [
            "pytest>=7.0.0",
            "pytest-asyncio>=0.21.0",
            "pytest-grpc>=0.8.0",
        ],
    },
    entry_points={
        "console_scripts": [
            "remotemedia-server=remotemedia.service.server:main",
        ],
    },
    include_package_data=True,
    package_data={
        "": ["*.yaml", "*.json", "*.proto"],
    },
    zip_safe=False,
)