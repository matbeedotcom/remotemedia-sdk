#!/usr/bin/env python3
"""
Setup script for RemoteMedia shared package (protobuf definitions)
"""

from setuptools import setup, find_packages

setup(
    name="remotemedia",
    version="0.1.0",
    author="Mathieu Gosbee",
    author_email="mail@matbee.com",
    description="RemoteMedia Processing - Shared protobuf definitions",
    url="https://github.com/matbeeDOTcom/remotemedia-sdk",
    packages=["remotemedia", "remotemedia.protos"],
    package_dir={"remotemedia": "remotemedia"},
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
        "protobuf>=4.21.0",
        "grpcio>=1.50.0",
        "grpcio-tools>=1.50.0",
        "numpy>=1.21.0,<2.0",
        "av>=14.0.0",
        "cloudpickle>=2.2.0",
    ],
    include_package_data=True,
    package_data={
        "remotemedia.protos": ["*.proto", "*.py"],
    },
    zip_safe=False,
)
