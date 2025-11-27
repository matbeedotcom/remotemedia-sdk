//! Output formatting for CLI commands

use anyhow::Result;
use clap::ValueEnum;
use serde::Serialize;

/// Output format options
#[derive(Debug, Clone, Copy, ValueEnum, Default)]
pub enum OutputFormat {
    /// Plain text output
    #[default]
    Text,
    /// JSON output
    Json,
    /// Table output
    Table,
}

/// Output helper
pub struct Outputter {
    format: OutputFormat,
}

impl Outputter {
    pub fn new(format: OutputFormat) -> Self {
        Self { format }
    }

    /// Output a value in the configured format
    pub fn output<T: Serialize>(&self, value: &T) -> Result<()> {
        match self.format {
            OutputFormat::Json => {
                println!("{}", serde_json::to_string_pretty(value)?);
            }
            OutputFormat::Text | OutputFormat::Table => {
                // For text output, try to serialize as JSON and print
                // Individual commands should handle their own text formatting
                let json = serde_json::to_value(value)?;
                if let Some(obj) = json.as_object() {
                    for (key, value) in obj {
                        println!("{}: {}", key, format_value(value));
                    }
                } else {
                    println!("{}", serde_json::to_string_pretty(value)?);
                }
            }
        }
        Ok(())
    }

    /// Get the format
    pub fn format(&self) -> OutputFormat {
        self.format
    }
}

fn format_value(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::Null => "null".to_string(),
        serde_json::Value::Bool(b) => b.to_string(),
        serde_json::Value::Number(n) => n.to_string(),
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Array(arr) => {
            let items: Vec<String> = arr.iter().map(format_value).collect();
            format!("[{}]", items.join(", "))
        }
        serde_json::Value::Object(_) => serde_json::to_string(value).unwrap_or_default(),
    }
}
