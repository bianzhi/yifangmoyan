//! # yifang-data: 墨岩K线分析系统 — 数据层
//!
//! 提供 K 线数据的读取、合成和管理功能。
//! 支持 Parquet 文件读取（moyan-project 格式）和多级别 K 线合成。

pub mod types;
pub mod source;
pub mod kline_manager;
pub mod sync;

pub use types::*;
pub use source::DataSource;
pub use kline_manager::{KLineManager, MoveDataResult};
pub use sync::{
    sync_stock, sync_board, get_data_status, get_all_stock_codes, get_board_stats,
    get_stock_codes_by_board, fetch_board_stock_codes, fetch_board_online_count, get_board_online_info,
    validate_stock, validate_stock_level, cross_validate_stock,
    SyncStockResult, SyncLevelResult, DataStatus, LevelStats, BoardStats, BoardOnlineInfo,
    ValidateStockResult, ValidateLevelResult, ValidationIssue,
};
