//! K 线管理器 — 读取 Parquet 数据，多级别 K 线合成
//!
//! 数据目录结构: {data_dir}/kline_cache/{timeframe_dir}/{symbol}.parquet
//! 例: data/kline_cache/1d/000001.parquet
//!
//! Parquet 文件列: datetime(timestamp[ns]), Open, High, Low, Close, Volume

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

use crate::source::DataSource;
use crate::types::*;

/// 默认数据目录
const DEFAULT_DATA_DIR: &str = "/Users/csdn/Code/moyan/moyan-project/data";

/// K 线管理器
pub struct KLineManager {
    data_dir: PathBuf,
}

impl KLineManager {
    /// 创建 K 线管理器
    pub fn new(data_dir: Option<&str>) -> Self {
        let data_dir = data_dir
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(DEFAULT_DATA_DIR));
        Self { data_dir }
    }

    /// 获取缓存根目录 (kline_cache)
    fn cache_dir(&self) -> PathBuf {
        self.data_dir.join("kline_cache")
    }

    /// 将 TimeFrame 映射为数据目录名
    fn timeframe_dir_name(tf: TimeFrame) -> &'static str {
        match tf {
            TimeFrame::M => "1mo",
            TimeFrame::W => "1wk",
            TimeFrame::D => "1d",
            TimeFrame::F60 => "1h",
            TimeFrame::F30 => "30m",
            TimeFrame::F15 => "15m",
            TimeFrame::F5 => "5m",
            TimeFrame::F1 => "1m",
        }
    }

    /// 从 Parquet 文件读取 K 线数据
    pub fn load_klines_from_parquet(
        &self,
        symbol: &str,
        timeframe: TimeFrame,
    ) -> Result<Vec<KLine>> {
        let tf_dir = Self::timeframe_dir_name(timeframe);
        let file_path = self.cache_dir().join(tf_dir).join(format!("{}.parquet", symbol));

        if !file_path.exists() {
            // 数据不存在: 尝试从更高级别合成
            if let Some(higher_klines) = self.try_resample_from_higher(symbol, timeframe)? {
                return Ok(higher_klines);
            }
            anyhow::bail!("Parquet 文件不存在: {:?}", file_path);
        }

        self.read_parquet_klines(&file_path, symbol, timeframe)
    }

    /// 尝试从更高级别 K 线合成目标级别
    fn try_resample_from_higher(
        &self,
        symbol: &str,
        timeframe: TimeFrame,
    ) -> Result<Option<Vec<KLine>>> {
        // 获取上级周期
        let higher_tf = match timeframe.higher_level() {
            Some(tf) => tf,
            None => return Ok(None),
        };

        let higher_klines = self.load_klines_from_parquet(symbol, higher_tf);
        match higher_klines {
            Ok(klines) if !klines.is_empty() => {
                // 简化: 无法从更高级别合成低级别数据，直接返回 None
                // (实际需要低级别数据才能合成高级别，而非反过来)
                Ok(None)
            }
            _ => Ok(None),
        }
    }

    /// 读取 parquet 文件中的 K 线数据
    fn read_parquet_klines(
        &self,
        path: &Path,
        symbol: &str,
        timeframe: TimeFrame,
    ) -> Result<Vec<KLine>> {
        use polars::prelude::*;

        let df = LazyFrame::scan_parquet(path, ScanArgsParquet::default())
            .context("读取 Parquet 文件失败")?
            .collect()
            .context("收集 Parquet 数据失败")?;

        if df.height() == 0 {
            return Ok(Vec::new());
        }

        self.dataframe_to_klines(&df, symbol, timeframe)
    }

    /// 将 DataFrame 转换为 KLine 结构体数组
    fn dataframe_to_klines(
        &self,
        df: &polars::prelude::DataFrame,
        symbol: &str,
        timeframe: TimeFrame,
    ) -> Result<Vec<KLine>> {
        let col_names = df.get_column_names_str();
        let n_rows = df.height();

        // 列名映射 — 适配多种格式
        let dt_name = col_names
            .iter()
            .find(|c| ["dt", "datetime", "date", "time", "timestamp"].contains(c))
            .context("找不到日期列")?;
        let open_name = col_names
            .iter()
            .find(|c| ["open", "Open"].contains(c))
            .context("找不到开盘价列")?;
        let close_name = col_names
            .iter()
            .find(|c| ["close", "Close"].contains(c))
            .context("找不到收盘价列")?;
        let high_name = col_names
            .iter()
            .find(|c| ["high", "High"].contains(c))
            .context("找不到最高价列")?;
        let low_name = col_names
            .iter()
            .find(|c| ["low", "Low"].contains(c))
            .context("找不到最低价列")?;
        let vol_name = col_names
            .iter()
            .find(|c| ["vol", "volume", "Volume"].contains(c))
            .context("找不到成交量列")?;
        // amount 列可选
        let amount_name = col_names
            .iter()
            .find(|c| ["amount", "Amount", "turnover"].contains(c));

        let dt_col = df.column(dt_name).context("获取日期列失败")?;
        let open_col = df.column(open_name).context("获取开盘价列失败")?;
        let close_col = df.column(close_name).context("获取收盘价列失败")?;
        let high_col = df.column(high_name).context("获取最高价列失败")?;
        let low_col = df.column(low_name).context("获取最低价列失败")?;
        let vol_col = df.column(vol_name).context("获取成交量列失败")?;
        let amount_col = amount_name
            .and_then(|name| df.column(name).ok());

        let mut klines = Vec::with_capacity(n_rows);
        for i in 0..n_rows {
            let dt = Self::extract_datetime(dt_col, i);
            let open = Self::extract_f64(open_col, i);
            let close = Self::extract_f64(close_col, i);
            let high = Self::extract_f64(high_col, i);
            let low = Self::extract_f64(low_col, i);
            let vol = Self::extract_f64(vol_col, i);
            let amount = amount_col
                .as_ref()
                .map(|c| Self::extract_f64(c, i))
                .unwrap_or(0.0);

            klines.push(KLine {
                symbol: symbol.to_string(),
                timeframe,
                dt,
                id: i as u64,
                open,
                close,
                high,
                low,
                vol,
                amount,
            });
        }

        Ok(klines)
    }

    /// 从 Column 中提取日期时间字符串
    fn extract_datetime(column: &polars::prelude::Column, idx: usize) -> String {
        // 优先处理字符串类型
        if let Some(ca) = column.try_str() {
            if let Some(v) = ca.get(idx) {
                return v.to_string();
            }
        }
        let series = column.as_materialized_series();
        // 处理 timestamp[ns] -> datetime
        if let Ok(ca) = series.datetime() {
            if let Some(v) = ca.get(idx) {
                // Polars datetime 值根据 TimeUnit 不同:
                // - Nanoseconds: v 是纳秒，除以 1_000_000_000 得秒
                // - Microseconds: v 是微秒，除以 1_000_000 得秒
                // - Milliseconds: v 是毫秒，除以 1_000 得秒
                let secs = match ca.time_unit() {
                    polars::prelude::TimeUnit::Nanoseconds => v / 1_000_000_000,
                    polars::prelude::TimeUnit::Microseconds => v / 1_000_000,
                    polars::prelude::TimeUnit::Milliseconds => v / 1_000,
                };
                if let Some(dt) = chrono::DateTime::from_timestamp(secs, 0).map(|dt| dt.naive_utc()) {
                    return dt.format("%Y-%m-%d %H:%M:%S").to_string();
                }
                return format!("{}", v);
            }
        }
        // 处理 date 类型
        if let Ok(ca) = series.date() {
            if let Some(v) = ca.get(idx) {
                // v 是距离 1970-01-01 的天数
                let days = v as i32;
                if let Some(dt) = chrono::NaiveDate::from_num_days_from_ce_opt(days + 719_163) {
                    return dt.format("%Y-%m-%d").to_string();
                }
                return format!("{}", v);
            }
        }
        format!("index_{}", idx)
    }

    /// 从 Column 中提取 f64 值
    fn extract_f64(column: &polars::prelude::Column, idx: usize) -> f64 {
        if let Some(ca) = column.try_f64() {
            if let Some(v) = ca.get(idx) {
                return v;
            }
        }
        if let Some(ca) = column.try_f32() {
            if let Some(v) = ca.get(idx) {
                return v as f64;
            }
        }
        if let Some(ca) = column.try_i64() {
            if let Some(v) = ca.get(idx) {
                return v as f64;
            }
        }
        if let Some(ca) = column.try_i32() {
            if let Some(v) = ca.get(idx) {
                return v as f64;
            }
        }
        if let Some(ca) = column.try_f64() {
            // 尝试从被物理化为 Int64 的列中提取
            let _ = ca;
        }
        0.0
    }

    /// 合成更高级别 K 线 (如 1m → 5m, 5m → 15m)
    pub fn resample_klines(&self, klines: &[KLine], target: TimeFrame) -> Result<Vec<KLine>> {
        if klines.is_empty() {
            return Ok(Vec::new());
        }

        let source_tf = klines[0].timeframe;
        if source_tf == target {
            return Ok(klines.to_vec());
        }

        let target_minutes = target.minutes().unwrap();
        let source_minutes = source_tf.minutes().unwrap();

        if target_minutes <= source_minutes {
            anyhow::bail!("目标周期 ({:?}) 必须大于源周期 ({:?})", target, source_tf);
        }

        if target_minutes % source_minutes != 0 {
            anyhow::bail!("目标周期 ({}) 不是源周期 ({}) 的整数倍", target_minutes, source_minutes);
        }

        let ratio = (target_minutes / source_minutes) as usize;
        let mut result = Vec::new();
        let mut i = 0;
        let mut result_id = 0u64;

        while i < klines.len() {
            let chunk_end = (i + ratio).min(klines.len());
            let chunk = &klines[i..chunk_end];

            if chunk.is_empty() {
                break;
            }

            let first = &chunk[0];
            let mut high = first.high;
            let mut low = first.low;
            let mut vol = 0.0;
            let mut amount = 0.0;

            for k in chunk {
                if k.high > high { high = k.high; }
                if k.low < low { low = k.low; }
                vol += k.vol;
                amount += k.amount;
            }

            let last = &chunk[chunk.len() - 1];

            result.push(KLine {
                symbol: first.symbol.clone(),
                timeframe: target,
                dt: first.dt.clone(),
                id: result_id,
                open: first.open,
                close: last.close,
                high,
                low,
                vol,
                amount,
            });

            result_id += 1;
            i = chunk_end;
        }

        Ok(result)
    }
}

impl DataSource for KLineManager {
    fn name(&self) -> &str {
        "parquet"
    }

    fn is_available(&self) -> bool {
        self.cache_dir().exists()
    }

    fn get_klines(
        &self,
        symbol: &str,
        timeframe: TimeFrame,
        start: Option<&str>,
        end: Option<&str>,
    ) -> Result<Vec<KLine>> {
        let mut klines = self.load_klines_from_parquet(symbol, timeframe)?;

        // 按日期过滤
        if start.is_some() || end.is_some() {
            klines.retain(|k| {
                let mut ok = true;
                if let Some(s) = start {
                    ok &= k.dt.as_str() >= s;
                }
                if let Some(e) = end {
                    ok &= k.dt.as_str() <= e;
                }
                ok
            });
        }

        Ok(klines)
    }

    fn search_stocks(&self, keyword: &str) -> Result<Vec<StockInfo>> {
        // 从日线目录获取全部股票列表
        let day_dir = self.cache_dir().join("1d");
        let dir = if day_dir.exists() {
            day_dir
        } else {
            self.cache_dir()
        };

        let entries = std::fs::read_dir(&dir)?;
        let mut results = Vec::new();

        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if let Some(code) = name.strip_suffix(".parquet") {
                if code.contains(keyword) {
                    results.push(StockInfo {
                        symbol: code.to_string(),
                        name: String::new(),
                        pinyin: String::new(),
                        market: if code.starts_with('6') || code.starts_with('9') {
                            "SH".to_string()
                        } else {
                            "SZ".to_string()
                        },
                    });
                }
            }
        }

        results.sort_by(|a, b| a.symbol.cmp(&b.symbol));
        Ok(results)
    }

    fn get_stock_info(&self, symbol: &str) -> Result<StockInfo> {
        let market = if symbol.starts_with('6') || symbol.starts_with('9') {
            "SH"
        } else {
            "SZ"
        };
        Ok(StockInfo {
            symbol: symbol.to_string(),
            name: String::new(),
            pinyin: String::new(),
            market: market.to_string(),
        })
    }
}
