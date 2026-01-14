"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.NodeType = void 0;
/**
 * All available node types
 */
var NodeType;
(function (NodeType) {
    // Base nodes
    NodeType["PassThroughNode"] = "PassThroughNode";
    NodeType["BufferNode"] = "BufferNode";
    // Audio nodes
    NodeType["AudioTransform"] = "AudioTransform";
    NodeType["AudioBuffer"] = "AudioBuffer";
    NodeType["AudioResampler"] = "AudioResampler";
    // Video nodes
    NodeType["VideoTransform"] = "VideoTransform";
    NodeType["VideoBuffer"] = "VideoBuffer";
    NodeType["VideoResizer"] = "VideoResizer";
    // Transform nodes
    NodeType["DataTransform"] = "DataTransform";
    NodeType["FormatConverter"] = "FormatConverter";
    NodeType["TransformersPipelineNode"] = "TransformersPipelineNode";
    // Math nodes
    NodeType["CalculatorNode"] = "CalculatorNode";
    // Execution nodes
    NodeType["CodeExecutorNode"] = "CodeExecutorNode";
    NodeType["SerializedClassExecutorNode"] = "SerializedClassExecutorNode";
    // Text nodes
    NodeType["TextProcessorNode"] = "TextProcessorNode";
})(NodeType || (exports.NodeType = NodeType = {}));
//# sourceMappingURL=node-types.js.map