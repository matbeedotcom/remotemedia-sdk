#!/usr/bin/env python3
"""
Remove typing module imports from remotemedia package.
This is a second pass after strip-hints to clean up unused imports.
"""

import sys
import pathlib
import re

def remove_typing_imports(package_path: pathlib.Path):
    """
    Remove typing imports from all Python files.

    Args:
        package_path: Path to remotemedia package
    """
    print(f"Removing typing imports from: {package_path}")
    print("-" * 80)

    py_files = list(package_path.rglob("*.py"))
    py_files = [f for f in py_files if not f.name.endswith('.bak')]

    print(f"Found {len(py_files)} Python files\n")

    cleaned_count = 0
    patterns = [
        r'^from typing import .*$',
        r'^import typing$',
        r'^from typing_extensions import .*$',
        r'^import typing_extensions$',
    ]

    for py_file in sorted(py_files):
        try:
            with open(py_file, 'r', encoding='utf-8') as f:
                lines = f.readlines()

            original_count = len(lines)
            new_lines = []
            removed = False

            for line in lines:
                # Check if line matches any typing import pattern
                is_typing_import = False
                for pattern in patterns:
                    if re.match(pattern, line.strip()):
                        is_typing_import = True
                        removed = True
                        break

                if not is_typing_import:
                    new_lines.append(line)

            if removed:
                with open(py_file, 'w', encoding='utf-8') as f:
                    f.writelines(new_lines)

                cleaned_count += 1
                print(f"OK {py_file.relative_to(package_path)} (removed {original_count - len(new_lines)} lines)")

        except Exception as e:
            print(f"FAIL {py_file.relative_to(package_path)}: {e}")

    print("-" * 80)
    print(f"Cleaned {cleaned_count} files")

    return cleaned_count

if __name__ == "__main__":
    script_dir = pathlib.Path(__file__).parent
    package_path = script_dir.parent.parent / "runtime" / "target" / "wasm32-wasi" / "wasi-deps" / "usr" / "local" / "lib" / "python3.12" / "site-packages" / "remotemedia"

    if not package_path.exists():
        print(f"Error: package path does not exist: {package_path}", file=sys.stderr)
        sys.exit(1)

    cleaned = remove_typing_imports(package_path)
    print(f"\nRemoved typing imports from {cleaned} files")
