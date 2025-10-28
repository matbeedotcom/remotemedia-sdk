import { NodeType } from './node-types.js';
import { PassThroughNode } from './pass-through-node.js';
import { BufferNode } from './buffer-node.js';
import { AudioTransform } from './audio-transform.js';
import { AudioBuffer } from './audio-buffer.js';
import { AudioResampler } from './audio-resampler.js';
import { VideoTransform } from './video-transform.js';
import { VideoBuffer } from './video-buffer.js';
import { VideoResizer } from './video-resizer.js';
import { DataTransform } from './data-transform.js';
import { FormatConverter } from './format-converter.js';
import { CalculatorNode } from './calculator-node.js';
import { CodeExecutorNode } from './code-executor-node.js';
import { TextProcessorNode } from './text-processor-node.js';
import { SerializedClassExecutorNode } from './serialized-class-executor-node.js';
import { TransformersPipelineNode } from './transformers-pipeline-node.js';

/**
 * Union type of all node interfaces
 */
export type Node = 
  PassThroughNode
  | BufferNode
  | AudioTransform
  | AudioBuffer
  | AudioResampler
  | VideoTransform
  | VideoBuffer
  | VideoResizer
  | DataTransform
  | FormatConverter
  | CalculatorNode
  | CodeExecutorNode
  | TextProcessorNode
  | SerializedClassExecutorNode
  | TransformersPipelineNode
;

/**
 * Maps NodeType to its complete interface
 * 
 * Use this for type-safe node operations:
 * const node: NodeMap[NodeType.CalculatorNode] = { name: "calc", process: (data) => result };
 */
export interface NodeMap {
  [NodeType.PassThroughNode]: PassThroughNode;
  [NodeType.BufferNode]: BufferNode;
  [NodeType.AudioTransform]: AudioTransform;
  [NodeType.AudioBuffer]: AudioBuffer;
  [NodeType.AudioResampler]: AudioResampler;
  [NodeType.VideoTransform]: VideoTransform;
  [NodeType.VideoBuffer]: VideoBuffer;
  [NodeType.VideoResizer]: VideoResizer;
  [NodeType.DataTransform]: DataTransform;
  [NodeType.FormatConverter]: FormatConverter;
  [NodeType.CalculatorNode]: CalculatorNode;
  [NodeType.CodeExecutorNode]: CodeExecutorNode;
  [NodeType.TextProcessorNode]: TextProcessorNode;
  [NodeType.SerializedClassExecutorNode]: SerializedClassExecutorNode;
  [NodeType.TransformersPipelineNode]: TransformersPipelineNode;
}

// Backward compatibility aliases
export type NodeConfig = Node;
export type NodeConfigMap = NodeMap;
