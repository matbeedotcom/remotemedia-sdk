#!/usr/bin/env python3
"""
Disable logging imports in remotemedia for browser compatibility.
Replace with no-op logger to avoid stack overflow.
"""

import sys
import pathlib
import re

NOOP_LOGGER = '''
# WASM compatibility: logging module causes stack overflow in browser
# import logging

class NoOpLogger:
    """No-op logger for browser WASM compatibility."""
    def info(self, *args, **kwargs): pass
    def debug(self, *args, **kwargs): pass
    def error(self, *args, **kwargs): pass
    def warning(self, *args, **kwargs): pass
    def critical(self, *args, **kwargs): pass
    def exception(self, *args, **kwargs): pass
    def getLogger(self, name): return self

_noop_logger = NoOpLogger()
'''

def disable_logging(package_path: pathlib.Path):
    """
    Disable logging in all Python files.

    Args:
        package_path: Path to remotemedia package
    """
    print(f"Disabling logging in: {package_path}")
    print("-" * 80)

    py_files = list(package_path.rglob("*.py"))
    py_files = [f for f in py_files if not f.name.endswith('.bak')]

    print(f"Found {len(py_files)} Python files\n")

    modified_count = 0

    for py_file in sorted(py_files):
        try:
            with open(py_file, 'r', encoding='utf-8') as f:
                content = f.read()

            # Check if file imports logging
            if not re.search(r'^import logging$', content, re.MULTILINE):
                continue

            # Replace import logging with noop logger
            new_content = re.sub(
                r'^import logging$',
                NOOP_LOGGER.strip(),
                content,
                flags=re.MULTILINE
            )

            # Replace logger = logging.getLogger(...)
            new_content = re.sub(
                r'logger = logging\.getLogger\([^)]*\)',
                'logger = _noop_logger',
                new_content
            )

            # Replace logging.getLogger(...) calls
            new_content = re.sub(
                r'logging\.getLogger\([^)]*\)',
                '_noop_logger',
                new_content
            )

            if new_content != content:
                with open(py_file, 'w', encoding='utf-8') as f:
                    f.write(new_content)

                modified_count += 1
                print(f"OK {py_file.relative_to(package_path)}")

        except Exception as e:
            print(f"FAIL {py_file.relative_to(package_path)}: {e}")

    print("-" * 80)
    print(f"Modified {modified_count} files")

    return modified_count

if __name__ == "__main__":
    script_dir = pathlib.Path(__file__).parent
    package_path = script_dir.parent.parent / "runtime" / "target" / "wasm32-wasi" / "wasi-deps" / "usr" / "local" / "lib" / "python3.12" / "site-packages" / "remotemedia"

    if not package_path.exists():
        print(f"Error: package path does not exist: {package_path}", file=sys.stderr)
        sys.exit(1)

    modified = disable_logging(package_path)
    print(f"\nDisabled logging in {modified} files")
