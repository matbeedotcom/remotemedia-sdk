import { NodeType } from './node-types';
import { PassThroughNode } from './pass-through-node';
import { BufferNode } from './buffer-node';
import { AudioTransform } from './audio-transform';
import { AudioBuffer } from './audio-buffer';
import { AudioResampler } from './audio-resampler';
import { VideoTransform } from './video-transform';
import { VideoBuffer } from './video-buffer';
import { VideoResizer } from './video-resizer';
import { DataTransform } from './data-transform';
import { FormatConverter } from './format-converter';
import { CalculatorNode } from './calculator-node';
import { CodeExecutorNode } from './code-executor-node';
import { TextProcessorNode } from './text-processor-node';
import { SerializedClassExecutorNode } from './serialized-class-executor-node';
import { TransformersPipelineNode } from './transformers-pipeline-node';
/**
 * Union type of all node interfaces
 */
export type Node = PassThroughNode | BufferNode | AudioTransform | AudioBuffer | AudioResampler | VideoTransform | VideoBuffer | VideoResizer | DataTransform | FormatConverter | CalculatorNode | CodeExecutorNode | TextProcessorNode | SerializedClassExecutorNode | TransformersPipelineNode;
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
export type NodeConfig = Node;
export type NodeConfigMap = NodeMap;
//# sourceMappingURL=config-types.d.ts.map