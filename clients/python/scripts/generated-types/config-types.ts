import { NodeType } from './node-types';
import { PassThroughNodeConfig } from './pass-through-node-config';
import { BufferNodeConfig } from './buffer-node-config';
import { AudioTransformConfig } from './audio-transform-config';
import { AudioBufferConfig } from './audio-buffer-config';
import { AudioResamplerConfig } from './audio-resampler-config';
import { VideoTransformConfig } from './video-transform-config';
import { VideoBufferConfig } from './video-buffer-config';
import { VideoResizerConfig } from './video-resizer-config';
import { DataTransformConfig } from './data-transform-config';
import { FormatConverterConfig } from './format-converter-config';
import { CalculatorNodeConfig } from './calculator-node-config';
import { CodeExecutorNodeConfig } from './code-executor-node-config';
import { TextProcessorNodeConfig } from './text-processor-node-config';
import { SerializedClassExecutorNodeConfig } from './serialized-class-executor-node-config';
import { TransformersPipelineNodeConfig } from './transformers-pipeline-node-config';

export type NodeConfig = 
  PassThroughNodeConfig
  | BufferNodeConfig
  | AudioTransformConfig
  | AudioBufferConfig
  | AudioResamplerConfig
  | VideoTransformConfig
  | VideoBufferConfig
  | VideoResizerConfig
  | DataTransformConfig
  | FormatConverterConfig
  | CalculatorNodeConfig
  | CodeExecutorNodeConfig
  | TextProcessorNodeConfig
  | SerializedClassExecutorNodeConfig
  | TransformersPipelineNodeConfig
;

export interface NodeConfigMap {
  [NodeType.PassThroughNode]: PassThroughNodeConfig;
  [NodeType.BufferNode]: BufferNodeConfig;
  [NodeType.AudioTransform]: AudioTransformConfig;
  [NodeType.AudioBuffer]: AudioBufferConfig;
  [NodeType.AudioResampler]: AudioResamplerConfig;
  [NodeType.VideoTransform]: VideoTransformConfig;
  [NodeType.VideoBuffer]: VideoBufferConfig;
  [NodeType.VideoResizer]: VideoResizerConfig;
  [NodeType.DataTransform]: DataTransformConfig;
  [NodeType.FormatConverter]: FormatConverterConfig;
  [NodeType.CalculatorNode]: CalculatorNodeConfig;
  [NodeType.CodeExecutorNode]: CodeExecutorNodeConfig;
  [NodeType.TextProcessorNode]: TextProcessorNodeConfig;
  [NodeType.SerializedClassExecutorNode]: SerializedClassExecutorNodeConfig;
  [NodeType.TransformersPipelineNode]: TransformersPipelineNodeConfig;
}
