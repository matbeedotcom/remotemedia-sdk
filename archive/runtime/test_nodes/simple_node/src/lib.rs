use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct SimpleNode {
    pub multiplier: f64,
}

impl SimpleNode {
    pub fn new(multiplier: f64) -> Self {
        Self { multiplier }
    }

    pub fn process(&self, value: f64) -> f64 {
        value * self.multiplier
    }
}
