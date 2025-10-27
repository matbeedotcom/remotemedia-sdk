#!/usr/bin/env python3
"""
Remove if TYPE_CHECKING blocks that became broken after removing typing imports.
"""

import sys
import pathlib
import re

def fix_type_checking_blocks(package_path: pathlib.Path):
    """
    Remove if TYPE_CHECKING: blocks from all Python files.

    Args:
        package_path: Path to remotemedia package
    """
    print(f"Fixing TYPE_CHECKING blocks in: {package_path}")
    print("-" * 80)

    py_files = list(package_path.rglob("*.py"))
    py_files = [f for f in py_files if not f.name.endswith('.bak')]

    print(f"Found {len(py_files)} Python files\n")

    fixed_count = 0

    for py_file in sorted(py_files):
        try:
            with open(py_file, 'r', encoding='utf-8') as f:
                lines = f.readlines()

            new_lines = []
            skip_block = False
            block_indent = 0
            removed_lines = 0

            for i, line in enumerate(lines):
                stripped = line.lstrip()

                # Check if this is a TYPE_CHECKING block
                if re.match(r'^if\s+TYPE_CHECKING\s*:', stripped):
                    skip_block = True
                    block_indent = len(line) - len(stripped)
                    removed_lines += 1
                    continue

                # If we're in a TYPE_CHECKING block
                if skip_block:
                    # Check if we're still in the block (indented more than the if statement)
                    current_indent = len(line) - len(line.lstrip())
                    if current_indent > block_indent and line.strip():
                        # Still in block, skip this line
                        removed_lines += 1
                        continue
                    else:
                        # Exited the block
                        skip_block = False
                        block_indent = 0

                new_lines.append(line)

            if removed_lines > 0:
                with open(py_file, 'w', encoding='utf-8') as f:
                    f.writelines(new_lines)

                fixed_count += 1
                print(f"OK {py_file.relative_to(package_path)} (removed {removed_lines} lines)")

        except Exception as e:
            print(f"FAIL {py_file.relative_to(package_path)}: {e}")

    print("-" * 80)
    print(f"Fixed {fixed_count} files")

    return fixed_count

if __name__ == "__main__":
    script_dir = pathlib.Path(__file__).parent
    package_path = script_dir.parent.parent / "runtime" / "target" / "wasm32-wasi" / "wasi-deps" / "usr" / "local" / "lib" / "python3.12" / "site-packages" / "remotemedia"

    if not package_path.exists():
        print(f"Error: package path does not exist: {package_path}", file=sys.stderr)
        sys.exit(1)

    fixed = fix_type_checking_blocks(package_path)
    print(f"\nFixed TYPE_CHECKING blocks in {fixed} files")
