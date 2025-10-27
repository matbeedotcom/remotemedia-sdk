#!/usr/bin/env python3
"""
Analyze minimal Python dependencies for basic remotemedia nodes (text_processor, calculator).
Only analyzes core + basic nodes, excludes ML/audio/video nodes.
"""

import sys
import pathlib
from modulefinder import ModuleFinder

def analyze_minimal_dependencies(package_path: pathlib.Path, stdlib_path: pathlib.Path):
    """
    Analyze import dependencies for minimal remotemedia usage.

    Args:
        package_path: Path to remotemedia package
        stdlib_path: Path to Python stdlib

    Returns:
        Set of stdlib module names that are imported
    """
    finder = ModuleFinder(path=[str(stdlib_path)])

    print(f"Analyzing MINIMAL dependencies for: {package_path}")
    print("-" * 80)

    # Only analyze core + basic nodes (exclude ML/audio/video)
    files_to_analyze = [
        # Core
        "core/__init__.py",
        "core/exceptions.py",
        "core/node.py",
        "core/pipeline.py",
        "core/types.py",
        "core/wasm_compat.py",
        # Basic nodes
        "nodes/__init__.py",
        "nodes/calculator.py",
        "nodes/text_processor.py",
        "nodes/simple_math.py",
        # Utilities
        "utils/__init__.py",
        "utils/logging.py",
    ]

    print(f"Analyzing {len(files_to_analyze)} essential files:\n")

    # Run modulefinder on each file
    for rel_path in files_to_analyze:
        py_file = package_path / rel_path
        if not py_file.exists():
            print(f"Warning: {rel_path} not found")
            continue

        print(f"  {rel_path}")
        try:
            finder.run_script(str(py_file))
        except Exception as e:
            print(f"    Warning: {e}")

    print("\n" + "=" * 80)
    print("MINIMAL STDLIB MODULES NEEDED:")
    print("=" * 80)

    # Collect stdlib modules
    stdlib_modules = set()

    for name, mod in finder.modules.items():
        if not mod.__file__:
            # Builtin module (no file)
            continue

        mod_file = pathlib.Path(mod.__file__)

        # Check if module is from stdlib
        try:
            if stdlib_path in mod_file.parents or mod_file.is_relative_to(stdlib_path):
                # Get top-level module name
                top_level = name.split('.')[0]
                stdlib_modules.add(top_level)
        except (ValueError, OSError):
            pass

    for mod_name in sorted(stdlib_modules):
        print(f"  - {mod_name}")

    print(f"\nTotal: {len(stdlib_modules)} modules")

    # Check if typing is needed
    if 'typing' in stdlib_modules:
        print("\nWARNING: 'typing' module is required (will cause stack overflow)")
    else:
        print("\nGOOD: 'typing' module is NOT required!")

    return stdlib_modules

if __name__ == "__main__":
    script_dir = pathlib.Path(__file__).parent
    package_path = script_dir.parent.parent / "runtime" / "target" / "wasm32-wasi" / "wasi-deps" / "usr" / "local" / "lib" / "python3.12" / "site-packages" / "remotemedia"
    stdlib_path = script_dir.parent.parent / "runtime" / "target" / "wasm32-wasi" / "wasi-deps" / "usr" / "local" / "lib" / "python3.12"

    if not package_path.exists():
        print(f"Error: package path does not exist: {package_path}", file=sys.stderr)
        sys.exit(1)

    deps = analyze_minimal_dependencies(package_path, stdlib_path)

    # Write to file
    output_file = script_dir / "minimal-stdlib-deps.txt"
    with open(output_file, 'w') as f:
        for mod_name in sorted(deps):
            f.write(f"{mod_name}\n")

    print(f"\nDependency list written to: {output_file}")
