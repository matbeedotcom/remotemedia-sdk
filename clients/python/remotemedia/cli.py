"""
Command-line interface for the RemoteMedia SDK.
"""

import argparse
import sys
from typing import List, Optional

from . import __version__
from .utils import setup_logging


def main(argv: Optional[List[str]] = None) -> int:
    """
    Main entry point for the RemoteMedia CLI.
    
    Args:
        argv: Command line arguments (defaults to sys.argv)
        
    Returns:
        Exit code (0 for success, non-zero for error)
    """
    parser = argparse.ArgumentParser(
        prog="remotemedia",
        description="RemoteMedia SDK Command Line Interface",
    )
    
    parser.add_argument(
        "--version",
        action="version",
        version=f"RemoteMedia SDK {__version__}",
    )
    
    parser.add_argument(
        "--log-level",
        choices=["DEBUG", "INFO", "WARNING", "ERROR", "CRITICAL"],
        default="INFO",
        help="Set the logging level",
    )
    
    subparsers = parser.add_subparsers(dest="command", help="Available commands")
    
    # Info command
    info_parser = subparsers.add_parser("info", help="Show SDK information")
    info_parser.add_argument(
        "--verbose", "-v", action="store_true", help="Show detailed information"
    )
    
    # Example command
    example_parser = subparsers.add_parser("example", help="Run example pipelines")
    example_parser.add_argument(
        "example_name", nargs="?", default="basic", help="Example to run"
    )
    
    args = parser.parse_args(argv)
    
    # Set up logging
    setup_logging(level=args.log_level)
    
    if args.command == "info":
        return _cmd_info(args)
    elif args.command == "example":
        return _cmd_example(args)
    else:
        parser.print_help()
        return 1


def _cmd_info(args) -> int:
    """Handle the info command."""
    print(f"RemoteMedia SDK {__version__}")
    print("A Python SDK for distributed A/V processing with remote offloading")
    
    if args.verbose:
        print("\nFeatures:")
        print("- Pythonic pipeline API")
        print("- Transparent remote offloading")
        print("- Real-time A/V processing")
        print("- WebRTC integration")
        print("- Secure remote execution")
        
        print(f"\nDevelopment Status: Phase 1 - Core SDK Framework")
    
    return 0


def _cmd_example(args) -> int:
    """Handle the example command."""
    print(f"Running example: {args.example_name}")
    
    if args.example_name == "basic":
        try:
            # Import here to avoid circular imports
            from examples.basic_pipeline import main as basic_main
            return basic_main()
        except ImportError:
            print("Error: Example not found or dependencies missing")
            return 1
    else:
        print(f"Error: Unknown example '{args.example_name}'")
        print("Available examples: basic")
        return 1


if __name__ == "__main__":
    sys.exit(main()) 