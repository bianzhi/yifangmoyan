//! # yifang-indicator: 墨岩K线分析系统 — 技术指标
//!
//! 提供常用技术指标计算：
//! - MACD (Moving Average Convergence Divergence)
//! - 成交量分析

pub mod macd;
pub mod volume;

pub use macd::calc_macd;
pub use volume::VolumeAnalysis;
