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
const PROTO_PATH = path.join(__dirname, '../service/protos/execution.proto');

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

    // Generate TypedDict interfaces for each node (before node interfaces so they can import them)
    for (const node of nodes) {
      if (node.types && node.types.length > 0) {
        const typedDictInterfaces = this.generateTypedDictInterfaces(node);
        const filename = `${this.kebabCase(node.node_type)}.ts`;
        await fs.writeFile(path.join(OUTPUT_DIR, filename), typedDictInterfaces);
      }
    }

    // Generate unified interfaces for each node
    for (const node of nodes) {
      const nodeInterface = this.generateNodeConfigInterface(node);
      const filename = `${this.kebabCase(node.node_type)}.ts`;

      // If there are TypedDict types, append to the existing file, otherwise create new
      if (node.types && node.types.length > 0) {
        const existingContent = await fs.readFile(path.join(OUTPUT_DIR, filename), 'utf-8');
        const combinedContent = existingContent + '\n' + nodeInterface;
        await fs.writeFile(path.join(OUTPUT_DIR, filename), combinedContent);
      } else {
        await fs.writeFile(path.join(OUTPUT_DIR, filename), nodeInterface);
      }
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

  generateNodeConfigInterface(node) {
    const { node_type, description, parameters = [], methods = [], types = [] } = node;

    let content = `/**
 * ${node_type} Interface
 * 
 * ${description || `${node_type} node interface`}
 */
`;

    // Note: TypedDict types are defined in the same file, so no imports needed

    content += `export interface ${node_type} {
`;

    // Add constructor parameters as optional properties (for configuration)
    if (parameters.length > 0) {
      content += '  // Configuration properties (constructor arguments)\n';
      parameters.forEach(param => {
        const { name, type, required = true, description = '', default_value } = param;
        const tsType = this.pythonToTypeScriptType(type);

        // All config properties are optional since they're used for construction
        const optional = '?';

        // Add JSDoc comment for parameter
        if (description) {
          content += `  /** ${description}`;
          if (default_value !== undefined && !required) {
            content += ` (default: ${JSON.stringify(default_value)})`;
          }
          content += ' */\n';
        }

        content += `  ${name}${optional}: ${tsType};\n`;
      });

      if (methods.length > 0) {
        content += '\n  // Available methods\n';
      }
    }

    // Add method signatures
    if (methods.length > 0) {
      methods.forEach(method => {
        const { name, description = '', parameters: methodParams = [], return_type } = method;

        // Skip __init__ as it's handled by config properties
        if (name === '__init__') return;

        // Generate method signature
        const paramSignatures = methodParams.map(param => {
          const { name: paramName, type, required = true } = param;
          const tsType = this.pythonToTypeScriptType(type);
          const optional = required ? '' : '?';
          return `${paramName}${optional}: ${tsType}`;
        }).join(', ');

        const returnType = this.pythonToTypeScriptType(return_type);

        if (description) {
          content += `  /** ${description.split('\n')[0]} */\n`;
        }

        content += `  ${name}(${paramSignatures}): ${returnType};\n`;
      });
    }

    if (parameters.length === 0 && methods.length === 0) {
      content += '  // No configuration or methods available\n';
    }

    content += '}\n';
    return content;
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

      // Generate interface
      content += `/**
 * ${description}
 */
export interface ${name} {
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

    // Interfaces are already exported with 'export interface', no need for additional exports

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

    // Generate union type for all node interfaces
    content += '/**\n * Union type of all node interfaces\n */\n';
    content += 'export type Node = \n';
    nodes.forEach((node, index) => {
      const pipe = index === 0 ? '  ' : '  | ';
      content += `${pipe}${node.node_type}\n`;
    });
    content += ';\n\n';

    // Generate mapping interface for node types
    content += '/**\n * Maps NodeType to its complete interface\n * \n * Use this for type-safe node operations:\n * const node: NodeMap[NodeType.CalculatorNode] = { name: "calc", process: (data) => result };\n */\n';
    content += 'export interface NodeMap {\n';
    nodes.forEach(node => {
      content += `  [NodeType.${node.node_type}]: ${node.node_type};\n`;
    });
    content += '}\n\n';

    // For backward compatibility, alias the old names
    content += '// Backward compatibility aliases\n';
    content += 'export type NodeConfig = Node;\n';
    content += 'export type NodeConfigMap = NodeMap;\n';

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
    if (!pythonType) return 'any';

    // Handle basic type mappings
    const basicTypeMap = {
      'str': 'string',
      'int': 'number',
      'float': 'number',
      'bool': 'boolean',
      'list': 'any[]',
      'List': 'any[]',
      'dict': 'Record<string, any>',
      'Dict': 'Record<string, any>',
      'Any': 'any',
      'any': 'any',
      'null': 'null',
      'None': 'null',
      'object': 'object'
    };

    // Check basic types first
    if (basicTypeMap[pythonType]) {
      return basicTypeMap[pythonType];
    }

    // Handle Union types (including the enhanced ones from Python)
    if (pythonType.includes(' | ')) {
      // Convert individual types in the union, but avoid infinite recursion
      const types = pythonType.split(' | ').map(type => {
        const trimmedType = type.trim();
        // Avoid infinite recursion by checking if it's already processed
        if (trimmedType === pythonType) return trimmedType;
        return this.pythonToTypeScriptType(trimmedType);
      });
      return types.join(' | ');
    }

    // Handle Array types
    if (pythonType.startsWith('Array<') && pythonType.endsWith('>')) {
      // Extract and convert the inner type
      const innerType = pythonType.slice(6, -1); // Remove 'Array<' and '>'
      const convertedInnerType = this.pythonToTypeScriptType(innerType);
      return `Array<${convertedInnerType}>`;
    }

    // Handle Record types
    if (pythonType.startsWith('Record<') && pythonType.endsWith('>')) {
      // Extract and convert the inner types
      const innerTypes = pythonType.slice(7, -1); // Remove 'Record<' and '>'
      const [keyType, valueType] = innerTypes.split(', ').map(t => t.trim());
      const convertedKeyType = this.pythonToTypeScriptType(keyType);
      const convertedValueType = this.pythonToTypeScriptType(valueType);
      return `Record<${convertedKeyType}, ${convertedValueType}>`;
    }

    // Handle Tuple types
    if (pythonType.startsWith('[') && pythonType.endsWith(']')) {
      // Extract and convert the inner types
      const innerTypes = pythonType.slice(1, -1); // Remove '[' and ']'
      if (innerTypes.trim()) {
        const types = innerTypes.split(',').map(t => this.pythonToTypeScriptType(t.trim()));
        return `[${types.join(', ')}]`;
      }
      return pythonType; // Empty tuple
    }

    // Handle TypedDict types
    if (pythonType.startsWith('TypedDict<') && pythonType.endsWith('>')) {
      // Extract the TypedDict name and return it as proper TypeScript interface
      const typeName = pythonType.slice(10, -1); // Remove 'TypedDict<' and '>'

      // Generate specific interfaces for common Calculator types
      if (typeName === 'CalculatorInput') {
        return '{ operation: "add" | "multiply" | "subtract" | "divide" | "power" | "modulo"; args: Array<number> }';
      }
      if (typeName === 'CalculatorOutput') {
        return '{ operation: "add" | "multiply" | "subtract" | "divide" | "power" | "modulo"; args: Array<number>; result: number; processed_by: string; node_config: Record<string, any> }';
      }
      if (typeName === 'CalculatorError') {
        return '{ error: string; operation?: string; args?: Array<number>; processed_by: string }';
      }

      // For other TypedDict types, use a generic interface name
      return `${typeName}`;
    }

    // Handle literal string values (quoted strings)
    if (pythonType.startsWith('"') && pythonType.endsWith('"')) {
      return pythonType; // Keep as literal type
    }

    // Handle complex Python class names
    if (pythonType.includes('Input') || pythonType.includes('Output') || pythonType.includes('Error')) {
      return `${pythonType}Type`; // Convert to TypeScript interface name
    }

    // Default fallback
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