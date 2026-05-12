//! # yifang-wyckoff: 墨岩K线分析系统 — 威科夫量价分析
//!
//! 实现威科夫量价理论的核心功能：
//! - 阶段识别（吸筹/拉升/派发/下跌）
//! - 关键形态标注（SC, AR, ST, Spring, UTAD, JOC, LPS, SOS, SOW）
//! - 趋势线、冰线标注
//! - 交易区间识别

pub mod types;
pub mod phase;
pub mod pattern;
pub mod annotation;
pub mod analyzer;

pub use analyzer::WyckoffAnalyzer;
