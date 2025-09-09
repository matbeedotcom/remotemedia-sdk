/**
 * All available node types
 */
export enum NodeType {
  // Base nodes
  PassThroughNode = 'PassThroughNode',
  BufferNode = 'BufferNode',

  // Audio nodes
  AudioTransform = 'AudioTransform',
  AudioBuffer = 'AudioBuffer',
  AudioResampler = 'AudioResampler',

  // Video nodes
  VideoTransform = 'VideoTransform',
  VideoBuffer = 'VideoBuffer',
  VideoResizer = 'VideoResizer',

  // Transform nodes
  DataTransform = 'DataTransform',
  FormatConverter = 'FormatConverter',
  TransformersPipelineNode = 'TransformersPipelineNode',

  // Math nodes
  CalculatorNode = 'CalculatorNode',

  // Execution nodes
  CodeExecutorNode = 'CodeExecutorNode',
  SerializedClassExecutorNode = 'SerializedClassExecutorNode',

  // Text nodes
  TextProcessorNode = 'TextProcessorNode'
}
