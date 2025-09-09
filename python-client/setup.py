#!/usr/bin/env python3
"""
Setup script for RemoteMedia Processing SDK
"""

from setuptools import setup, find_packages
import os

# Read the README file
with open("README.md", "r", encoding="utf-8") as fh:
    long_description = fh.read()

# Read requirements


def read_requirements(filename):
    """Read requirements from file"""
    if os.path.exists(filename):
        with open(filename, "r", encoding="utf-8") as f:
            return [
                line.strip() for line in f 
                if line.strip() and not line.startswith("#")
            ]
    return []


setup(
    name="remotemedia",
    version="0.1.0",
    author="RemoteMedia Team",
    author_email="team@remotemedia.dev",
    description=(
        "A Python SDK for distributed audio/video/data processing "
        "with remote offloading"
    ),
    long_description=long_description,
    long_description_content_type="text/markdown",
    url="https://github.com/remotemedia/remotemedia-sdk",
    packages=find_packages(include=['remotemedia*', 'remote_service*']),
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
        "grpcio",
        "grpcio-tools",
        "protobuf",
        "numpy",
        "av",
        "cloudpickle"
    ],
    extras_require={
        "dev": read_requirements("requirements-dev.txt"),
        "ml": read_requirements("requirements-ml.txt"),
    },
    entry_points={
        "console_scripts": [
            "remotemedia=remotemedia.cli:main",
        ],
    },
    include_package_data=True,
    package_data={
        "remotemedia": ["*.yaml", "*.json"],
    },
    zip_safe=False,
) 