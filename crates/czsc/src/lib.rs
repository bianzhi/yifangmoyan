//! # yifang-czsc: 墨岩K线分析系统 — 缠论核心
//!
//! 实现缠论（缠中说禅技术分析）的核心算法：
//! - 去除包含关系
//! - 分型识别
//! - 笔的构建
//! - 线段分析
//! - 中枢识别（笔中枢、线段中枢）
//! - 背驰检测
//! - 三类买卖点

pub mod include;
pub mod fenxing;
pub mod bi;
pub mod xd;
pub mod zs;
pub mod beichi;
pub mod buy_sell;
pub mod analyzer;

pub use analyzer::CzscAnalyzer;
