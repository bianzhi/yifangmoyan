//! # yifang-wyckoff: 墨岩K线分析系统 — 威科夫量价分析
//!
//! 严格实现威科夫（Richard D. Wyckoff）量价理论核心算法。
//!
//! ## 威科夫三大法则
//!
//! 1. **供需法则** (Law of Supply and Demand): 价格由供需关系决定
//!    - 需求 > 供给 → 价格上涨
//!    - 供给 > 需求 → 价格下跌
//!    - 供给 = 需求 → 价格横盘
//!
//! 2. **因果法则** (Law of Cause and Effect): 
//!    - 交易区间（因）→ 趋势运动（果）
//!    - 因越大，果越大
//!    - 吸筹区间越宽，后续涨幅越大
//!
//! 3. **努力与结果法则** (Law of Effort vs. Result):
//!    - 成交量 = 努力，价格变动 = 结果
//!    - 量价协调 → 趋势持续
//!    - 量价背离 → 趋势可能反转
//!
//! ## 模块结构
//!
//! - `effort`: 努力与结果法则实现
//! - `phase`: 阶段识别（吸筹5阶段 + 派发5阶段）
//! - `pattern`: 威科夫关键形态事件识别
//! - `trading_range`: 交易区间识别（结构驱动）
//! - `supply_demand`: 供需线绘制
//! - `annotation`: 标注生成
//! - `analyzer`: 完整分析流程

pub mod effort;
pub mod phase;
pub mod pattern;
pub mod trading_range;
pub mod supply_demand;
pub mod annotation;
pub mod analyzer;

pub use analyzer::WyckoffAnalyzer;
