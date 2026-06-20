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

// 公式: 5m write = input × 1.25, 1h write = input × 2.0, cache read = input × 0.1
// https://platform.claude.com/docs/en/about-claude/pricing
fn from_base(input: f64, output: f64) -> ModelPricing {
    ModelPricing {
        input,
        output,
        cache_5m_write: input * 1.25,
        cache_1h_write: input * 2.0,
        cache_read: input * 0.10,
    }
}

// claude-opus-4-7 / claude-opus-4-7-20251201 等から minor バージョン (= 7) を抜く。
// claude-opus-4 (無印, retired) は None。
fn opus_minor(model: &str) -> Option<u32> {
    let rest = model.strip_prefix("claude-opus-4-")?;
    rest.split('-').next()?.parse().ok()
}

pub fn lookup(model: &str) -> ModelPricing {
    if let Some(minor) = opus_minor(model) {
        // 4.5+ は新 pricing、 4.0/4.1 は legacy
        if minor >= 5 {
            return from_base(5.0, 25.0);
        } else {
            return from_base(15.0, 75.0);
        }
    }
    if model == "claude-opus-4" {
        return from_base(15.0, 75.0);
    }
    if model.starts_with("claude-fable-5") || model.starts_with("claude-mythos-5") {
        return from_base(10.0, 50.0);
    }
    if model.starts_with("claude-sonnet-4") {
        return from_base(3.0, 15.0);
    }
    if model.starts_with("claude-haiku-4") {
        return from_base(1.0, 5.0);
    }
    UNKNOWN
}
