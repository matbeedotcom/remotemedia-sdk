//! Sampling strategies for LLM text generation

use rand::Rng;

/// Sampler for token selection during generation
pub struct Sampler {
    temperature: f32,
    top_p: f32,
    top_k: usize,
    repeat_penalty: f32,
}

impl Sampler {
    /// Create a new sampler with given parameters
    pub fn new(temperature: f32, top_p: f32, top_k: u32, repeat_penalty: f32) -> Self {
        Self {
            temperature: temperature.max(0.0),
            top_p: top_p.clamp(0.0, 1.0),
            top_k: top_k as usize,
            repeat_penalty: repeat_penalty.max(1.0),
        }
    }

    /// Sample a token from logits
    pub fn sample(&self, logits: &mut [f32], previous_tokens: &[u32]) -> u32 {
        // Apply repeat penalty
        self.apply_repeat_penalty(logits, previous_tokens);

        // Apply temperature
        if self.temperature > 0.0 {
            for logit in logits.iter_mut() {
                *logit /= self.temperature;
            }
        }

        // Convert to probabilities via softmax
        let probs = self.softmax(logits);

        // Apply top-k filtering
        let filtered = self.top_k_filter(&probs);

        // Apply top-p (nucleus) sampling
        let nucleus = self.top_p_filter(&filtered);

        // Sample from distribution
        self.sample_from_probs(&nucleus)
    }

    /// Apply repetition penalty to previously seen tokens
    fn apply_repeat_penalty(&self, logits: &mut [f32], previous_tokens: &[u32]) {
        if self.repeat_penalty <= 1.0 {
            return;
        }

        for &token in previous_tokens {
            if let Some(logit) = logits.get_mut(token as usize) {
                if *logit > 0.0 {
                    *logit /= self.repeat_penalty;
                } else {
                    *logit *= self.repeat_penalty;
                }
            }
        }
    }

    /// Softmax function
    fn softmax(&self, logits: &[f32]) -> Vec<(usize, f32)> {
        let max_logit = logits.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
        let exp_sum: f32 = logits.iter().map(|x| (x - max_logit).exp()).sum();

        logits
            .iter()
            .enumerate()
            .map(|(i, &x)| (i, (x - max_logit).exp() / exp_sum))
            .collect()
    }

    /// Filter to top-k tokens
    fn top_k_filter(&self, probs: &[(usize, f32)]) -> Vec<(usize, f32)> {
        if self.top_k == 0 || self.top_k >= probs.len() {
            return probs.to_vec();
        }

        let mut sorted = probs.to_vec();
        sorted.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
        sorted.truncate(self.top_k);
        sorted
    }

    /// Filter using nucleus (top-p) sampling
    fn top_p_filter(&self, probs: &[(usize, f32)]) -> Vec<(usize, f32)> {
        if self.top_p >= 1.0 {
            return probs.to_vec();
        }

        let mut sorted = probs.to_vec();
        sorted.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());

        let mut cumsum = 0.0;
        let mut cutoff_idx = sorted.len();
        
        for (i, (_, prob)) in sorted.iter().enumerate() {
            cumsum += prob;
            if cumsum > self.top_p {
                cutoff_idx = i + 1;
                break;
            }
        }

        sorted.truncate(cutoff_idx);
        sorted
    }

    /// Sample a token from probability distribution
    fn sample_from_probs(&self, probs: &[(usize, f32)]) -> u32 {
        if probs.is_empty() {
            return 0;
        }

        // Greedy sampling if temperature is 0
        if self.temperature == 0.0 {
            return probs
                .iter()
                .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap())
                .map(|(i, _)| *i as u32)
                .unwrap_or(0);
        }

        // Normalize probabilities
        let total: f32 = probs.iter().map(|(_, p)| p).sum();
        let normalized: Vec<(usize, f32)> = probs
            .iter()
            .map(|(i, p)| (*i, p / total))
            .collect();

        // Random sampling
        let mut rng = rand::thread_rng();
        let r: f32 = rng.gen();
        let mut cumsum = 0.0;

        for (idx, prob) in normalized {
            cumsum += prob;
            if r <= cumsum {
                return idx as u32;
            }
        }

        // Fallback to last token
        probs.last().map(|(i, _)| *i as u32).unwrap_or(0)
    }
}

impl Default for Sampler {
    fn default() -> Self {
        Self::new(0.7, 0.9, 40, 1.1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sampler_creation() {
        let sampler = Sampler::new(0.7, 0.9, 40, 1.1);
        assert_eq!(sampler.temperature, 0.7);
        assert_eq!(sampler.top_p, 0.9);
    }

    #[test]
    fn test_greedy_sampling() {
        let sampler = Sampler::new(0.0, 1.0, 0, 1.0);
        let mut logits = vec![1.0, 2.0, 3.0, 0.5];
        let token = sampler.sample(&mut logits, &[]);
        assert_eq!(token, 2); // Highest logit
    }

    #[test]
    fn test_repeat_penalty() {
        let sampler = Sampler::new(0.0, 1.0, 0, 2.0);
        let mut logits = vec![1.0, 2.0, 3.0, 0.5];
        let _ = sampler.sample(&mut logits, &[2]); // Penalize token 2
        assert!(logits[2] < 3.0);
    }
}
