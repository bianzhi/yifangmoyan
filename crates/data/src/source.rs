//! 数据源抽象

use anyhow::Result;
use crate::types::{KLine, StockInfo, TimeFrame};

/// 数据源 trait — 统一的数据获取接口
pub trait DataSource: Send + Sync {
    /// 数据源名称
    fn name(&self) -> &str;

    /// 检查数据源是否可用
    fn is_available(&self) -> bool;

    /// 获取 K 线数据
    fn get_klines(
        &self,
        symbol: &str,
        timeframe: TimeFrame,
        start: Option<&str>,
        end: Option<&str>,
    ) -> Result<Vec<KLine>>;

    /// 搜索股票
    fn search_stocks(&self, keyword: &str) -> Result<Vec<StockInfo>>;

    /// 获取股票信息
    fn get_stock_info(&self, symbol: &str) -> Result<StockInfo>;
}
