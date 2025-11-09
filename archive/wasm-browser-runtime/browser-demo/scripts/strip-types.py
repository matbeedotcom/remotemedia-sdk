#!/usr/bin/env python3
"""
Strip type hints from remotemedia package to avoid typing module import.
This eliminates the stack overflow issue in browser WASM execution.
"""

import sys
import pathlib
import shutil
from strip_hints import strip_file_to_string

def strip_package_types(package_path: pathlib.Path, backup: bool = True):
    """
    Strip type hints from all Python files in the package.

    Args:
        package_path: Path to remotemedia package
        backup: Create .py.bak backup files before stripping
    """
    print(f"Stripping type hints from: {package_path}")
    print("-" * 80)

    # Find all .py files
    py_files = list(package_path.rglob("*.py"))

    # Exclude backup files
    py_files = [f for f in py_files if not f.name.endswith('.bak')]

    print(f"Found {len(py_files)} Python files\n")

    stripped_count = 0
    failed_count = 0

    for py_file in sorted(py_files):
        try:
            # Create backup if requested
            if backup:
                backup_file = py_file.with_suffix('.py.bak')
                if not backup_file.exists():
                    shutil.copy2(py_file, backup_file)

            # Read original content
            with open(py_file, 'r', encoding='utf-8') as f:
                original = f.read()

            # Strip type hints
            stripped = strip_file_to_string(
                str(py_file),
                to_empty=True,  # Remove imports that become empty after stripping
                strip_nl=False,  # Keep newlines to preserve line numbers
            )

            # Write stripped content
            with open(py_file, 'w', encoding='utf-8') as f:
                f.write(stripped)

            stripped_count += 1
            print(f"OK {py_file.relative_to(package_path)}")

        except Exception as e:
            failed_count += 1
            print(f"FAIL {py_file.relative_to(package_path)}: {e}")

            # Restore from backup if stripping failed
            if backup:
                backup_file = py_file.with_suffix('.py.bak')
                if backup_file.exists():
                    shutil.copy2(backup_file, py_file)

    print("-" * 80)
    print(f"Stripping complete:")
    print(f"  Stripped: {stripped_count}")
    print(f"  Failed: {failed_count}")

    return stripped_count, failed_count

if __name__ == "__main__":
    # Determine package path
    if len(sys.argv) > 1:
        package_path = pathlib.Path(sys.argv[1])
    else:
        script_dir = pathlib.Path(__file__).parent
        package_path = script_dir.parent.parent / "runtime" / "target" / "wasm32-wasi" / "wasi-deps" / "usr" / "local" / "lib" / "python3.12" / "site-packages" / "remotemedia"

    if not package_path.exists():
        print(f"Error: package path does not exist: {package_path}", file=sys.stderr)
        sys.exit(1)

    # Ask for confirmation
    print(f"\nThis will strip type hints from all Python files in:")
    print(f"  {package_path}")
    print(f"\nBackup files (.py.bak) will be created.")

    response = input("\nContinue? [y/N]: ").strip().lower()
    if response != 'y':
        print("Cancelled.")
        sys.exit(0)

    stripped, failed = strip_package_types(package_path, backup=True)
    sys.exit(0 if failed == 0 else 1)
