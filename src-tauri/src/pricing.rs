#[derive(Clone, Copy, Debug)]
pub struct ModelPricing {
    pub input: f64,
    pub output: f64,
    pub cache_5m_write: f64,
    pub cache_1h_write: f64,
    pub cache_read: f64,
}

const UNKNOWN: ModelPricing = ModelPricing {
    input: 0.0,
    output: 0.0,
    cache_5m_write: 0.0,
    cache_1h_write: 0.0,
    cache_read: 0.0,
};

pub fn lookup(model: &str) -> ModelPricing {
    if model.starts_with("claude-opus-4") || model.starts_with("claude-fable-") {
        ModelPricing {
            input: 15.0,
            output: 75.0,
            cache_5m_write: 18.75,
            cache_1h_write: 30.0,
            cache_read: 1.50,
        }
    } else if model.starts_with("claude-sonnet-4") {
        ModelPricing {
            input: 3.0,
            output: 15.0,
            cache_5m_write: 3.75,
            cache_1h_write: 6.0,
            cache_read: 0.30,
        }
    } else if model.starts_with("claude-haiku-4") {
        ModelPricing {
            input: 1.0,
            output: 5.0,
            cache_5m_write: 1.25,
            cache_1h_write: 2.0,
            cache_read: 0.10,
        }
    } else {
        UNKNOWN
    }
}
