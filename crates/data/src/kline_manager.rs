//! K 线管理器 — 读取 Parquet 数据，多级别 K 线合成
//!
//! 数据目录结构: {data_dir}/kline_cache/{timeframe_dir}/{symbol}.parquet
//! 例: data/kline_cache/1d/000001.parquet
//!
//! Parquet 文件列: datetime(timestamp[ns]), Open, High, Low, Close, Volume

use anyhow::{Context, Result};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use crate::source::DataSource;
use crate::types::*;
use serde::{Deserialize, Serialize};

/// 股票名称缓存条目
#[derive(Debug, Clone, Serialize, Deserialize)]
struct StockNameEntry {
    name: String,
    pinyin: String,
}

/// 数据移动结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MoveDataResult {
    pub moved: usize,
    pub failed: usize,
    pub errors: Vec<String>,
}

/// K 线管理器
pub struct KLineManager {
    data_dir: PathBuf,
    /// 股票名称缓存: code -> StockNameEntry
    stock_name_cache: Mutex<Option<HashMap<String, StockNameEntry>>>,
}

impl KLineManager {
    /// 创建 K 线管理器
    /// data_dir 为 None 时使用应用数据目录下的 data 子目录
    pub fn new(data_dir: Option<&str>) -> Self {
        let data_dir = data_dir
            .map(PathBuf::from)
            .unwrap_or_else(|| default_data_dir());
        Self {
            data_dir,
            stock_name_cache: Mutex::new(None),
        }
    }

    /// 获取数据目录
    pub fn data_dir(&self) -> &Path {
        &self.data_dir
    }

    /// 设置数据目录（运行时切换）
    pub fn set_data_dir(&mut self, path: &str) {
        self.data_dir = PathBuf::from(path);
    }

    /// 将现有数据移动到新目录
    /// old_dir: 旧目录路径
    /// new_dir: 新目录路径
    /// 返回: (移动文件数, 失败文件数, 失败信息列表)
    pub fn move_data_to(old_dir: &Path, new_dir: &Path) -> Result<(usize, usize, Vec<String>)> {
        let old_cache = old_dir.join("kline_cache");
        let new_cache = new_dir.join("kline_cache");

        if !old_cache.exists() {
            return Ok((0, 0, vec![]));
        }

        // 创建新目录
        std::fs::create_dir_all(&new_cache)
            .with_context(|| format!("创建目标目录失败: {}", new_cache.display()))?;

        let mut moved = 0usize;
        let mut failed = 0usize;
        let mut errors = Vec::new();

        // 遍历旧目录下的所有子目录 (1d, 1wk, 1mo, 1h, ...)
        for entry in std::fs::read_dir(&old_cache)
            .with_context(|| format!("读取旧目录失败: {}", old_cache.display()))?
        {
            let entry = entry?;
            let ft = entry.file_type()?;
            if !ft.is_dir() {
                continue;
            }

            let sub_dir_name = entry.file_name();
            let old_sub = entry.path();
            let new_sub = new_cache.join(&sub_dir_name);

            // 创建目标子目录
            if let Err(e) = std::fs::create_dir_all(&new_sub) {
                errors.push(format!("创建 {} 失败: {}", new_sub.display(), e));
                failed += 1;
                continue;
            }

            // 移动所有 parquet 文件
            for file_entry in std::fs::read_dir(&old_sub)
                .with_context(|| format!("读取 {} 失败", old_sub.display()))?
            {
                let file_entry = file_entry?;
                let fname = file_entry.file_name();
                let name_str = fname.to_string_lossy();

                // 只移动 parquet 文件
                if !name_str.ends_with(".parquet") {
                    continue;
                }

                let src = file_entry.path();
                let dst = new_sub.join(&fname);

                match std::fs::rename(&src, &dst) {
                    Ok(()) => moved += 1,
                    Err(e) => {
                        // 跨文件系统移动需要复制+删除
                        if let Err(copy_err) = std::fs::copy(&src, &dst).and_then(|_| std::fs::remove_file(&src)) {
                            errors.push(format!("移动 {} 失败: rename={}, copy={}", src.display(), e, copy_err));
                            failed += 1;
                        } else {
                            moved += 1;
                        }
                    }
                }
            }
        }

        // 尝试删除旧空目录（非递归，仅当空时）
        let _ = try_remove_empty_dirs(&old_cache);

        Ok((moved, failed, errors))
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
            // 本地无该股票数据，返回空列表（前端显示空图表，而非报错）
            return Ok(Vec::new());
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
    pub fn extract_datetime(column: &polars::prelude::Column, idx: usize) -> String {
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
                if let Some(dt) = chrono::DateTime::from_timestamp(secs, 0).map(|dt| dt.naive_local()) {
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
    pub fn extract_f64(column: &polars::prelude::Column, idx: usize) -> f64 {
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
    /// 确保股票名称缓存已加载（懒加载，首次搜索时触发）
    pub fn ensure_stock_name_cache(&self) {
        let mut cache = self.stock_name_cache.lock().unwrap();
        if cache.is_some() {
            return;
        }

        // 尝试从本地 JSON 缓存文件加载
        let cache_path = self.data_dir.join("stock_names.json");
        if cache_path.exists() {
            if let Ok(data) = std::fs::read_to_string(&cache_path) {
                if let Ok(map) = serde_json::from_str::<HashMap<String, StockNameEntry>>(&data) {
                    eprintln!("[搜索] 从缓存加载 {} 只股票名称", map.len());
                    *cache = Some(map);
                    return;
                }
            }
        }

        // 缓存文件不存在或无效，从在线 API 获取
        eprintln!("[搜索] 从在线API获取股票名称列表...");
        let mut map: HashMap<String, StockNameEntry> = HashMap::new();

        // 优先使用 Tushare（数据最全，有 name 字段）
        if let Ok(codes_with_names) = crate::sync::fetch_stock_list_with_names() {
            for (code, name) in &codes_with_names {
                let pinyin = to_pinyin_abbr(name);
                map.insert(code.clone(), StockNameEntry {
                    name: name.clone(),
                    pinyin,
                });
            }
            eprintln!("[搜索] Tushare 获取到 {} 只股票名称", codes_with_names.len());
        }

        // Tushare 失败，尝试东方财富
        if map.is_empty() {
            if let Ok(codes_with_names) = crate::sync::fetch_stock_names_eastmoney() {
                for (code, name) in &codes_with_names {
                    let pinyin = to_pinyin_abbr(name);
                    map.insert(code.clone(), StockNameEntry {
                        name: name.clone(),
                        pinyin,
                    });
                }
                eprintln!("[搜索] 东方财富获取到 {} 只股票名称", codes_with_names.len());
            }
        }

        if !map.is_empty() {
            // 保存缓存文件
            if let Ok(json) = serde_json::to_string(&map) {
                if let Err(e) = std::fs::write(&cache_path, json) {
                    eprintln!("[搜索] 保存名称缓存失败: {}", e);
                }
            }
        }

        // 始终注入指数/板块指数名称（即使在线 API 失败也保证可搜索指数）
        let index_map = crate::sync::get_index_name_map();
        for (code, (name, pinyin)) in &index_map {
            map.entry(code.clone()).or_insert_with(|| StockNameEntry {
                name: name.clone(),
                pinyin: pinyin.clone(),
            });
        }

        *cache = Some(map);
    }

    /// 使股票名称缓存失效（数据同步后可调用）
    pub fn invalidate_stock_name_cache(&self) {
        let mut cache = self.stock_name_cache.lock().unwrap();
        *cache = None;
        // 删除缓存文件，下次搜索时重新获取
        let cache_path = self.data_dir.join("stock_names.json");
        let _ = std::fs::remove_file(cache_path);
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

        // 分钟级数据需包含时间部分，过滤掉只有日期的脏数据
        // （旧版同步可能将日线数据混入了分钟级文件）
        if timeframe.is_minute_level() {
            klines.retain(|k| k.dt.contains(':'));
        }

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
        // 确保缓存已加载
        self.ensure_stock_name_cache();

        let keyword_lower = keyword.to_lowercase();
        let cache = self.stock_name_cache.lock().unwrap();
        let name_cache = cache.as_ref().unwrap();

        // 从日线目录获取全部股票代码
        let day_dir = self.cache_dir().join("1d");
        let dir = if day_dir.exists() {
            day_dir
        } else {
            self.cache_dir()
        };

        let mut seen = std::collections::HashSet::new();
        let mut results = Vec::new();

        // ── 1. 从本地 parquet 文件匹配（已有缓存） ──
        if let Ok(entries) = std::fs::read_dir(&dir) {
            for entry in entries.flatten() {
                let fname = entry.file_name().to_string_lossy().to_string();
                if let Some(code) = fname.strip_suffix(".parquet") {
                    // 获取缓存的名称和拼音
                    let (name, pinyin) = match name_cache.get(code) {
                        Some(entry) => (entry.name.clone(), entry.pinyin.clone()),
                        None => (String::new(), String::new()),
                    };

                    // 匹配：代码包含、名称包含、拼音前缀匹配
                    let matched = code.contains(keyword)
                        || name.contains(keyword)
                        || pinyin.starts_with(&keyword_lower)
                        || pinyin.contains(&keyword_lower);

                    if matched {
                        seen.insert(code.to_string());
                        results.push(StockInfo {
                            symbol: code.to_string(),
                            name,
                            pinyin,
                            market: if code.starts_with("BK") || code.starts_with("88") {
                                "BK".to_string()
                            } else if code.starts_with('6') || code.starts_with('9') {
                                "SH".to_string()
                            } else {
                                "SZ".to_string()
                            },
                        });
                    }
                }
            }
        }

        // ── 2. 从名称缓存中补充匹配（未缓存的概念板块/指数等） ──
        for (code, entry) in name_cache.iter() {
            if seen.contains(code) {
                continue;
            }
            let matched = code.contains(keyword)
                || entry.name.contains(keyword)
                || entry.pinyin.starts_with(&keyword_lower)
                || entry.pinyin.contains(&keyword_lower);
            if matched {
                seen.insert(code.clone());
                results.push(StockInfo {
                    symbol: code.clone(),
                    name: entry.name.clone(),
                    pinyin: entry.pinyin.clone(),
                    market: if code.starts_with("BK") || code.starts_with("88") {
                        "BK".to_string()
                    } else if code.starts_with('6') || code.starts_with('9') {
                        "SH".to_string()
                    } else {
                        "SZ".to_string()
                    },
                });
            }
        }

        // 排序：代码前缀匹配优先，然后拼音前缀匹配，最后按代码排序
        results.sort_by(|a, b| {
            let a_code_match = a.symbol.starts_with(keyword);
            let b_code_match = b.symbol.starts_with(keyword);
            let a_pinyin_match = a.pinyin.starts_with(&keyword_lower);
            let b_pinyin_match = b.pinyin.starts_with(&keyword_lower);

            match (a_code_match, b_code_match) {
                (true, false) => std::cmp::Ordering::Less,
                (false, true) => std::cmp::Ordering::Greater,
                _ => match (a_pinyin_match, b_pinyin_match) {
                    (true, false) => std::cmp::Ordering::Less,
                    (false, true) => std::cmp::Ordering::Greater,
                    _ => a.symbol.cmp(&b.symbol),
                },
            }
        });

        Ok(results)
    }

    fn get_stock_info(&self, symbol: &str) -> Result<StockInfo> {
        // 确保缓存已加载
        self.ensure_stock_name_cache();

        let market = if symbol.starts_with("BK") || symbol.starts_with("88") {
            "BK"
        } else if symbol.starts_with('6') || symbol.starts_with('9') {
            "SH"
        } else {
            "SZ"
        };

        let cache = self.stock_name_cache.lock().unwrap();
        let (name, pinyin) = match cache.as_ref().unwrap().get(symbol) {
            Some(entry) => (entry.name.clone(), entry.pinyin.clone()),
            None => (String::new(), String::new()),
        };

        Ok(StockInfo {
            symbol: symbol.to_string(),
            name,
            pinyin,
            market: market.to_string(),
        })
    }
}

// ═══════════════════════════════════════════════════════════
//  辅助函数
// ═══════════════════════════════════════════════════════════

/// 默认数据目录：优先使用 YIFANG_DATA_DIR 环境变量，否则使用 app 数据目录
fn default_data_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("YIFANG_DATA_DIR") {
        return PathBuf::from(dir);
    }

    // macOS: ~/Library/Application Support/com.moyan.yifang/data
    // Linux: ~/.local/share/com.moyan.yifang/data
    // Windows: C:\Users\{user}\AppData\Roaming\com.moyan.yifang\data
    if let Some(data_dir) = dirs::data_dir() {
        return data_dir.join("com.moyan.yifang").join("data");
    }

    // 兜底：当前目录下的 data
    PathBuf::from("data")
}

/// 汉字转拼音缩写（取每个汉字的首字母）
/// 例: "平安银行" -> "payh", "贵州茅台" -> "gzmt"
fn to_pinyin_abbr(name: &str) -> String {
    use pinyin::ToPinyin;

    let mut result = String::new();
    for c in name.chars() {
        if let Some(pinyin) = c.to_pinyin() {
            if let Some(first) = pinyin.with_tone_num().chars().next() {
                result.push(first.to_ascii_lowercase());
            }
        } else {
            // 非汉字字符（数字、字母等）直接保留
            if c.is_ascii_alphanumeric() {
                result.push(c.to_ascii_lowercase());
            }
        }
    }
    result
}

/// 递归尝试删除空目录
pub fn try_remove_empty_dirs(dir: &Path) -> std::io::Result<()> {
    if !dir.exists() {
        return Ok(());
    }

    // 先处理子目录
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            let _ = try_remove_empty_dirs(&entry.path());
        }
    }

    // 尝试删除自身（只有空目录才能删成功）
    let _ = std::fs::remove_dir(dir);
    Ok(())
}
