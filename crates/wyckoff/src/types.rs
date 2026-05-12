//! 威科夫内部类型

use serde::{Deserialize, Serialize};

/// 威科夫市场阶段
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum WyckoffPhase {
    /// 吸筹 (Accumulation)
    Accumulation,
    /// 拉升 (Markup)
    Markup,
    /// 派发 (Distribution)
    Distribution,
    /// 下跌 (Markdown)
    Markdown,
}

impl std::fmt::Display for WyckoffPhase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WyckoffPhase::Accumulation => write!(f, "吸筹"),
            WyckoffPhase::Markup => write!(f, "拉升"),
            WyckoffPhase::Distribution => write!(f, "派发"),
            WyckoffPhase::Markdown => write!(f, "下跌"),
        }
    }
}
