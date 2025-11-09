#!/usr/bin/env python3
"""
Analyze Python import dependencies for remotemedia package using modulefinder.
This identifies the minimal set of stdlib modules needed to run remotemedia pipelines.
"""

import sys
import pathlib
from modulefinder import ModuleFinder

def analyze_dependencies(package_path: pathlib.Path, stdlib_path: pathlib.Path):
    """
    Analyze import dependencies for all Python files in the package.

    Args:
        package_path: Path to remotemedia package
        stdlib_path: Path to Python stdlib

    Returns:
        Set of stdlib module names that are imported
    """
    finder = ModuleFinder(path=[str(stdlib_path)])

    print(f"Analyzing dependencies for: {package_path}")
    print("-" * 80)

    # Find all .py files in remotemedia package
    py_files = list(package_path.rglob("*.py"))
    print(f"Found {len(py_files)} Python files in remotemedia package\n")

    # Run modulefinder on each file
    for py_file in sorted(py_files):
        print(f"Analyzing: {py_file.relative_to(package_path)}")
        try:
            finder.run_script(str(py_file))
        except Exception as e:
            print(f"  Warning: {e}")

    print("\n" + "=" * 80)
    print("STDLIB MODULES IMPORTED:")
    print("=" * 80)

    # Collect stdlib modules (exclude remotemedia itself and builtins)
    stdlib_modules = set()

    for name, mod in finder.modules.items():
        if not mod.__file__:
            # Builtin module (no file)
            continue

        mod_file = pathlib.Path(mod.__file__)

        # Check if module is from stdlib (not from remotemedia)
        if stdlib_path in mod_file.parents:
            # Get top-level module name
            top_level = name.split('.')[0]
            stdlib_modules.add(top_level)

    for mod_name in sorted(stdlib_modules):
        print(f"  - {mod_name}")

    print(f"\nTotal stdlib modules: {len(stdlib_modules)}")

    # Also report any missing modules
    print("\n" + "=" * 80)
    print("MISSING MODULES (may need to be included):")
    print("=" * 80)

    missing = []
    for name in finder.badmodules.keys():
        # Skip C extensions and platform-specific modules
        if not name.startswith('_') and '.' not in name:
            missing.append(name)

    if missing:
        for mod_name in sorted(missing):
            print(f"  - {mod_name}")
    else:
        print("  (none)")

    return stdlib_modules

if __name__ == "__main__":
    # Determine paths
    if len(sys.argv) > 2:
        package_path = pathlib.Path(sys.argv[1])
        stdlib_path = pathlib.Path(sys.argv[2])
    else:
        script_dir = pathlib.Path(__file__).parent
        package_path = script_dir.parent.parent / "runtime" / "target" / "wasm32-wasi" / "wasi-deps" / "usr" / "local" / "lib" / "python3.12" / "site-packages" / "remotemedia"
        stdlib_path = script_dir.parent.parent / "runtime" / "target" / "wasm32-wasi" / "wasi-deps" / "usr" / "local" / "lib" / "python3.12"

    if not package_path.exists():
        print(f"Error: package path does not exist: {package_path}", file=sys.stderr)
        sys.exit(1)

    if not stdlib_path.exists():
        print(f"Error: stdlib path does not exist: {stdlib_path}", file=sys.stderr)
        sys.exit(1)

    deps = analyze_dependencies(package_path, stdlib_path)

    # Write to file for build-python-fs.js to use
    output_file = script_dir / "stdlib-deps.txt"
    with open(output_file, 'w') as f:
        for mod_name in sorted(deps):
            f.write(f"{mod_name}\n")

    print(f"\n✓ Dependency list written to: {output_file}")
