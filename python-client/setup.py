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
    name="remotemedia-client",
    version="0.1.0",
    author="Mathieu Gosbee",
    author_email="mail@matbee.com",
    description=(
        "A Python SDK for distributed audio/video/data processing "
        "with remote offloading"
    ),
    long_description=long_description,
    long_description_content_type="text/markdown",
    url="https://github.com/matbeeDOTcom/remotemedia-sdk",
    packages=find_packages(where=".", exclude=["tests", "tests.*"]),
    package_dir={},
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
        "remotemedia",
        "grpcio>=1.50.0",
        "grpcio-tools>=1.50.0",
        "protobuf>=4.21.0",
        "numpy>=1.21.0,<2.0",
        "av>=14.0.0",
        "cloudpickle>=2.2.0"
    ],
    extras_require={
        "dev": read_requirements("requirements-dev.txt"),
        "ml": read_requirements("requirements-ml.txt"),
    },
    entry_points={
        "console_scripts": [
            "remotemedia-cli=remotemedia.client.cli:main",
        ],
    },
    include_package_data=True,
    package_data={
        "remotemedia": ["*.yaml", "*.json"],
    },
    zip_safe=False,
) 