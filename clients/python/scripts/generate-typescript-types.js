#!/usr/bin/env node

/**
 * TypeScript Type Generator for RemoteMedia Processing SDK
 * 
 * This script connects to the gRPC service, fetches all registered nodes,
 * and generates TypeScript definition files natively.
 */

const grpc = require('@grpc/grpc-js');
const protoLoader = require('@grpc/proto-loader');
const fs = require('fs').promises;
const path = require('path');

// Configuration
const GRPC_HOST = process.env.GRPC_HOST || 'localhost';
const GRPC_PORT = process.env.GRPC_PORT || 50052;
const OUTPUT_DIR = process.env.OUTPUT_DIR || './generated-types';

// Proto file paths - adjust these to your actual proto file locations
const PROTO_PATH = path.join(__dirname, '../remote_service/protos/execution.proto');

class TypeScriptGenerator {
  constructor() {
    this.client = null;
    this.nodeDefinitions = null;
  }

  async initialize() {
    try {
      // Load proto definitions
      const packageDefinition = protoLoader.loadSync(PROTO_PATH, {
        keepCase: true,
        longs: String,
        enums: String,
        defaults: true,
        oneofs: true
      });

      const proto = grpc.loadPackageDefinition(packageDefinition);

      // Create gRPC client
      this.client = new proto.remotemedia.execution.RemoteExecutionService(
        `${GRPC_HOST}:${GRPC_PORT}`,
        grpc.credentials.createInsecure()
      );

      console.log(`Connected to gRPC service at ${GRPC_HOST}:${GRPC_PORT}`);
    } catch (error) {
      console.error('Failed to initialize gRPC client:', error);
      throw error;
    }
  }

  async fetchNodeDefinitions() {
    return new Promise((resolve, reject) => {
      this.client.ExportTypeScriptDefinitions({}, (error, response) => {
        if (error) {
          reject(error);
          return;
        }

        if (response.status !== 'EXECUTION_STATUS_SUCCESS') {
          reject(new Error(`Server error: ${response.error_message}`));
          return;
        }

        try {
          // The server now returns JSON instead of TypeScript strings
          this.nodeDefinitions = JSON.parse(response.typescript_definitions);
          console.log(`Fetched ${this.nodeDefinitions.nodes.length} node definitions`);
          resolve(this.nodeDefinitions);
        } catch (parseError) {
          reject(new Error(`Failed to parse node definitions: ${parseError.message}`));
        }
      });
    });
  }

  async generateTypes() {
    await fs.mkdir(OUTPUT_DIR, { recursive: true });

    // Generate base types
    await this.generateBaseTypes();

    // Generate node-specific types
    await this.generateNodeTypes();

    // Generate index file
    await this.generateIndexFile();

    console.log(`TypeScript definitions generated in ${OUTPUT_DIR}`);
  }

  async generateBaseTypes() {
    const baseTypes = `/**
 * Base TypeScript interfaces for RemoteMedia Processing SDK
 * Generated at: ${this.nodeDefinitions.generated_at}
 * Service version: ${this.nodeDefinitions.service_version}
 */

export interface RemoteMediaNode {
  name?: string;
  config?: Record<string, any>;
  process(data: any): any | Promise<any>;
  initialize?(): Promise<void>;
  cleanup?(): Promise<void>;
  flush?(): any | Promise<any>;
}

export interface RemoteExecutorConfig {
  host: string;
  port: number;
  protocol?: 'grpc' | 'http';
  authToken?: string;
  timeout?: number;
  maxRetries?: number;
  sslEnabled?: boolean;
  pipPackages?: string[];
}

export interface ExecutionOptions {
  timeout?: number;
  maxMemoryMb?: number;
  cpuLimit?: number;
  enableGpu?: boolean;
  priority?: 'low' | 'normal' | 'high';
}

export interface ExecutionResponse<T = any> {
  status: 'success' | 'error';
  data?: T;
  error?: {
    message: string;
    traceback?: string;
  };
  metrics?: {
    startTimestamp: number;
    endTimestamp: number;
    durationMs: number;
    memoryPeakMb?: number;
    cpuTimeMs?: number;
  };
}

export interface StreamHandle {
  send(data: any): Promise<void>;
  close(): Promise<void>;
  readonly sessionId: string;
}

export interface NodeInfo {
  node_type: string;
  category: string;
  description: string;
  parameters: NodeParameter[];
}

export interface NodeParameter {
  name: string;
  type: string;
  required: boolean;
  default_value?: any;
  description?: string;
}

export type SerializationFormat = 'json' | 'pickle';
`;

    await fs.writeFile(path.join(OUTPUT_DIR, 'base.ts'), baseTypes);
  }

  async generateNodeTypes() {
    const nodes = this.nodeDefinitions.nodes;

    // Group nodes by category
    const nodesByCategory = {};
    nodes.forEach(node => {
      const category = node.category || 'base';
      if (!nodesByCategory[category]) {
        nodesByCategory[category] = [];
      }
      nodesByCategory[category].push(node);
    });

    // Generate NodeType enum
    const nodeTypeEnum = this.generateNodeTypeEnum(nodesByCategory);
    await fs.writeFile(path.join(OUTPUT_DIR, 'node-types.ts'), nodeTypeEnum);

    // Generate unified interfaces for each node (TypedDict + main interface)
    for (const node of nodes) {
      const nodeInterfaces = this.generateTypedDictInterfaces(node);
      const filename = `${this.kebabCase(node.node_type)}.ts`;
      await fs.writeFile(path.join(OUTPUT_DIR, filename), nodeInterfaces);
    }

    // Generate unified config types
    const configTypes = this.generateConfigTypes(nodes);
    await fs.writeFile(path.join(OUTPUT_DIR, 'config-types.ts'), configTypes);

    // Generate client interface
    const clientInterface = this.generateClientInterface();
    await fs.writeFile(path.join(OUTPUT_DIR, 'client.ts'), clientInterface);
  }

  generateNodeTypeEnum(nodesByCategory) {
    let enumContent = `/**
 * All available node types
 */
export enum NodeType {
`;

    Object.entries(nodesByCategory).forEach(([category, nodes], categoryIndex) => {
      enumContent += `  // ${this.capitalize(category)} nodes\n`;

      nodes.forEach((node, nodeIndex) => {
        const isLast = categoryIndex === Object.keys(nodesByCategory).length - 1 &&
          nodeIndex === nodes.length - 1;
        const comma = isLast ? '' : ',';
        enumContent += `  ${node.node_type} = '${node.node_type}'${comma}\n`;
      });

      if (categoryIndex < Object.keys(nodesByCategory).length - 1) {
        enumContent += '\n';
      }
    });

    enumContent += '}\n';
    return enumContent;
  }



  generateTypedDictInterfaces(node) {
    const { node_type, types = [] } = node;

    let content = `/**
 * TypeScript interfaces for ${node_type}
 * Auto-generated from Python TypedDict classes
 */

`;

    types.forEach(typedDict => {
      const { name, description, fields = [] } = typedDict;

      // Generate interface with node-specific prefix to avoid naming conflicts
      const uniqueName = name.startsWith(node_type) ? name : `${node_type}${name}`;

      content += `/**
 * ${description}
 */
export interface ${uniqueName} {
`;

      if (fields.length === 0) {
        content += '  // No fields defined\n';
      } else {
        fields.forEach(field => {
          const { name: fieldName, type, required = true } = field;
          const tsType = this.pythonToTypeScriptType(type);
          const optional = required ? '' : '?';

          content += `  ${fieldName}${optional}: ${tsType};\n`;
        });
      }

      content += '}\n\n';
    });

    // Add main node interface
    content += `
/**
 * ${node_type} Interface
 * 
 * ${node.description || `Interface for ${node_type} node`}
 */
export interface ${node_type} {
  // Configuration properties (constructor arguments)`;

    // Add constructor parameters
    if (node.parameters && node.parameters.length > 0) {
      node.parameters.forEach(param => {
        const { name, type, required = true, description = '', default_value } = param;
        const tsType = this.pythonToTypeScriptType(type);
        const optional = required ? '' : '?';

        if (description) {
          content += `\n  /** ${description}`;
          if (default_value !== undefined && !required) {
            content += ` (default: ${JSON.stringify(default_value)})`;
          }
          content += ' */';
        }

        content += `\n  ${name}${optional}: ${tsType};`;
      });
    } else {
      content += `\n  args?: any;`;
    }

    // Add common methods that all nodes have
    content += `

  // Available methods
  /** Clean up resources used by the node. */
  cleanup(): null;
  /** Extract session ID from input data. */
  extract_session_id(data: any): string | null;
  /** Get the node configuration. */
  get_config(): Record<string, any>;
  /** Get the current session ID. */
  get_session_id(): string | null;
  /** Get the session state for the given session ID. */
  get_session_state(session_id?: string | null): any | null;`;

    // Add node-specific methods based on available types
    const inputType = types.find(t => t.name.includes('Input'));
    const outputType = types.find(t => t.name.includes('Output'));
    const errorType = types.find(t => t.name.includes('Error'));

    if (inputType && outputType) {
      const inputTypeName = inputType.name.startsWith(node_type) ? inputType.name : `${node_type}${inputType.name}`;
      const outputTypeName = outputType.name.startsWith(node_type) ? outputType.name : `${node_type}${outputType.name}`;
      const errorTypeName = errorType ? (errorType.name.startsWith(node_type) ? errorType.name : `${node_type}${errorType.name}`) : 'any';

      content += `
  /** Initialize the node before processing. */
  initialize(): null;
  /** Merge processed data with metadata. */
  merge_data_metadata(data: any, metadata: Record<string, any> | null): any;
  /** Process input data through this node. */
  process(data: ${inputTypeName} | any): ${outputTypeName} | ${errorTypeName};
  /** Set the current session ID for state management. */
  set_session_id(session_id: string): null;
  /** Split data into content and metadata components. */
  split_data_metadata(data: any): any | any;`;
    } else {
      content += `
  /** Initialize the node before processing. */
  initialize(): null;
  /** Merge processed data with metadata. */
  merge_data_metadata(data: any, metadata: Record<string, any> | null): any;
  /** Process input data through this node. */
  process(data: any): any;
  /** Set the current session ID for state management. */
  set_session_id(session_id: string): null;
  /** Split data into content and metadata components. */
  split_data_metadata(data: any): any | any;`;
    }

    content += '\n}\n';

    return content;
  }

  generateConfigTypes(nodes) {
    let content = `import { NodeType } from './node-types';\n`;

    // Import all node interfaces
    nodes.forEach(node => {
      const filename = this.kebabCase(node.node_type);
      content += `import { ${node.node_type} } from './${filename}';\n`;
    });

    content += '\n';

    // Generate union type of all node interfaces
    content += '/**\n * Union type of all node interfaces\n */\nexport type Node = \n';
    nodes.forEach((node, index) => {
      const pipe = index === 0 ? '  ' : '  | ';
      content += `${pipe}${node.node_type}\n`;
    });
    content += ';\n\n';

    // Generate mapping interface
    content += '/**\n * Maps NodeType to its complete interface\n * \n * Use this for type-safe node operations:\n * const node: NodeMap[NodeType.CalculatorNode] = { name: "calc", process: (data) => result };\n */\nexport interface NodeMap {\n';
    nodes.forEach(node => {
      content += `  [NodeType.${node.node_type}]: ${node.node_type};\n`;
    });
    content += '}\n\n';

    // Backward compatibility aliases
    content += '// Backward compatibility aliases\nexport type NodeConfig = Node;\nexport type NodeConfigMap = NodeMap;\n';

    return content;
  }

  generateClientInterface() {
    return `import { ExecutionResponse, ExecutionOptions, StreamHandle, NodeInfo } from './base';
import { NodeMap } from './config-types';
import { NodeType } from './node-types';

/**
 * RemoteMedia Processing Client Interface
 */
export interface RemoteExecutionClient {
  /**
   * Execute a node with type-safe configuration
   * 
   * @param nodeType - The type of node to instantiate and execute
   * @param config - Configuration object with constructor args for the node
   * @param inputData - Data to process with the node
   * @param options - Execution options
   */
  executeNode<T extends NodeType>(
    nodeType: T,
    config: Partial<NodeMap[T]>,
    inputData: any,
    options?: ExecutionOptions
  ): Promise<ExecutionResponse>;

  /**
   * List all available nodes
   */
  listAvailableNodes(): Promise<NodeInfo[]>;

  /**
   * Stream data through a node
   * 
   * @param nodeType - The type of node to instantiate and use for streaming
   * @param config - Configuration object with constructor args for the node
   * @param onData - Callback for processed data
   * @param onError - Callback for errors
   */
  streamNode<T extends NodeType>(
    nodeType: T,
    config: Partial<NodeMap[T]>,
    onData: (data: any) => void,
    onError?: (error: Error) => void
  ): StreamHandle;

  /**
   * Close the client connection
   */
  close(): Promise<void>;
}
`;
  }

  async generateIndexFile() {
    const nodes = this.nodeDefinitions.nodes;

    let content = `/**
 * RemoteMedia Processing SDK TypeScript Definitions
 * Generated at: ${this.nodeDefinitions.generated_at}
 * Service version: ${this.nodeDefinitions.service_version}
 */

// Base interfaces
export * from './base';

// Node types and configurations
export * from './node-types';
export * from './config-types';

// Individual node interfaces
`;

    nodes.forEach(node => {
      const filename = this.kebabCase(node.node_type);
      content += `export * from './${filename}';\n`;
    });

    content += `
// Client interface
export * from './client';
`;

    await fs.writeFile(path.join(OUTPUT_DIR, 'index.ts'), content);
  }

  // Utility methods
  pythonToTypeScriptType(pythonType) {
    const typeMap = {
      'str': 'string',
      'string': 'string',
      'int': 'number',
      'number': 'number',
      'float': 'number',
      'bool': 'boolean',
      'boolean': 'boolean',
      'null': 'null',
      'list': 'any[]',
      'List': 'any[]',
      'Array<any>': 'any[]',
      'dict': 'Record<string, any>',
      'Dict': 'Record<string, any>',
      'Record<string, any>': 'Record<string, any>',
      'Any': 'any',
      'any': 'any',
      'Optional': 'any',
      'Union': 'any',
      'timedelta': 'any',
      'datetime': 'Date',
      'date': 'Date',
      'time': 'string',
      'Callable': 'Function',
      'callable': 'Function',
      'ndarray': 'Float32Array | number[]',
      'numpy.ndarray': 'Float32Array | number[]',
      'torch.Tensor': 'any',
      'Tensor': 'any',
      'Path': 'string',
      'pathlib.Path': 'string',
      'bytes': 'ArrayBuffer',
      'bytearray': 'ArrayBuffer'
    };

    // Handle Array types with specific inner types
    if (pythonType.startsWith('Array<') && pythonType.endsWith('>')) {
      const innerType = pythonType.slice(6, -1);
      const mappedInnerType = this.pythonToTypeScriptType(innerType);
      return `Array<${mappedInnerType}>`;
    }

    // Handle general Array types that include Array< anywhere
    if (pythonType.includes('Array<')) {
      // Replace any Array<...> patterns recursively
      return pythonType.replace(/Array<([^>]+)>/g, (match, innerType) => {
        const mappedInnerType = this.pythonToTypeScriptType(innerType);
        return `Array<${mappedInnerType}>`;
      });
    }

    // Handle tuple types (e.g., "[int, int]" -> "[number, number]")
    if (pythonType.startsWith('[') && pythonType.endsWith(']')) {
      const innerTypes = pythonType.slice(1, -1).split(',').map(t => t.trim());
      const mappedTypes = innerTypes.map(t => this.pythonToTypeScriptType(t));
      return `[${mappedTypes.join(', ')}]`;
    }

    // Handle quoted string literals (e.g., '"add"' -> '"add"')
    if (pythonType.startsWith('"') && pythonType.endsWith('"')) {
      return pythonType; // Keep as literal type
    }

    // Handle Record<str, any> pattern
    if (pythonType.startsWith('Record<str,') || pythonType.startsWith('Record<str, ')) {
      return pythonType.replace('Record<str,', 'Record<string,').replace('Record<str, ', 'Record<string, ');
    }

    // Check if the type exists in our mapping first (before handling complex types)
    if (typeMap[pythonType]) {
      return typeMap[pythonType];
    }



    // Handle union types (e.g., "string | null")
    if (pythonType.includes(' | ')) {
      const types = pythonType.split(' | ').map(t => t.trim());
      const mappedTypes = types.map(t => this.pythonToTypeScriptType(t));
      return mappedTypes.join(' | ');
    }

    // Handle TypedDict references
    if (pythonType.includes('TypedDict<')) {
      // Extract the TypedDict name
      const match = pythonType.match(/TypedDict<(.+)>/);
      if (match) {
        return match[1]; // Return just the type name
      }
    }

    // Fallback system: If we don't recognize the type, return 'any'
    // Log a warning so we can track unmapped types  
    console.warn(`Unknown Python type "${pythonType}" mapped to 'any'`);
    return 'any';
  }

  kebabCase(str) {
    return str.replace(/([a-z])([A-Z])/g, '$1-$2').toLowerCase();
  }

  capitalize(str) {
    return str.charAt(0).toUpperCase() + str.slice(1);
  }

  async cleanup() {
    if (this.client) {
      this.client.close();
    }
  }
}

// Main execution
async function main() {
  const generator = new TypeScriptGenerator();

  try {
    await generator.initialize();
    await generator.fetchNodeDefinitions();
    await generator.generateTypes();

    console.log('✅ TypeScript definitions generated successfully!');
    console.log(`📁 Output directory: ${OUTPUT_DIR}`);
    console.log('🔧 Import with: import { NodeType, RemoteExecutionClient } from "./generated-types"');

  } catch (error) {
    console.error('❌ Error generating TypeScript definitions:', error);
    process.exit(1);
  } finally {
    await generator.cleanup();
  }
}

if (require.main === module) {
  main();
}

module.exports = { TypeScriptGenerator };