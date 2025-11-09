#!/usr/bin/env python3
"""
Compile Python stdlib .py files to .pyc bytecode to reduce recursion depth during imports.
This helps avoid "Maximum call stack size exceeded" errors in browser WASM execution.
"""

import py_compile
import os
import pathlib
import sys

def compile_stdlib(stdlib_dir: pathlib.Path):
    """
    Compile all .py files in stdlib to .pyc bytecode.

    Args:
        stdlib_dir: Path to the Python stdlib directory
    """
    compiled_count = 0
    failed_count = 0
    skipped_count = 0

    print(f"Compiling Python files in: {stdlib_dir}")
    print("-" * 80)

    for py_file in sorted(stdlib_dir.rglob("*.py")):
        # Skip __pycache__ directories
        if "__pycache__" in py_file.parts:
            continue

        # Generate .pyc file path in __pycache__ directory
        # Python expects: __pycache__/<name>.cpython-312.pyc
        pyc_dir = py_file.parent / "__pycache__"
        pyc_name = f"{py_file.stem}.cpython-312.pyc"
        pyc_file = pyc_dir / pyc_name

        # Check if .pyc already exists and is newer than .py
        if pyc_file.exists() and pyc_file.stat().st_mtime >= py_file.stat().st_mtime:
            skipped_count += 1
            continue

        try:
            # Create __pycache__ directory if it doesn't exist
            pyc_dir.mkdir(exist_ok=True)

            # Compile .py to .pyc
            py_compile.compile(
                str(py_file),
                cfile=str(pyc_file),
                doraise=True,
                optimize=-1  # No optimization
            )

            compiled_count += 1
            print(f"✓ {py_file.relative_to(stdlib_dir)}")

        except Exception as e:
            failed_count += 1
            print(f"✗ {py_file.relative_to(stdlib_dir)}: {e}", file=sys.stderr)

    print("-" * 80)
    print(f"Compilation complete:")
    print(f"  Compiled: {compiled_count}")
    print(f"  Skipped (up-to-date): {skipped_count}")
    print(f"  Failed: {failed_count}")

    return compiled_count, failed_count

if __name__ == "__main__":
    # Determine stdlib directory
    if len(sys.argv) > 1:
        stdlib_dir = pathlib.Path(sys.argv[1])
    else:
        # Default: ../runtime/target/wasm32-wasi/wasi-deps/usr/local/lib/python3.12
        script_dir = pathlib.Path(__file__).parent
        stdlib_dir = script_dir.parent.parent / "runtime" / "target" / "wasm32-wasi" / "wasi-deps" / "usr" / "local" / "lib" / "python3.12"

    if not stdlib_dir.exists():
        print(f"Error: stdlib directory does not exist: {stdlib_dir}", file=sys.stderr)
        sys.exit(1)

    compiled, failed = compile_stdlib(stdlib_dir)
    sys.exit(0 if failed == 0 else 1)
