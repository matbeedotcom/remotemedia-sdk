use crate::audio::{AudioBuffer, AudioFormat};
use crate::audio::format::{i16_to_f32, i32_to_f32, f32_to_i16, f32_to_i32};
use crate::executor::node_executor::{NodeExecutor, NodeContext};
use crate::nodes::registry::NodeFactory;
use crate::error::{Error, Result};
use serde_json::Value;
use std::sync::Arc;

/// Rust-native audio format conversion node
pub struct RustFormatConverterNode {
    target_format: AudioFormat,
}

impl RustFormatConverterNode {
    pub fn new(target_format: AudioFormat) -> Self {
        Self { target_format }
    }

    fn convert_samples(&self, input_format: AudioFormat, samples: &[f32]) -> Result<Vec<f32>> {
        match (&input_format, &self.target_format) {
            // No conversion needed
            (AudioFormat::F32, AudioFormat::F32) => Ok(samples.to_vec()),
            (AudioFormat::I16, AudioFormat::I16) => Ok(samples.to_vec()),
            (AudioFormat::I32, AudioFormat::I32) => Ok(samples.to_vec()),
            
            // I16 -> F32
            (AudioFormat::I16, AudioFormat::F32) => {
                // Reinterpret f32 slice as i16 (zero-copy via bytemuck)
                let i16_samples = bytemuck::cast_slice::<f32, i16>(samples);
                Ok(i16_to_f32(i16_samples))
            },
            
            // I32 -> F32
            (AudioFormat::I32, AudioFormat::F32) => {
                // Reinterpret f32 slice as i32 (zero-copy via bytemuck)
                let i32_samples = bytemuck::cast_slice::<f32, i32>(samples);
                Ok(i32_to_f32(i32_samples))
            },
            
            // F32 -> I16 (then reinterpret as f32 for storage)
            (AudioFormat::F32, AudioFormat::I16) => {
                let i16_samples = f32_to_i16(samples);
                // Cast i16 back to f32 storage format (this is safe with bytemuck)
                let f32_storage = bytemuck::cast_slice::<i16, f32>(&i16_samples);
                Ok(f32_storage.to_vec())
            },
            
            // F32 -> I32 (then reinterpret as f32 for storage)
            (AudioFormat::F32, AudioFormat::I32) => {
                let i32_samples = f32_to_i32(samples);
                // Cast i32 back to f32 storage format
                let f32_storage = bytemuck::cast_slice::<i32, f32>(&i32_samples);
                Ok(f32_storage.to_vec())
            },
            
            // I16 -> I32
            (AudioFormat::I16, AudioFormat::I32) => {
                let i16_samples = bytemuck::cast_slice::<f32, i16>(samples);
                let f32_intermediate = i16_to_f32(i16_samples);
                let i32_samples = f32_to_i32(&f32_intermediate);
                let f32_storage = bytemuck::cast_slice::<i32, f32>(&i32_samples);
                Ok(f32_storage.to_vec())
            },
            
            // I32 -> I16
            (AudioFormat::I32, AudioFormat::I16) => {
                let i32_samples = bytemuck::cast_slice::<f32, i32>(samples);
                let f32_intermediate = i32_to_f32(i32_samples);
                let i16_samples = f32_to_i16(&f32_intermediate);
                let f32_storage = bytemuck::cast_slice::<i16, f32>(&i16_samples);
                Ok(f32_storage.to_vec())
            },
        }
    }
}

#[async_trait::async_trait]
impl NodeExecutor for RustFormatConverterNode {
    async fn initialize(&mut self, _context: &NodeContext) -> Result<()> {
        // No initialization needed
        Ok(())
    }

    async fn process(&mut self, input: Value) -> Result<Vec<Value>> {
        // Extract audio data from input
        let audio_data = input.get("data")
            .and_then(|d| d.as_array())
            .ok_or_else(|| Error::Execution("Missing audio data array".into()))?;

        let samples: Vec<f32> = audio_data.iter()
            .filter_map(|v| v.as_f64().map(|f| f as f32))
            .collect();

        let sample_rate = input.get("sample_rate")
            .and_then(|v| v.as_u64())
            .ok_or_else(|| Error::Execution("Missing sample_rate".into()))? as u32;

        let channels = input.get("channels")
            .and_then(|v| v.as_u64())
            .unwrap_or(1) as u16;

        let input_format_str = input.get("format")
            .and_then(|v| v.as_str())
            .unwrap_or("F32");
        
        let input_format = match input_format_str {
            "F32" => AudioFormat::F32,
            "I16" => AudioFormat::I16,
            "I32" => AudioFormat::I32,
            _ => AudioFormat::F32,
        };

        // Convert samples
        let converted_samples = self.convert_samples(input_format, &samples)?;

        // Create output
        let format_str = match self.target_format {
            AudioFormat::F32 => "F32",
            AudioFormat::I16 => "I16",
            AudioFormat::I32 => "I32",
        };

        let output = serde_json::json!({
            "data": converted_samples,
            "sample_rate": sample_rate,
            "channels": channels,
            "format": format_str
        });

        Ok(vec![output])
    }

    async fn cleanup(&mut self) -> Result<()> {
        // No cleanup needed
        Ok(())
    }
}

/// Factory for creating RustFormatConverterNode instances
pub struct FormatConverterNodeFactory {
    target_format: AudioFormat,
}

impl FormatConverterNodeFactory {
    pub fn new(target_format: AudioFormat) -> Self {
        Self { target_format }
    }
}

impl NodeFactory for FormatConverterNodeFactory {
    fn create(&self, _params: Value) -> Result<Box<dyn NodeExecutor>> {
        Ok(Box::new(RustFormatConverterNode::new(self.target_format)))
    }

    fn node_type(&self) -> &str {
        "RustFormatConverterNode"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_format_converter_creation() {
        let node = RustFormatConverterNode::new(AudioFormat::I16);
        assert_eq!(node.target_format, AudioFormat::I16);
    }

    #[tokio::test]
    async fn test_format_converter_initialize() {
        let mut node = RustFormatConverterNode::new(AudioFormat::F32);
        let context = NodeContext {
            node_id: "test".to_string(),
            node_type: "format_converter".to_string(),
            params: serde_json::json!({}),
            metadata: std::collections::HashMap::new(),
        };
        
        let result = node.initialize(&context).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_format_converter_factory() {
        let factory = FormatConverterNodeFactory::new(AudioFormat::I16);
        let node = factory.create(serde_json::json!({}));
        
        assert!(node.is_ok());
    }

    #[tokio::test]
    async fn test_no_conversion_needed() {
        let node = RustFormatConverterNode::new(AudioFormat::F32);
        
        let input_samples = vec![0.1, 0.2, 0.3, 0.4];
        let result = node.convert_samples(AudioFormat::F32, &input_samples);
        
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), input_samples);
    }

    #[tokio::test]
    async fn test_f32_to_i16_conversion() {
        let node = RustFormatConverterNode::new(AudioFormat::I16);
        
        let input_samples = vec![0.0, 0.5, -0.5, 1.0];
        let result = node.convert_samples(AudioFormat::F32, &input_samples);
        
        assert!(result.is_ok());
        
        // Result should have same number of f32 values (but containing i16 data)
        let converted = result.unwrap();
        assert_eq!(converted.len(), 2); // 4 i16 values = 2 f32 values in storage
    }
}
