#!/usr/bin/env python3
"""
Minify Python stdlib modules to reduce recursion depth in browser.
Uses python-minifier to reduce complexity.
"""

import sys
import pathlib
import subprocess
import shutil

def minify_stdlib(stdlib_path: pathlib.Path):
    """
    Minify all Python files in stdlib.

    Args:
        stdlib_path: Path to Python stdlib
    """
    print(f"Minifying Python stdlib: {stdlib_path}")
    print("-" * 80)

    # Find all .py files (excluding site-packages which we handle separately)
    py_files = []
    for py_file in stdlib_path.rglob("*.py"):
        # Skip site-packages (we'll handle remotemedia separately)
        if "site-packages" in py_file.parts:
            continue
        # Skip test files
        if "test" in py_file.parts or py_file.name.startswith("test_"):
            continue
        # Skip __pycache__
        if "__pycache__" in py_file.parts:
            continue
        py_files.append(py_file)

    print(f"Found {len(py_files)} stdlib files to minify\n")

    minified_count = 0
    failed_count = 0
    skipped_count = 0

    for py_file in sorted(py_files):
        # Skip if backup already exists (already minified)
        backup_file = py_file.with_suffix('.py.original')
        if backup_file.exists():
            skipped_count += 1
            continue

        try:
            # Create backup
            shutil.copy2(py_file, backup_file)

            # Minify with pyminify
            # Don't remove asserts or debug code, just simplify structure
            result = subprocess.run(
                ['pyminify', str(py_file), '--output', str(py_file)],
                capture_output=True,
                text=True,
                check=True
            )

            minified_count += 1
            if minified_count % 50 == 0:
                print(f"Minified {minified_count} files...")

        except subprocess.CalledProcessError as e:
            failed_count += 1
            # Restore from backup on failure
            shutil.copy2(backup_file, py_file)
            print(f"FAIL {py_file.relative_to(stdlib_path)}: {e.stderr[:100]}")
        except Exception as e:
            failed_count += 1
            print(f"FAIL {py_file.relative_to(stdlib_path)}: {e}")

    print("-" * 80)
    print(f"Minification complete:")
    print(f"  Minified: {minified_count}")
    print(f"  Skipped (already done): {skipped_count}")
    print(f"  Failed: {failed_count}")

    return minified_count, failed_count

if __name__ == "__main__":
    script_dir = pathlib.Path(__file__).parent
    stdlib_path = script_dir.parent.parent / "runtime" / "target" / "wasm32-wasi" / "wasi-deps" / "usr" / "local" / "lib" / "python3.12"

    if not stdlib_path.exists():
        print(f"Error: stdlib path does not exist: {stdlib_path}", file=sys.stderr)
        sys.exit(1)

    print(f"\nThis will minify all Python stdlib files in:")
    print(f"  {stdlib_path}")
    print(f"\nBackup files (.py.original) will be created.")
    print(f"This may take several minutes...")

    response = input("\nContinue? [y/N]: ").strip().lower()
    if response != 'y':
        print("Cancelled.")
        sys.exit(0)

    minified, failed = minify_stdlib(stdlib_path)
    sys.exit(0 if failed == 0 else 1)
