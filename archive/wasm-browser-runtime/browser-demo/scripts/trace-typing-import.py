#!/usr/bin/env python3
"""
Trace which module imports typing by hooking into the import system.
"""

import sys
import pathlib

# Add the stdlib and remotemedia to path
stdlib_path = pathlib.Path(__file__).parent.parent.parent / "runtime" / "target" / "wasm32-wasi" / "wasi-deps" / "usr" / "local" / "lib" / "python3.12"
sys.path.insert(0, str(stdlib_path / "site-packages"))
sys.path.insert(0, str(stdlib_path))

import_stack = []

class ImportTracer:
    def find_module(self, fullname, path=None):
        if 'typing' in fullname:
            print(f"\n{'  ' * len(import_stack)}Import stack when typing was imported:")
            for i, mod in enumerate(import_stack):
                print(f"{'  ' * i}  -> {mod}")
            print(f"{'  ' * len(import_stack)}  -> {fullname} (TYPING!)")
        return None  # Let default importer handle it

sys.meta_path.insert(0, ImportTracer())

# Now try importing remotemedia core components
print("Importing remotemedia.core.node...")
import_stack.append("remotemedia.core.node")
try:
    import remotemedia.core.node
except Exception as e:
    print(f"Error: {e}")
finally:
    import_stack.pop()

print("\n" + "=" * 80)
print("Import complete. Check if typing was imported:")
print("  'typing' in sys.modules:", 'typing' in sys.modules)
