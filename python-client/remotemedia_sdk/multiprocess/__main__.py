"""
Main module for running multiprocess nodes.

This allows the module to be run directly:
    python -m remotemedia_sdk.multiprocess.runner
"""

from .runner import run

if __name__ == "__main__":
    run()