#!/usr/bin/env python3
"""
Generate Python protobuf files from .proto sources in remotemedia-grpc transport
"""

import os
import sys
import subprocess
from pathlib import Path

def generate_protos():
    """Generate Python protobuf files"""
    
    # Get paths
    script_dir = Path(__file__).parent
    python_client_dir = script_dir.parent
    sdk_root = python_client_dir.parent
    proto_dir = sdk_root / "transports" / "remotemedia-grpc" / "protos"
    output_dir = python_client_dir / "remotemedia" / "protos"
    
    if not proto_dir.exists():
        print(f"ERROR: Proto directory not found: {proto_dir}")
        sys.exit(1)
    
    # Create output directory
    output_dir.mkdir(parents=True, exist_ok=True)
    
    # Create __init__.py
    init_file = output_dir / "__init__.py"
    init_file.write_text('"""Generated protobuf files"""\n')
    
    # Find all .proto files
    proto_files = list(proto_dir.glob("*.proto"))
    if not proto_files:
        print(f"ERROR: No .proto files found in {proto_dir}")
        sys.exit(1)
    
    print(f"Found {len(proto_files)} .proto files:")
    for proto_file in proto_files:
        print(f"  - {proto_file.name}")
    
    # Generate Python code using grpc_tools
    print(f"\nGenerating Python protobuf files...")
    print(f"  Proto source: {proto_dir}")
    print(f"  Output dir: {output_dir}")
    
    try:
        # Use grpc_tools.protoc to generate Python files
        from grpc_tools import protoc
        
        for proto_file in proto_files:
            print(f"\nGenerating {proto_file.name}...")
            
            # Build protoc command arguments
            args = [
                'grpc_tools.protoc',
                f'--proto_path={proto_dir}',
                f'--python_out={output_dir}',
                f'--grpc_python_out={output_dir}',
                str(proto_file),
            ]
            
            result = protoc.main(args)
            
            if result != 0:
                print(f"ERROR: Failed to generate {proto_file.name}")
                sys.exit(1)
            
            print(f"  ✓ Generated {proto_file.stem}_pb2.py")
            print(f"  ✓ Generated {proto_file.stem}_pb2_grpc.py")
    
    except ImportError:
        print("ERROR: grpcio-tools not installed. Install with: pip install grpcio-tools")
        sys.exit(1)
    
    print("\n✅ All protobuf files generated successfully!")
    print(f"\nGenerated files in: {output_dir}")
    
    # Fix imports in generated files (change absolute imports to relative)
    print("\nFixing imports in generated files...")
    generated_files = sorted(output_dir.glob("*_pb2*.py"))
    for f in generated_files:
        print(f"  - {f.name}")
        
        # Read the file
        content = f.read_text()
        
        # Fix imports: change "import common_pb2" to "from . import common_pb2"
        # and similarly for other pb2 imports
        import re
        
        # Pattern to match: "import xxx_pb2 as yyy__pb2"
        pattern = r'^import (\w+_pb2) as (\w+__pb2)$'
        replacement = r'from . import \1 as \2'
        
        lines = content.split('\n')
        fixed_lines = []
        for line in lines:
            # Only fix imports that are from our proto files
            if re.match(pattern, line):
                module_name = re.match(pattern, line).group(1)
                # Check if it's one of our generated files
                if (output_dir / f"{module_name}.py").exists():
                    line = re.sub(pattern, replacement, line)
            fixed_lines.append(line)
        
        # Write back
        f.write_text('\n'.join(fixed_lines))
    
    print("\n✅ Imports fixed successfully!")

if __name__ == "__main__":
    generate_protos()

