//! # yifang-data: 墨岩K线分析系统 — 数据层
//!
//! 提供 K 线数据的读取、合成和管理功能。
//! 支持 Parquet 文件读取（moyan-project 格式）和多级别 K 线合成。

pub mod types;
pub mod source;
pub mod kline_manager;

pub use types::*;
pub use source::DataSource;
pub use kline_manager::KLineManager;
