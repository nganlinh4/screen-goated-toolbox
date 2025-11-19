/// Centralized Groq Model Configuration
/// 
/// This module manages which model(s) to use for translation requests.
/// You can easily switch between rotation mode and single-model mode here.

pub struct ModelSelector {
    use_rotation: bool,
    current_is_maverick: bool,
}

impl ModelSelector {
    /// Create a new model selector
    /// 
    /// # Arguments
    /// * `use_rotation` - If true, alternates between Scout and Maverick. If false, uses only Scout.
    pub fn new(use_rotation: bool) -> Self {
        Self {
            use_rotation,
            current_is_maverick: false,
        }
    }

    /// Get the next model to use and update internal state
    /// 
    /// Returns the model name as a string
    pub fn get_next_model(&mut self) -> String {
        if self.use_rotation {
            // Toggle between models
            self.current_is_maverick = !self.current_is_maverick;
            if self.current_is_maverick {
                "meta-llama/llama-4-maverick-17b-128e-instruct".to_string()
            } else {
                "meta-llama/llama-4-scout-17b-16e-instruct".to_string()
            }
        } else {
            // Always use Scout
            "meta-llama/llama-4-scout-17b-16e-instruct".to_string()
        }
    }

    /// Get the current model without changing state
    pub fn current_model(&self) -> String {
        if self.use_rotation {
            if self.current_is_maverick {
                "meta-llama/llama-4-maverick-17b-128e-instruct".to_string()
            } else {
                "meta-llama/llama-4-scout-17b-16e-instruct".to_string()
            }
        } else {
            "meta-llama/llama-4-scout-17b-16e-instruct".to_string()
        }
    }
}

// --- CONFIGURATION CONSTANTS ---

/// Set this to `true` to rotate between Scout and Maverick models
/// Set this to `false` to always use Scout only
pub const USE_MODEL_ROTATION: bool = true;

/// Available model names for reference
pub mod models {
    pub const SCOUT: &str = "meta-llama/llama-4-scout-17b-16e-instruct";
    pub const MAVERICK: &str = "meta-llama/llama-4-maverick-17b-128e-instruct";
}
