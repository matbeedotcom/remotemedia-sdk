/**
 * package-loader.ts
 *
 * Handles loading and validation of .rmpkg package files in the browser.
 */

import JSZip from 'jszip';

export interface Manifest {
  version: string;
  metadata?: {
    name?: string;
    description?: string;
    author?: string;
    version?: string;
    license?: string;
  };
  runtime?: {
    target: string;
    min_version?: string;
    features?: string[];
  };
  nodes: Array<{
    id: string;
    node_type: string;
    params: Record<string, unknown>;
  }>;
  connections: Array<{
    from: string;
    to: string;
  }>;
}

export interface PackageMetadata {
  package_name: string;
  package_version: string;
  author?: string;
  license?: string;
  repository?: string;
  tags?: string[];
  created_at?: string;
  runtime_info?: {
    wasm_size_bytes: number;
    has_python_nodes: boolean;
    has_rust_nodes: boolean;
    node_types: string[];
    node_count: number;
    connection_count: number;
  };
  performance_hints?: {
    expected_load_time_ms: number;
    expected_execution_time_ms: number;
    memory_requirement_mb: number;
  };
}

export interface RemoteMediaPackage {
  manifest: Manifest;
  wasmBinary: ArrayBuffer;
  metadata: PackageMetadata | null;
}

export interface ValidationResult {
  valid: boolean;
  errors: string[];
  warnings: string[];
}

export class PackageLoader {
  private static readonly MAX_PACKAGE_SIZE = 100 * 1024 * 1024; // 100 MB
  private static readonly SUPPORTED_RUNTIME_TARGETS = ['wasm32-wasi'];
  private static readonly REQUIRED_FILES = ['manifest.json', 'runtime.wasm'];

  /**
   * Load a .rmpkg package from a File object
   */
  static async loadFromFile(file: File): Promise<RemoteMediaPackage> {
    // Check file size
    if (file.size > this.MAX_PACKAGE_SIZE) {
      throw new Error(
        `Package too large: ${this.formatBytes(file.size)} (max ${this.formatBytes(this.MAX_PACKAGE_SIZE)})`
      );
    }

    // Check file extension
    if (!file.name.endsWith('.rmpkg')) {
      throw new Error(
        `Invalid package file: expected .rmpkg extension, got ${file.name}`
      );
    }

    // Load ZIP archive
    const zip = new JSZip();
    let archive: JSZip;
    try {
      archive = await zip.loadAsync(file);
    } catch (err) {
      throw new Error(`Failed to extract package: ${err instanceof Error ? err.message : String(err)}`);
    }

    // Validate required files
    const files = Object.keys(archive.files);
    const missingFiles = this.REQUIRED_FILES.filter(f => !files.includes(f));
    if (missingFiles.length > 0) {
      throw new Error(
        `Missing required files: ${missingFiles.join(', ')}`
      );
    }

    // Extract manifest.json
    const manifestFile = archive.files['manifest.json'];
    if (!manifestFile) {
      throw new Error('manifest.json not found in package');
    }

    let manifest: Manifest;
    try {
      const manifestText = await manifestFile.async('text');
      manifest = JSON.parse(manifestText);
    } catch (err) {
      throw new Error(`Invalid manifest.json: ${err instanceof Error ? err.message : String(err)}`);
    }

    // Validate manifest
    const validation = this.validateManifest(manifest);
    if (!validation.valid) {
      throw new Error(
        `Invalid manifest:\n${validation.errors.join('\n')}`
      );
    }

    // Show warnings if any
    if (validation.warnings.length > 0) {
      console.warn('Package validation warnings:');
      validation.warnings.forEach(w => console.warn(`  - ${w}`));
    }

    // Extract runtime.wasm
    const wasmFile = archive.files['runtime.wasm'];
    if (!wasmFile) {
      throw new Error('runtime.wasm not found in package');
    }

    const wasmBinary = await wasmFile.async('arraybuffer');

    // Extract metadata.json (optional)
    let metadata: PackageMetadata | null = null;
    const metadataFile = archive.files['metadata.json'];
    if (metadataFile) {
      try {
        const metadataText = await metadataFile.async('text');
        metadata = JSON.parse(metadataText);
      } catch (err) {
        console.warn('Failed to parse metadata.json:', err);
      }
    }

    return {
      manifest,
      wasmBinary,
      metadata
    };
  }

  /**
   * Validate a manifest structure
   */
  static validateManifest(manifest: Manifest): ValidationResult {
    const errors: string[] = [];
    const warnings: string[] = [];

    // Check version
    if (!manifest.version) {
      errors.push('Missing required field: version');
    } else if (manifest.version !== 'v1') {
      warnings.push(`Unsupported manifest version: ${manifest.version} (expected v1)`);
    }

    // Check nodes
    if (!manifest.nodes || !Array.isArray(manifest.nodes)) {
      errors.push('Missing or invalid nodes array');
    } else if (manifest.nodes.length === 0) {
      warnings.push('Pipeline has no nodes');
    }

    // Check connections
    if (!manifest.connections || !Array.isArray(manifest.connections)) {
      errors.push('Missing or invalid connections array');
    }

    // Check runtime configuration
    if (!manifest.runtime) {
      warnings.push('Missing runtime configuration. Assuming wasm32-wasi target.');
    } else {
      if (!manifest.runtime.target) {
        errors.push('Missing required field: runtime.target');
      } else if (!this.SUPPORTED_RUNTIME_TARGETS.includes(manifest.runtime.target)) {
        errors.push(
          `Unsupported runtime target: ${manifest.runtime.target} ` +
          `(supported: ${this.SUPPORTED_RUNTIME_TARGETS.join(', ')})`
        );
      }
    }

    // Validate nodes
    if (manifest.nodes) {
      manifest.nodes.forEach((node, i) => {
        if (!node.id) {
          errors.push(`Node ${i}: missing required field: id`);
        }
        if (!node.node_type) {
          errors.push(`Node ${i}: missing required field: node_type`);
        }
        if (!node.params) {
          warnings.push(`Node ${node.id || i}: missing params (using empty object)`);
        }
      });
    }

    // Validate connections
    if (manifest.nodes && manifest.connections) {
      const nodeIds = new Set(manifest.nodes.map(n => n.id));
      manifest.connections.forEach((conn, i) => {
        if (!conn.from) {
          errors.push(`Connection ${i}: missing required field: from`);
        } else if (!nodeIds.has(conn.from)) {
          errors.push(`Connection ${i}: unknown node id: ${conn.from}`);
        }
        if (!conn.to) {
          errors.push(`Connection ${i}: missing required field: to`);
        } else if (!nodeIds.has(conn.to)) {
          errors.push(`Connection ${i}: unknown node id: ${conn.to}`);
        }
      });
    }

    return {
      valid: errors.length === 0,
      errors,
      warnings
    };
  }

  /**
   * Get package info summary for display
   */
  static getPackageInfo(pkg: RemoteMediaPackage): string {
    const lines: string[] = [];

    lines.push(`Package: ${pkg.metadata?.package_name || pkg.manifest.metadata?.name || 'Unnamed'}`);

    if (pkg.metadata?.package_version || pkg.manifest.metadata?.version) {
      lines.push(`Version: ${pkg.metadata?.package_version || pkg.manifest.metadata?.version}`);
    }

    if (pkg.manifest.metadata?.description) {
      lines.push(`Description: ${pkg.manifest.metadata.description}`);
    }

    if (pkg.metadata?.author || pkg.manifest.metadata?.author) {
      lines.push(`Author: ${pkg.metadata?.author || pkg.manifest.metadata?.author}`);
    }

    lines.push(`Nodes: ${pkg.manifest.nodes.length}`);
    lines.push(`Connections: ${pkg.manifest.connections?.length || 0}`);
    lines.push(`Runtime: ${pkg.manifest.runtime?.target || 'wasm32-wasi (default)'}`);
    lines.push(`WASM Size: ${this.formatBytes(pkg.wasmBinary.byteLength)}`);

    if (pkg.metadata?.runtime_info?.node_types) {
      lines.push(`Node Types: ${pkg.metadata.runtime_info.node_types.join(', ')}`);
    }

    if (pkg.metadata?.performance_hints) {
      const hints = pkg.metadata.performance_hints;
      lines.push(`Est. Load Time: ${hints.expected_load_time_ms}ms`);
      lines.push(`Est. Execution Time: ${hints.expected_execution_time_ms}ms`);
      lines.push(`Memory Required: ${hints.memory_requirement_mb}MB`);
    }

    return lines.join('\n');
  }

  /**
   * Format bytes to human-readable string
   */
  private static formatBytes(bytes: number): string {
    if (bytes < 1024) return `${bytes} B`;
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(2)} KB`;
    return `${(bytes / (1024 * 1024)).toFixed(2)} MB`;
  }
}
