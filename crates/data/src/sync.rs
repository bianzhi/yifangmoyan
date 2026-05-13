//! K 线数据同步模块 — 多数据源协同下载，带完整性校验
//!
//! 数据源优先级（按级别动态调整）：
//! - 日线/周线/月线: 新浪 → 腾讯 → Tushare → 网易 → 东方财富(日线)
//! - 60F/30F/15F/5F/1F: 东方财富(最优) → 新浪 → 腾讯 → Tushare
//!
//! 数据源特点：
//! 1. 新浪财经 (sina) — 全级别，速度快，最多 2000 条
//! 2. 腾讯财经 (tencent) — 全级别，稳定，最多 2000 条
//! 3. 东方财富 (eastmoney) — 分钟级最优，最多 10000 条，日线也支持
//! 4. Tushare — 数据质量最高，全级别，需要 token
//! 5. 网易财经 (netease) — 仅日线/周线/月线
//!
//! 同步策略：主源失败 → 自动切换备源；多源数据交叉验证

use anyhow::{Context, Result};
use chrono::NaiveDate;
use polars::prelude::*;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use crate::types::TimeFrame;

// ═══════════════════════════════════════════════════════════
//  数据源解析结构
// ═══════════════════════════════════════════════════════════

/// 新浪 K 线返回的单条记录
#[derive(Debug, Clone, Deserialize)]
struct SinaKlineItem {
    day: String,
    open: String,
    high: String,
    low: String,
    close: String,
    volume: String,
}

/// 腾讯 K 线返回格式 (CSV like: "日期,开盘,收盘,最高,最低,成交量")
/// 直接解析为 Vec<String>

/// 网易 K 线返回格式 (CSV: "日期,开盘,收盘,最高,最低,成交量,涨跌幅")
/// 直接解析为 Vec<String>

// ═══════════════════════════════════════════════════════════
//  内部 K 线记录
// ═══════════════════════════════════════════════════════════

/// K 线记录 (内部用)
#[derive(Debug, Clone)]
struct KlineRecord {
    datetime: String,
    open: f64,
    high: f64,
    low: f64,
    close: f64,
    volume: i64,
}

// ═══════════════════════════════════════════════════════════
//  同步配置
// ═══════════════════════════════════════════════════════════

/// 同步级别对应的参数
struct DataSourceConfig {
    /// 新浪 API scale 参数
    sina_scale: u32,
    /// 新浪请求量
    sina_datalen: u32,
    /// 腾讯 klt 参数 (K线类型)
    tencent_klt: Option<&'static str>,
    /// 网易 klt 参数
    netease_klt: Option<&'static str>,
    /// 东方财富 klt 参数 (K线类型)
    eastmoney_klt: Option<&'static str>,
    /// Tushare freq 参数
    tushare_freq: Option<&'static str>,
    /// 东方财富请求条数
    eastmoney_lmt: u32,
}


/// 内置 Tushare Token (从 moyan-project 配置迁移)
const TUSHARE_TOKEN: &str = "7b09c93667a6ac2a7c4bdc76bc8f3fe2977a93d412f39de40c0b51c3";

impl TimeFrame {
    /// 多数据源参数配置
    fn source_config(&self) -> Option<DataSourceConfig> {
        match self {
            TimeFrame::M => Some(DataSourceConfig {
                sina_scale: 240, // 新浪没有月线，要从日线重采样
                sina_datalen: 2000,
                tencent_klt: None, // 腾讯也没有月线
                netease_klt: Some("month"),
                eastmoney_klt: None,  // 东方财富不支持月线
                eastmoney_lmt: 0,
                tushare_freq: Some("M"), // Tushare 月线
            }),
            TimeFrame::W => Some(DataSourceConfig {
                sina_scale: 1200,
                sina_datalen: 600,
                tencent_klt: Some("w"),
                netease_klt: Some("week"),
                eastmoney_klt: None,  // 东方财富没有周线
                eastmoney_lmt: 0,
                tushare_freq: Some("W"),
            }),
            TimeFrame::D => Some(DataSourceConfig {
                sina_scale: 240,
                sina_datalen: 2000,
                tencent_klt: Some("day"),
                netease_klt: Some("day"),
                eastmoney_klt: Some("101"), // 东方财富日线 klt=101
                eastmoney_lmt: 5000,
                tushare_freq: Some("D"),
            }),
            TimeFrame::F60 => Some(DataSourceConfig {
                sina_scale: 60,
                sina_datalen: 500,
                tencent_klt: Some("60"),
                netease_klt: None,
                eastmoney_klt: Some("60"),
                eastmoney_lmt: 10000,
                tushare_freq: Some("60min"),
            }),
            TimeFrame::F30 => Some(DataSourceConfig {
                sina_scale: 30,
                sina_datalen: 500,
                tencent_klt: Some("30"),
                netease_klt: None,
                eastmoney_klt: Some("30"),
                eastmoney_lmt: 10000,
                tushare_freq: Some("30min"),
            }),
            TimeFrame::F15 => Some(DataSourceConfig {
                sina_scale: 15,
                sina_datalen: 500,
                tencent_klt: Some("15"),
                netease_klt: None,
                eastmoney_klt: Some("15"),
                eastmoney_lmt: 10000,
                tushare_freq: Some("15min"),
            }),
            TimeFrame::F5 => Some(DataSourceConfig {
                sina_scale: 5,
                sina_datalen: 500,
                tencent_klt: Some("5"),
                netease_klt: None,
                eastmoney_klt: Some("5"),
                eastmoney_lmt: 10000,
                tushare_freq: Some("5min"),
            }),
            TimeFrame::F1 => Some(DataSourceConfig {
                sina_scale: 1,
                sina_datalen: 500,
                tencent_klt: Some("1"),
                netease_klt: None,
                eastmoney_klt: Some("1"),
                eastmoney_lmt: 10000,
                tushare_freq: Some("1min"),
            }),
        }
    }
}

// ═══════════════════════════════════════════════════════════
//  公共类型
// ═══════════════════════════════════════════════════════════

/// 单只股票单级别的同步结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncLevelResult {
    pub level: String,
    pub status: String,       // "ok" | "skip" | "fail" | "error"
    pub count: usize,         // 最终保存的记录数
    pub source: String,       // 实际使用的数据源
    pub msg: String,
}

/// 单只股票的完整同步结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncStockResult {
    pub symbol: String,
    pub levels: Vec<SyncLevelResult>,
}

/// 数据目录中某级别的统计
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LevelStats {
    pub level: String,
    pub dir_name: String,
    pub file_count: usize,
    pub sample_symbol: Option<String>,
    pub sample_count: Option<usize>,
    pub sample_start: Option<String>,
    pub sample_end: Option<String>,
}

/// 整体数据状态
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataStatus {
    pub data_dir: String,
    pub total_stocks: usize,
    pub levels: Vec<LevelStats>,
    pub boards: Vec<BoardStats>,
}

/// 板块统计
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BoardStats {
    /// 板块标识：sh_main / sz_main / gem / star / all_a
    pub id: String,
    /// 板块名称：上证主板 / 深证主板 / 创业板 / 科创板 / 全 A 股
    pub name: String,
    /// 该板块已有数据文件数
    pub count: usize,
}

/// 根据股票代码判断板块
pub fn classify_board(code: &str) -> &'static str {
    if code.len() == 6 && (code.starts_with('4') || code.starts_with('8')) {
        "bse" // 北交所
    } else if code.starts_with("688") || code.starts_with("689") {
        "star" // 科创板
    } else if code.starts_with("300") || code.starts_with("301") {
        "gem" // 创业板
    } else if code.starts_with("6") || code.starts_with("9") {
        "sh_main" // 上证主板（6xx/9xx）
    } else if code.starts_with("000") || code.starts_with("001")
        || code.starts_with("002") || code.starts_with("003")
    {
        "sz_main" // 深证主板（含原中小板002/003）
    } else {
        "other"
    }
}

/// 获取各板块统计
pub fn get_board_stats(data_dir: &Path) -> Vec<BoardStats> {
    let codes = get_all_stock_codes(data_dir);

    let mut sh_main = 0usize;
    let mut sz_main = 0usize;
    let mut gem = 0usize;
    let mut star = 0usize;
    let mut bse = 0usize;

    for code in &codes {
        match classify_board(code) {
            "sh_main" => sh_main += 1,
            "sz_main" => sz_main += 1,
            "gem" => gem += 1,
            "star" => star += 1,
            "bse" => bse += 1,
            _ => {}
        }
    }

    vec![
        BoardStats { id: "sh_main".into(), name: "上证主板".into(), count: sh_main },
        BoardStats { id: "sz_main".into(), name: "深证主板".into(), count: sz_main },
        BoardStats { id: "gem".into(), name: "创业板".into(), count: gem },
        BoardStats { id: "star".into(), name: "科创板".into(), count: star },
        BoardStats { id: "bse".into(), name: "北交所".into(), count: bse },
        BoardStats { id: "all_a".into(), name: "全 A 股".into(), count: codes.len() },
    ]
}

/// 构建东方财富专用 HTTP client（带正确的 Referer + User-Agent + 重试）
fn build_eastmoney_client() -> Result<reqwest::blocking::Client> {
    Ok(reqwest::blocking::Client::builder()
        .user_agent("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36")
        .timeout(std::time::Duration::from_secs(15))
        .build()?)
}

/// 从东方财富在线 API 获取指定板块的股票代码列表
/// 支持分页，确保获取全部股票
pub fn fetch_board_stock_codes(board: &str) -> Result<Vec<String>> {
    let fs = match board {
        "sh_main" => "m:1+t:2",           // 上证主板
        "sz_main" => "m:0+t:6",           // 深证主板
        "gem" => "m:0+t:80",              // 创业板
        "star" => "m:1+t:23",             // 科创板
        "bse" => "m:0+t:81",              // 北交所
        "all_a" => "m:0+t:6,m:0+t:80,m:1+t:2,m:1+t:23,m:0+t:81", // 全部 A 股
        _ => return Err(anyhow::anyhow!("未知板块: {}", board)),
    };

    let client = build_eastmoney_client()?;

    // 分页获取全部股票代码
    let mut all_codes = Vec::new();
    let mut page = 1;
    let page_size = 5000u64;

    loop {
        let url = format!(
            "https://push2.eastmoney.com/api/qt/clist/get?pn={}&pz={}&po=1&np=1&ut=bd1d9ddb04089700cf9c27f6f7426281&fltt=2&invt=2&fid=f3&fs={}&fields=f12",
            page, page_size, fs
        );

        let mut last_err = None;
        let mut codes = Vec::new();
        let mut total = 0usize;

        // 每页最多重试 3 次
        for attempt in 0..3 {
            match try_fetch_board_codes_with_total(&client, &url) {
                Ok((c, t)) => {
                    codes = c;
                    total = t;
                    break;
                }
                Err(e) => {
                    last_err = Some(e);
                    if attempt < 2 {
                        std::thread::sleep(std::time::Duration::from_millis(500 * (attempt as u64 + 1)));
                    }
                }
            }
        }

        if let Some(e) = last_err {
            return Err(e);
        }

        let page_count = codes.len();
        all_codes.extend(codes);

        // 如果已获取全部或本页无数据，退出循环
        if all_codes.len() >= total || page_count == 0 {
            break;
        }

        page += 1;
    }

    Ok(all_codes)
}

fn try_fetch_board_codes_with_total(client: &reqwest::blocking::Client, url: &str) -> Result<(Vec<String>, usize)> {
    let resp = client.get(url)
        .header("Referer", "https://quote.eastmoney.com/")
        .header("Accept", "*/*")
        .send()?;
    let body = resp.text()?;

    if body.is_empty() {
        return Ok((Vec::new(), 0));
    }

    let json: serde_json::Value = serde_json::from_str(&body)
        .context("解析东方财富股票列表失败")?;

    let total = json.get("data")
        .and_then(|d| d.get("total"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as usize;

    let diff = json.get("data")
        .and_then(|d| d.get("diff"))
        .and_then(|v| v.as_array());

    let Some(diff) = diff else {
        return Ok((Vec::new(), total));
    };

    let codes: Vec<String> = diff.iter()
        .filter_map(|item| item.get("f12").and_then(|v| v.as_str()).map(|s| s.to_string()))
        .filter(|c| !c.is_empty())
        .collect();

    Ok((codes, total))
}

/// 板块在线信息（含按级别的本地统计）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BoardOnlineInfo {
    pub id: String,
    pub name: String,
    pub total_count: usize,                              // 在线总股票数
    pub local_count: usize,                              // 本地有日线数据的股票数（兼容旧前端）
    pub level_counts: std::collections::HashMap<String, usize>,  // 每个级别的本地股票数
}

/// 从东方财富在线 API 获取指定板块的股票总数（轻量级，仅请求1条数据取 total）
pub fn fetch_board_online_count(board: &str) -> Result<usize> {
    let fs = match board {
        "sh_main" => "m:1+t:2",
        "sz_main" => "m:0+t:6",
        "gem" => "m:0+t:80",
        "star" => "m:1+t:23",
        "bse" => "m:0+t:81",
        _ => return Err(anyhow::anyhow!("未知板块: {}", board)),
    };

    // 只请求1条数据，从 data.total 获取总数
    let url = format!(
        "https://push2.eastmoney.com/api/qt/clist/get?pn=1&pz=1&np=1&ut=bd1d9ddb04089700cf9c27f6f7426281&fltt=2&invt=2&fid=f3&fs={}&fields=f12",
        fs
    );

    let client = build_eastmoney_client()?;

    // 最多重试 3 次，提升可靠性
    let mut last_err = None;
    for attempt in 0..3 {
        match try_fetch_online_count(&client, &url) {
            Ok(total) => return Ok(total),
            Err(e) => {
                last_err = Some(e);
                if attempt < 2 {
                    std::thread::sleep(std::time::Duration::from_millis(500 * (attempt as u64 + 1)));
                }
            }
        }
    }
    Err(last_err.unwrap())
}

fn try_fetch_online_count(client: &reqwest::blocking::Client, url: &str) -> Result<usize> {
    let resp = client.get(url)
        .header("Referer", "https://quote.eastmoney.com/")
        .header("Accept", "*/*")
        .send()?;
    let body = resp.text()?;

    if body.is_empty() {
        return Ok(0);
    }

    let json: serde_json::Value = serde_json::from_str(&body)
        .context("解析东方财富股票列表失败")?;

    let total = json.get("data")
        .and_then(|d| d.get("total"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as usize;

    Ok(total)
}

/// 获取板块在线信息（各板块有多少只股票）
/// 并发获取各板块在线总数，同时统计本地每个级别的股票数
pub fn get_board_online_info(data_dir: &Path) -> Vec<BoardOnlineInfo> {
    let cache_dir = data_dir.join("kline_cache");
    let all_tfs = TimeFrame::all();

    // ── 先统计每个级别目录下每个板块的本地股票数 ──
    // level_counts[board_id][dir_name] = count
    let mut level_counts: std::collections::HashMap<String, std::collections::HashMap<String, usize>> =
        std::collections::HashMap::new();
    let mut board_has_any: std::collections::HashMap<String, std::collections::HashSet<String>> =
        std::collections::HashMap::new(); // board_id -> set of codes

    for tf in all_tfs {
        let dir_name = tf_dir_name(*tf);
        let dir = cache_dir.join(dir_name);
        if !dir.exists() { continue; }
        let entries = match std::fs::read_dir(&dir) {
            Ok(e) => e,
            Err(_) => continue,
        };
        for entry in entries.flatten() {
            let code = match entry.file_name().to_str()
                .and_then(|n| n.strip_suffix(".parquet"))
            {
                Some(c) => c.to_string(),
                None => continue,
            };
            let board = classify_board(&code);
            if board == "other" { continue; }
            // level_counts[board][dir_name] += 1
            level_counts
                .entry(board.to_string())
                .or_default()
                .entry(dir_name.to_string())
                .and_modify(|c| *c += 1)
                .or_insert(1);
            // board_has_any[board].insert(code)
            board_has_any
                .entry(board.to_string())
                .or_default()
                .insert(code);
        }
    }

    let sub_boards = [
        ("sh_main", "上证主板"),
        ("sz_main", "深证主板"),
        ("gem", "创业板"),
        ("star", "科创板"),
        ("bse", "北交所"),
    ];

    // 并发获取各板块在线总数
    let handles: Vec<_> = sub_boards.iter().map(|(id, _name)| {
        let id = id.to_string();
        std::thread::spawn(move || {
            (id.clone(), fetch_board_online_count(&id))
        })
    }).collect();

    let mut online_counts: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for h in handles {
        if let Ok((id, result)) = h.join() {
            online_counts.insert(id, result.unwrap_or(0));
        }
    }

    let mut results: Vec<BoardOnlineInfo> = Vec::new();

    for (id, name) in &sub_boards {
        // local_count: 本地有任何级别数据的股票数
        let local_count = board_has_any.get(*id).map(|s| s.len()).unwrap_or(0);
        let online_count = *online_counts.get(*id).unwrap_or(&0);
        let lv_counts = level_counts.remove(*id).unwrap_or_default();
        results.push(BoardOnlineInfo {
            id: id.to_string(),
            name: name.to_string(),
            total_count: online_count,
            local_count,
            level_counts: lv_counts,
        });
    }

    // 全 A 股 = 各子板块汇总
    let all_online_count: usize = results.iter().map(|r| r.total_count).sum();
    let all_local_count: usize = results.iter().map(|r| r.local_count).sum();

    // 汇总各级别计数
    let mut all_level_counts: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for r in &results {
        for (lv, cnt) in &r.level_counts {
            *all_level_counts.entry(lv.clone()).or_insert(0) += cnt;
        }
    }

    results.push(BoardOnlineInfo {
        id: "all_a".to_string(),
        name: "全 A 股".to_string(),
        total_count: all_online_count,
        local_count: all_local_count,
        level_counts: all_level_counts,
    });

    results
}

/// 获取指定板块的股票代码列表（优先从在线 API 获取）
pub fn get_stock_codes_by_board(_data_dir: &Path, board: &str) -> Vec<String> {
    fetch_board_stock_codes(board).unwrap_or_else(|_| Vec::new())
}

// ═══════════════════════════════════════════════════════════
//  校验类型
// ═══════════════════════════════════════════════════════════

/// 单条校验发现的问题
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationIssue {
    pub severity: String,   // "error" | "warning" | "info"
    pub category: String,   // "ohlc" | "gap" | "zero" | "cross_source" | "count"
    pub row_index: Option<usize>,
    pub datetime: Option<String>,
    pub message: String,
}

/// 单只股票单级别的校验结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidateLevelResult {
    pub level: String,
    pub total_rows: usize,
    pub issues: Vec<ValidationIssue>,
    pub score: f64,         // 0.0~1.0, 1.0 = 完美
}

/// 单只股票的完整校验结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidateStockResult {
    pub symbol: String,
    pub levels: Vec<ValidateLevelResult>,
    pub overall_score: f64,
}

// ═══════════════════════════════════════════════════════════
//  股票代码转换
// ═══════════════════════════════════════════════════════════

/// 股票代码转新浪 symbol (sz000001, sh600000)
fn code_to_sina(code: &str) -> String {
    if code.starts_with('6') || code.starts_with('9') {
        format!("sh{}", code)
    } else if code.starts_with('0') || code.starts_with('3') {
        format!("sz{}", code)
    } else if code.starts_with('4') || code.starts_with('8') {
        format!("bj{}", code)
    } else {
        format!("sz{}", code)
    }
}

/// 股票代码转腾讯 symbol (0sz000001, 0sh600000)
fn code_to_tencent(code: &str) -> String {
    if code.starts_with('6') || code.starts_with('9') {
        format!("sh{}", code)
    } else if code.starts_with('0') || code.starts_with('3') {
        format!("sz{}", code)
    } else if code.starts_with('4') || code.starts_with('8') {
        format!("bj{}", code)
    } else {
        format!("sz{}", code)
    }
}

/// 股票代码转网易代码 (0开头不变, 6开头加1: 0000001, 1600000)
fn code_to_netease(code: &str) -> String {
    if code.starts_with('6') || code.starts_with('9') {
        format!("1{}", code)
    } else if code.starts_with('0') || code.starts_with('3') {
        format!("0{}", code)
    } else if code.starts_with('4') || code.starts_with('8') {
        format!("0{}", code)
    } else {
        format!("0{}", code)
    }
}

/// 股票代码转东方财富 secid (上海: 1.600000, 深圳: 0.000001, 北京: 0.430001)
fn code_to_eastmoney(code: &str) -> String {
    if code.starts_with('6') || code.starts_with('9') {
        format!("1.{}", code) // 上海
    } else if code.starts_with('0') || code.starts_with('3') {
        format!("0.{}", code) // 深圳
    } else if code.starts_with('4') || code.starts_with('8') {
        format!("0.{}", code) // 北京(暂归深圳)
    } else {
        format!("0.{}", code)
    }
}

/// 股票代码转 Tushare ts_code (600000.SH, 000001.SZ, 430001.BJ)
fn code_to_tushare(code: &str) -> String {
    if code.starts_with('6') || code.starts_with('9') {
        format!("{}.SH", code)
    } else if code.starts_with('0') || code.starts_with('3') {
        format!("{}.SZ", code)
    } else if code.starts_with('4') || code.starts_with('8') {
        format!("{}.BJ", code)
    } else {
        format!("{}.SH", code)
    }
}

// ═══════════════════════════════════════════════════════════
//  数据源 1: 新浪财经
// ═══════════════════════════════════════════════════════════
fn build_http_client() -> Result<reqwest::blocking::Client> {
    let mut headers = reqwest::header::HeaderMap::new();
    headers.insert("Referer", "https://finance.sina.com.cn/".parse().unwrap());

    Ok(reqwest::blocking::Client::builder()
        .user_agent("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36")
        .timeout(std::time::Duration::from_secs(15))
        .default_headers(headers)
        .build()?)
}

/// 从新浪获取 K 线数据
/// since: 可选的增量起始日期 (格式 "2024-01-01")，新浪 API 不支持日期过滤，客户端侧裁剪
fn fetch_sina_kline(sina_symbol: &str, scale: u32, datalen: u32, since: Option<&str>) -> Result<Vec<KlineRecord>> {
    let url = format!(
        "https://money.finance.sina.com.cn/quotes_service/api/json_v2.php/CN_MarketData.getKLineData?symbol={}&scale={}&ma=no&datalen={}",
        sina_symbol, scale, datalen
    );

    let client = build_http_client()?;
    let resp = client.get(&url).send()?;
    let body = resp.text()?;

    if body.is_empty() || body == "null" {
        return Ok(Vec::new());
    }

    let data: Vec<SinaKlineItem> = serde_json::from_str(&body)
        .context("解析新浪 K 线数据失败")?;

    let records: Vec<KlineRecord> = data.iter().map(|item| KlineRecord {
        datetime: item.day.clone(),
        open: item.open.parse().unwrap_or(0.0),
        high: item.high.parse().unwrap_or(0.0),
        low: item.low.parse().unwrap_or(0.0),
        close: item.close.parse().unwrap_or(0.0),
        volume: item.volume.parse().unwrap_or(0),
    }).collect();

    // 新浪 API 不支持日期过滤，客户端侧裁剪增量数据
    Ok(if let Some(since_dt) = since {
        records.into_iter().filter(|r| r.datetime.as_str() >= since_dt).collect()
    } else {
        records
    })
}

// ═══════════════════════════════════════════════════════════
//  数据源 2: 腾讯财经
// ═══════════════════════════════════════════════════════════

/// 从腾讯获取 K 线数据
/// API: https://web.ifzq.gtimg.cn/appstock/app/fqkline/get?
///      param=sh600000,day,2023-01-01,2025-12-31,2000,qfq
/// since: 可选的增量起始日期 (格式 "2024-01-01")，通过 API 参数传递
fn fetch_tencent_kline(tencent_symbol: &str, klt: &str, since: Option<&str>) -> Result<Vec<KlineRecord>> {
    let start = since.unwrap_or("2020-01-01");
    let param = format!("{},{},{},,2000,qfq", tencent_symbol, klt, start);
    let url = format!(
        "https://web.ifzq.gtimg.cn/appstock/app/fqkline/get?param={}",
        param
    );

    let client = build_http_client()?;
    let resp = client.get(&url).send()?;
    let body = resp.text()?;

    if body.is_empty() {
        return Ok(Vec::new());
    }

    // 腾讯返回 JSON 结构: { "data": { "sh600000": { "qfqday": [[date,open,close,high,low,volume], ...] } } }
    let json: serde_json::Value = serde_json::from_str(&body)
        .context("解析腾讯 K 线数据失败")?;

    // 找到实际数据数组
    let data_obj = json.get("data")
        .and_then(|d| d.get(tencent_symbol));

    let arr = data_obj
        .and_then(|d| {
            // 优先 qfq 前复权数据
            d.get("qfqday")
                .or_else(|| d.get("day"))
                .or_else(|| d.get("qfqweek"))
                .or_else(|| d.get("week"))
                .or_else(|| d.get("qfqmonth"))
                .or_else(|| d.get("month"))
        })
        .and_then(|v| v.as_array());

    let Some(arr) = arr else {
        return Ok(Vec::new());
    };

    let mut records = Vec::new();
    for item in arr {
        if let Some(row) = item.as_array() {
            if row.len() >= 6 {
                records.push(KlineRecord {
                    datetime: row[0].as_str().unwrap_or("").to_string(),
                    open: row[1].as_str().and_then(|s| s.parse().ok()).unwrap_or(0.0),
                    close: row[2].as_str().and_then(|s| s.parse().ok()).unwrap_or(0.0),
                    high: row[3].as_str().and_then(|s| s.parse().ok()).unwrap_or(0.0),
                    low: row[4].as_str().and_then(|s| s.parse().ok()).unwrap_or(0.0),
                    volume: row[5].as_str().and_then(|s| s.parse().ok()).unwrap_or(0),
                });
            }
        }
    }

    Ok(records)
}

// ═══════════════════════════════════════════════════════════
//  数据源 3: 网易财经
// ═══════════════════════════════════════════════════════════

/// 从网易获取 K 线数据
/// API: http://quotes.money.163.com/service/chddata.html?code=0600000&start=20230101&end=20251231&fields=TCLOSE;HIGH;LOW;TOPEN;VOTURNOVER
/// since: 可选的增量起始日期 (格式 "20240101"，无连字符)，通过 API start 参数传递
fn fetch_netease_kline(netease_code: &str, klt: &str, since: Option<&str>) -> Result<Vec<KlineRecord>> {
    let start = since.unwrap_or("20200101");
    let url = format!(
        "http://quotes.money.163.com/service/chddata.html?code={}&start={}&end=20991231&fields=TCLOSE;HIGH;LOW;TOPEN;VOTURNOVER&klt={}",
        netease_code, start, klt
    );

    let client = build_http_client()?;
    let resp = client.get(&url).send()?;
    let body = resp.text()?;

    if body.is_empty() {
        return Ok(Vec::new());
    }

    // 网易返回 CSV 格式:
    // 日期,股票代码,名称,收盘价,最高价,最低价,开盘价,成交量
    // 注意：网易数据是倒序的（最新在前）
    let mut records = Vec::new();
    for line in body.lines().skip(1) {  // 跳过表头
        let cols: Vec<&str> = line.split(',').collect();
        if cols.len() < 8 {
            continue;
        }

        let dt = cols[0].trim().replace('/', "-");
        if dt.is_empty() || dt.len() < 8 {
            continue;
        }

        let close: f64 = cols[3].parse().unwrap_or(0.0);
        let high: f64 = cols[4].parse().unwrap_or(0.0);
        let low: f64 = cols[5].parse().unwrap_or(0.0);
        let open: f64 = cols[6].parse().unwrap_or(0.0);
        let volume: i64 = cols[7].parse().unwrap_or(0);

        // 跳过停牌日 (全部值为 0)
        if close == 0.0 && open == 0.0 {
            continue;
        }

        records.push(KlineRecord {
            datetime: dt,
            open,
            high,
            low,
            close,
            volume,
        });
    }

    // 网易返回数据从新到旧排序，需要反转为时间正序
    records.reverse();
    Ok(records)
}


// ═══════════════════════════════════════════════════════════
//  数据源 4: 东方财富 (分钟级最优)
// ═══════════════════════════════════════════════════════════

/// 从东方财富获取 K 线数据
/// API: http://push2his.eastmoney.com/api/qt/stock/kline/get
/// klt: 101=日线, 60=60分钟, 30=30分钟, 15=15分钟, 5=5分钟, 1=1分钟
/// beg: 可选的增量起始日期 (格式 "20240101"，无连字符)，通过 API beg 参数传递
fn fetch_eastmoney_kline(secid: &str, klt: &str, lmt: u32, beg: Option<&str>) -> Result<Vec<KlineRecord>> {
    let beg_param = beg.unwrap_or("19900101");
    let url = "http://push2his.eastmoney.com/api/qt/stock/kline/get";
    let params = format!(
        "secid={}&ut=fa5fd1943c7b386f172d6893dbfba10b&fields1=f1,f2,f3,f4,f5,f6&fields2=f51,f52,f53,f54,f55,f56,f57,f58,f59,f60,f61&klt={}&fqt=1&beg={}&end=20500101&lmt={}",
        secid, klt, beg_param, lmt
    );
    let full_url = format!("{}?{}", url, params);

    let client = reqwest::blocking::Client::builder()
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/91.0.4472.124 Safari/537.36")
        .timeout(std::time::Duration::from_secs(15))
        .build()?;

    let resp = client
        .get(&full_url)
        .header("Referer", "http://quote.eastmoney.com/")
        .send()?;
    let body = resp.text()?;

    if body.is_empty() {
        return Ok(Vec::new());
    }

    // 东方财富返回 JSON: { "data": { "klines": ["2024-01-02,10.5,11.2,11.5,10.3,123456", ...] } }
    let json: serde_json::Value = serde_json::from_str(&body)
        .context("解析东方财富 K 线数据失败")?;

    let klines = json.get("data")
        .and_then(|d| d.get("klines"))
        .and_then(|v| v.as_array());

    let Some(klines) = klines else {
        return Ok(Vec::new());
    };

    let mut records = Vec::new();
    for item in klines {
        if let Some(line) = item.as_str() {
            let parts: Vec<&str> = line.split(',').collect();
            if parts.len() >= 6 {
                records.push(KlineRecord {
                    datetime: parts[0].to_string(),
                    open: parts[1].parse().unwrap_or(0.0),
                    close: parts[2].parse().unwrap_or(0.0),
                    high: parts[3].parse().unwrap_or(0.0),
                    low: parts[4].parse().unwrap_or(0.0),
                    volume: parts[5].parse().unwrap_or(0),
                });
            }
        }
    }

    Ok(records)
}

// ═══════════════════════════════════════════════════════════
//  数据源 5: Tushare (数据质量最高)
// ═══════════════════════════════════════════════════════════

/// 从 Tushare 获取 K 线数据
/// API: https://api.tushare.pro
/// 支持日线/周线/月线，分钟线需要更高积分
/// since: 可选的增量起始日期 (格式 "20240101"，无连字符)，通过 API start_date 参数传递
fn fetch_tushare_kline(ts_code: &str, freq: &str, since: Option<&str>) -> Result<Vec<KlineRecord>> {
    // 选择 Tushare 接口名称
    let api_name = match freq {
        "D" => "daily",
        "W" => "weekly",
        "M" => "monthly",
        _ => "stk_mins", // 分钟线
    };

    let end_date = chrono::Local::now().format("%Y%m%d").to_string();
    let default_start = {
        let five_years_ago = chrono::Local::now() - chrono::Duration::try_days(365 * 5).unwrap_or_default();
        five_years_ago.format("%Y%m%d").to_string()
    };
    let start_date = since.unwrap_or(&default_start);

    // 构造请求体
    let params = if freq == "D" || freq == "W" || freq == "M" {
        serde_json::json!({
            "api_name": api_name,
            "token": TUSHARE_TOKEN,
            "params": {
                "ts_code": ts_code,
                "start_date": start_date,
                "end_date": end_date,
            },
            "fields": "trade_date,open,high,low,close,vol"
        })
    } else {
        // 分钟线 — 需要更高积分，失败时自动降级为日线
        serde_json::json!({
            "api_name": api_name,
            "token": TUSHARE_TOKEN,
            "params": {
                "ts_code": ts_code,
                "start_date": format!("{} 09:00:00", start_date),
                "end_date": format!("{} 15:00:00", end_date),
                "freq": freq,
            },
            "fields": "trade_time,open,high,low,close,vol"
        })
    };

    let client = reqwest::blocking::Client::builder()
        .user_agent("Mozilla/5.0")
        .timeout(std::time::Duration::from_secs(20))
        .build()?;

    let resp = client
        .post("https://api.tushare.pro")
        .json(&params)
        .send()?;
    let body = resp.text()?;

    if body.is_empty() {
        return Ok(Vec::new());
    }

    let json: serde_json::Value = serde_json::from_str(&body)
        .context("解析 Tushare 响应失败")?;

    // 检查 Tushare 返回码
    let code = json.get("code").and_then(|c| c.as_i64()).unwrap_or(-1);
    if code != 0 {
        // 分钟线可能积分不足，尝试降级为日线
        if freq != "D" && freq != "W" && freq != "M" {
            return fetch_tushare_kline(ts_code, "D", since);
        }
        let msg = json.get("msg").and_then(|m| m.as_str()).unwrap_or("unknown error");
        anyhow::bail!("Tushare error: {}", msg);
    }

    // 解析数据
    let fields = json.get("data")
        .and_then(|d| d.get("fields"))
        .and_then(|f| f.as_array());

    let items = json.get("data")
        .and_then(|d| d.get("items"))
        .and_then(|f| f.as_array());

    let Some(fields) = fields else { return Ok(Vec::new()); };
    let Some(items) = items else { return Ok(Vec::new()); };

    // 找到各字段索引
    let field_names: Vec<&str> = fields.iter()
        .filter_map(|f| f.as_str())
        .collect();

    let date_idx = field_names.iter().position(|f| *f == "trade_date" || *f == "trade_time");
    let open_idx = field_names.iter().position(|f| *f == "open");
    let high_idx = field_names.iter().position(|f| *f == "high");
    let low_idx = field_names.iter().position(|f| *f == "low");
    let close_idx = field_names.iter().position(|f| *f == "close");
    let vol_idx = field_names.iter().position(|f| *f == "vol" || *f == "volume");

    let mut records = Vec::new();
    for item in items {
        if let Some(row) = item.as_array() {
            let get_f64 = |idx: Option<usize>| -> f64 {
                idx.and_then(|i| row.get(i)).and_then(|v| v.as_f64()).unwrap_or(0.0)
            };
            let get_i64 = |idx: Option<usize>| -> i64 {
                idx.and_then(|i| row.get(i)).and_then(|v| v.as_f64()).unwrap_or(0.0) as i64
            };
            let get_string = |idx: Option<usize>| -> String {
                idx.and_then(|i| row.get(i))
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string()
            };

            let dt = get_string(date_idx);
            if dt.is_empty() {
                continue;
            }

            // 日期格式转换: "20240102" -> "2024-01-02"
            let datetime = if dt.len() == 8 && !dt.contains('-') {
                format!("{}-{}-{}", &dt[0..4], &dt[4..6], &dt[6..8])
            } else if dt.len() > 10 && dt.contains(' ') {
                // 分钟线格式: "2024-01-02 09:30:00"
                dt
            } else {
                dt
            };

            let open = get_f64(open_idx);
            let close = get_f64(close_idx);
            let high = get_f64(high_idx);
            let low = get_f64(low_idx);
            let volume = get_i64(vol_idx);

            if close == 0.0 && open == 0.0 {
                continue;
            }

            records.push(KlineRecord {
                datetime,
                open,
                high,
                low,
                close,
                volume,
            });
        }
    }

    Ok(records)
}

// ═══════════════════════════════════════════════════════════
//  多数据源协同获取
// ═══════════════════════════════════════════════════════════
//  多数据源协同获取
// ═══════════════════════════════════════════════════════════

/// 数据源获取结果：记录了来自哪个源
struct FetchResult {
    records: Vec<KlineRecord>,
    source: String, // "sina" | "tencent" | "eastmoney" | "tushare" | "netease" | ...
}

/// 多数据源协同获取 K 线数据
/// 按级别动态调整数据源优先级：
/// - 日线/周线/月线: 新浪 → 腾讯 → Tushare → 网易 → 东方财富(日线)
/// - 分钟级: 东方财富(最优) → 新浪 → 腾讯 → Tushare
/// since: 可选的增量起始日期 (格式 "2024-01-01"，有连字符)，各数据源内部按需转换格式
fn fetch_kline_multi_source(
    code: &str,
    tf: TimeFrame,
    since: Option<&str>,
) -> Result<FetchResult> {
    let sina_symbol = code_to_sina(code);
    let tencent_symbol = code_to_tencent(code);
    let netease_code = code_to_netease(code);
    let em_secid = code_to_eastmoney(code);
    let ts_code = code_to_tushare(code);

    // 各数据源需要的日期格式不同，提前转换
    // since 格式: "2024-01-01"（有连字符）
    // 东财/网易/Tushare: "20240101"（无连字符）
    let since_nohyphen = since.map(|s| s.replace("-", ""));
    let since_sina = since;                       // "2024-01-01"
    let since_tencent = since;                    // "2024-01-01"
    let since_netease = since_nohyphen.as_deref();// "20240101"
    let since_eastmoney = since_nohyphen.as_deref(); // "20240101"
    let since_tushare = since_nohyphen.as_deref();   // "20240101"

    let cfg = match tf.source_config() {
        Some(c) => c,
        None => return Ok(FetchResult { records: Vec::new(), source: "none".into() }),
    };

    let is_daily_or_above = matches!(tf, TimeFrame::M | TimeFrame::W | TimeFrame::D);
    let is_minute = matches!(tf, TimeFrame::F60 | TimeFrame::F30 | TimeFrame::F15 | TimeFrame::F5 | TimeFrame::F1);

    if is_minute {
        // ═══ 分钟级优先级: 东方财富(最多10000条) → 新浪 → 腾讯 → Tushare ═══

        // ─── 1. 东方财富 (分钟级最优) ───
        if let Some(klt) = cfg.eastmoney_klt {
            match fetch_eastmoney_kline(&em_secid, klt, cfg.eastmoney_lmt, since_eastmoney) {
                Ok(data) if !data.is_empty() => {
                    return Ok(FetchResult { records: data, source: "eastmoney".into() });
                }
                _ => {}
            }
        }

        // ─── 2. 新浪 ───
        match fetch_sina_kline(&sina_symbol, cfg.sina_scale, cfg.sina_datalen, since_sina) {
            Ok(data) if !data.is_empty() => {
                return Ok(FetchResult { records: data, source: "sina".into() });
            }
            _ => {}
        }

        // ─── 3. 腾讯 ───
        if let Some(klt) = cfg.tencent_klt {
            match fetch_tencent_kline(&tencent_symbol, klt, since_tencent) {
                Ok(data) if !data.is_empty() => {
                    return Ok(FetchResult { records: data, source: "tencent".into() });
                }
                _ => {}
            }
        }

        // ─── 4. Tushare (分钟线可能积分不足，内部自动降级日线) ───
        if let Some(freq) = cfg.tushare_freq {
            match fetch_tushare_kline(&ts_code, freq, since_tushare) {
                Ok(data) if !data.is_empty() => {
                    return Ok(FetchResult { records: data, source: "tushare".into() });
                }
                _ => {}
            }
        }

        // ─── 5. 新浪兜底(增大请求量) ───
        match fetch_sina_kline(&sina_symbol, cfg.sina_scale, 1000, since_sina) {
            Ok(data) if !data.is_empty() => {
                return Ok(FetchResult { records: data, source: "sina_retry".into() });
            }
            _ => {}
        }

    } else {
        // ═══ 日线/周线/月线优先级: 新浪 → 腾讯 → Tushare → 网易 → 东方财富(仅日线) ═══

        // ─── 1. 新浪 (月线不支持,需日线重采样) ───
        if tf != TimeFrame::M {
            match fetch_sina_kline(&sina_symbol, cfg.sina_scale, cfg.sina_datalen, since_sina) {
                Ok(data) if !data.is_empty() => {
                    return Ok(FetchResult { records: data, source: "sina".into() });
                }
                _ => {}
            }
        }

        // ─── 2. 腾讯 ───
        if let Some(klt) = cfg.tencent_klt {
            match fetch_tencent_kline(&tencent_symbol, klt, since_tencent) {
                Ok(data) if !data.is_empty() => {
                    return Ok(FetchResult { records: data, source: "tencent".into() });
                }
                _ => {}
            }
        }

        // ─── 3. Tushare (全级别，数据质量最高) ───
        if let Some(freq) = cfg.tushare_freq {
            match fetch_tushare_kline(&ts_code, freq, since_tushare) {
                Ok(data) if !data.is_empty() => {
                    return Ok(FetchResult { records: data, source: "tushare".into() });
                }
                _ => {}
            }
        }

        // ─── 4. 网易 (仅日线/周线/月线) ───
        if is_daily_or_above {
            if let Some(klt) = cfg.netease_klt {
                match fetch_netease_kline(&netease_code, klt, since_netease) {
                    Ok(data) if !data.is_empty() => {
                        return Ok(FetchResult { records: data, source: "netease".into() });
                    }
                    _ => {}
                }
            }
        }

        // ─── 5. 东方财富 (仅日线) ───
        if let Some(klt) = cfg.eastmoney_klt {
            match fetch_eastmoney_kline(&em_secid, klt, cfg.eastmoney_lmt, since_eastmoney) {
                Ok(data) if !data.is_empty() => {
                    return Ok(FetchResult { records: data, source: "eastmoney".into() });
                }
                _ => {}
            }
        }

        // ─── 6. 日线兜底 (周线失败时从日线重采样) ───
        if tf == TimeFrame::D || tf == TimeFrame::W {
            match fetch_sina_kline(&sina_symbol, 240, 2000, None) {
                Ok(data) if !data.is_empty() => {
                    return Ok(FetchResult { records: data, source: "sina_daily_fallback".into() });
                }
                _ => {}
            }
            match fetch_tencent_kline(&tencent_symbol, "day", None) {
                Ok(data) if !data.is_empty() => {
                    return Ok(FetchResult { records: data, source: "tencent_daily_fallback".into() });
                }
                _ => {}
            }
            match fetch_netease_kline(&netease_code, "day", None) {
                Ok(data) if !data.is_empty() => {
                    return Ok(FetchResult { records: data, source: "netease_daily_fallback".into() });
                }
                _ => {}
            }
        }
    }

    Ok(FetchResult { records: Vec::new(), source: "none".into() })
}

// ═══════════════════════════════════════════════════════════
//  数据转换与重采样
// ═══════════════════════════════════════════════════════════

/// 过滤起始日期
fn filter_by_start(records: &[KlineRecord], start_date: &str) -> Vec<KlineRecord> {
    records.iter()
        .filter(|r| r.datetime.as_str() >= start_date)
        .cloned()
        .collect()
}

/// 将日期字符串减去一天 (格式 "2024-01-15" → "2024-01-14")
fn subtract_one_day(date_str: &str) -> String {
    let s = if date_str.len() > 10 { &date_str[..10] } else { date_str };
    NaiveDate::parse_from_str(s, "%Y-%m-%d")
        .ok()
        .and_then(|d: NaiveDate| d.checked_sub_signed(chrono::Duration::try_days(1).unwrap_or_default()))
        .map(|d| d.format("%Y-%m-%d").to_string())
        .unwrap_or_else(|| date_str.to_string())
}

/// 从日线重采样为周线
fn resample_to_weekly(daily: &[KlineRecord]) -> Vec<KlineRecord> {
    let mut weekly: std::collections::BTreeMap<String, KlineRecord> = std::collections::BTreeMap::new();
    for r in daily {
        let dt_str = &r.datetime;
        let week_key = if dt_str.len() >= 10 {
            match NaiveDate::parse_from_str(&dt_str[..10], "%Y-%m-%d") {
                Ok(dt) => {
                    use chrono::Datelike;
                    let weekday = dt.weekday().num_days_from_monday() as i64;
                    let monday = dt - chrono::Duration::try_days(weekday).unwrap_or_default();
                    monday.format("%Y-%m-%d").to_string()
                }
                Err(_) => dt_str[..7].to_string(),
            }
        } else {
            dt_str.clone()
        };

        weekly.entry(week_key).and_modify(|w| {
            w.high = w.high.max(r.high);
            w.low = w.low.min(r.low);
            w.close = r.close;
            w.volume += r.volume;
            w.datetime = r.datetime.clone();
        }).or_insert_with(|| KlineRecord {
            datetime: r.datetime.clone(),
            open: r.open,
            high: r.high,
            low: r.low,
            close: r.close,
            volume: r.volume,
        });
    }
    weekly.into_values().collect()
}

/// 从日线重采样为月线
fn resample_to_monthly(daily: &[KlineRecord]) -> Vec<KlineRecord> {
    let mut monthly: std::collections::BTreeMap<String, KlineRecord> = std::collections::BTreeMap::new();
    for r in daily {
        let month_key = if r.datetime.len() >= 7 {
            r.datetime[..7].to_string()
        } else {
            r.datetime.clone()
        };

        monthly.entry(month_key).and_modify(|m| {
            m.high = m.high.max(r.high);
            m.low = m.low.min(r.low);
            m.close = r.close;
            m.volume += r.volume;
            m.datetime = r.datetime.clone();
        }).or_insert_with(|| KlineRecord {
            datetime: r.datetime.clone(),
            open: r.open,
            high: r.high,
            low: r.low,
            close: r.close,
            volume: r.volume,
        });
    }
    monthly.into_values().collect()
}

// ═══════════════════════════════════════════════════════════
//  Parquet 读写
// ═══════════════════════════════════════════════════════════

fn load_existing_parquet(filepath: &Path) -> Result<Vec<KlineRecord>> {
    if !filepath.exists() {
        return Ok(Vec::new());
    }

    let df = LazyFrame::scan_parquet(filepath, ScanArgsParquet::default())?
        .collect()?;

    if df.height() == 0 {
        return Ok(Vec::new());
    }

    let col_names = df.get_column_names_str();

    let dt_name = col_names.iter()
        .find(|c| ["dt", "datetime", "date", "time", "timestamp"].contains(c))
        .unwrap_or(&"");

    let open_name = col_names.iter().find(|c| ["open", "Open"].contains(c)).unwrap_or(&"");
    let high_name = col_names.iter().find(|c| ["high", "High"].contains(c)).unwrap_or(&"");
    let low_name = col_names.iter().find(|c| ["low", "Low"].contains(c)).unwrap_or(&"");
    let close_name = col_names.iter().find(|c| ["close", "Close"].contains(c)).unwrap_or(&"");
    let vol_name = col_names.iter().find(|c| ["vol", "volume", "Volume"].contains(c)).unwrap_or(&"");

    let dt_col = df.column(dt_name).ok();
    let open_col = df.column(open_name).ok();
    let high_col = df.column(high_name).ok();
    let low_col = df.column(low_name).ok();
    let close_col = df.column(close_name).ok();
    let vol_col = df.column(vol_name).ok();

    let mut records = Vec::new();
    for i in 0..df.height() {
        let dt = dt_col.as_ref().map(|c| crate::kline_manager::KLineManager::extract_datetime(c, i)).unwrap_or_default();
        let open = open_col.as_ref().map(|c| crate::kline_manager::KLineManager::extract_f64(c, i)).unwrap_or(0.0);
        let high = high_col.as_ref().map(|c| crate::kline_manager::KLineManager::extract_f64(c, i)).unwrap_or(0.0);
        let low = low_col.as_ref().map(|c| crate::kline_manager::KLineManager::extract_f64(c, i)).unwrap_or(0.0);
        let close = close_col.as_ref().map(|c| crate::kline_manager::KLineManager::extract_f64(c, i)).unwrap_or(0.0);
        let vol = vol_col.as_ref().map(|c| crate::kline_manager::KLineManager::extract_f64(c, i)).unwrap_or(0.0);

        records.push(KlineRecord {
            datetime: dt,
            open,
            high,
            low,
            close,
            volume: vol as i64,
        });
    }

    Ok(records)
}

/// 合并新旧记录 (按日期去重，以新数据优先)
fn merge_records(old: &[KlineRecord], new: &[KlineRecord]) -> Vec<KlineRecord> {
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut merged = Vec::new();

    // 先插新数据
    for r in new {
        seen.insert(r.datetime.clone());
        merged.push(r.clone());
    }
    // 再插旧数据中未被覆盖的
    for r in old {
        if seen.insert(r.datetime.clone()) {
            merged.push(r.clone());
        }
    }

    // 按日期排序
    merged.sort_by(|a, b| a.datetime.cmp(&b.datetime));
    merged
}

/// 将记录保存为 parquet
fn save_parquet(records: &[KlineRecord], filepath: &Path) -> Result<()> {
    if records.is_empty() {
        return Ok(());
    }

    let mut datetimes = Vec::new();
    let mut opens = Vec::new();
    let mut highs = Vec::new();
    let mut lows = Vec::new();
    let mut closes = Vec::new();
    let mut volumes = Vec::new();

    for r in records {
        datetimes.push(r.datetime.clone());
        opens.push(r.open);
        highs.push(r.high);
        lows.push(r.low);
        closes.push(r.close);
        volumes.push(r.volume as f64);
    }

    let df = DataFrame::new(vec![
        Series::new("datetime".into(), datetimes).into(),
        Series::new("Open".into(), opens).into(),
        Series::new("High".into(), highs).into(),
        Series::new("Low".into(), lows).into(),
        Series::new("Close".into(), closes).into(),
        Series::new("Volume".into(), volumes).into(),
    ])?;

    std::fs::create_dir_all(filepath.parent().context("parquet 路径无效")?)?;

    let file = std::fs::File::create(filepath)?;
    ParquetWriter::new(file).finish(&mut df.clone())?;

    Ok(())
}

// ═══════════════════════════════════════════════════════════
//  数据校验
// ═══════════════════════════════════════════════════════════

/// 校验单个级别 parquet 数据
pub fn validate_stock_level(
    data_dir: &Path,
    symbol: &str,
    tf: TimeFrame,
) -> ValidateLevelResult {
    let dir_name = tf_dir_name(tf);
    let filepath = data_dir.join("kline_cache").join(dir_name).join(format!("{}.parquet", symbol));

    let mut issues = Vec::new();

    let records = match load_existing_parquet(&filepath) {
        Ok(r) => r,
        Err(e) => {
            return ValidateLevelResult {
                level: dir_name.to_string(),
                total_rows: 0,
                issues: vec![ValidationIssue {
                    severity: "error".into(),
                    category: "count".into(),
                    row_index: None,
                    datetime: None,
                    message: format!("无法加载 parquet: {}", e),
                }],
                score: 0.0,
            };
        }
    };

    let total = records.len();
    if total == 0 {
        return ValidateLevelResult {
            level: dir_name.to_string(),
            total_rows: 0,
            issues: vec![ValidationIssue {
                severity: "warning".into(),
                category: "count".into(),
                row_index: None,
                datetime: None,
                message: "数据为空".into(),
            }],
            score: 0.0,
        };
    }

    let mut error_count = 0u32;
    let mut warning_count = 0u32;

    // ─── 1. OHLC 逻辑校验 ───
    for (i, r) in records.iter().enumerate() {
        // high >= max(open, close, low)
        let max_price = r.open.max(r.close).max(r.low);
        if r.high < max_price - 0.001 {
            issues.push(ValidationIssue {
                severity: "error".into(),
                category: "ohlc".into(),
                row_index: Some(i),
                datetime: Some(r.datetime.clone()),
                message: format!("High({:.2}) < max(O,C,L)={:.2}", r.high, max_price),
            });
            error_count += 1;
        }

        // low <= min(open, close, high)
        let min_price = r.open.min(r.close).min(r.high);
        if r.low > min_price + 0.001 {
            issues.push(ValidationIssue {
                severity: "error".into(),
                category: "ohlc".into(),
                row_index: Some(i),
                datetime: Some(r.datetime.clone()),
                message: format!("Low({:.2}) > min(O,C,H)={:.2}", r.low, min_price),
            });
            error_count += 1;
        }

        // 价格为负
        if r.open < 0.0 || r.high < 0.0 || r.low < 0.0 || r.close < 0.0 {
            issues.push(ValidationIssue {
                severity: "error".into(),
                category: "ohlc".into(),
                row_index: Some(i),
                datetime: Some(r.datetime.clone()),
                message: format!("价格为负: O={:.2} H={:.2} L={:.2} C={:.2}", r.open, r.high, r.low, r.close),
            });
            error_count += 1;
        }

        // 成交量为负
        if r.volume < 0 {
            issues.push(ValidationIssue {
                severity: "error".into(),
                category: "zero".into(),
                row_index: Some(i),
                datetime: Some(r.datetime.clone()),
                message: format!("成交量为负: {}", r.volume),
            });
            error_count += 1;
        }

        // 全零行（可能停牌但数据异常）
        if r.open == 0.0 && r.high == 0.0 && r.low == 0.0 && r.close == 0.0 && r.volume == 0 {
            issues.push(ValidationIssue {
                severity: "warning".into(),
                category: "zero".into(),
                row_index: Some(i),
                datetime: Some(r.datetime.clone()),
                message: "OHLCV 全部为零（停牌日?）".into(),
            });
            warning_count += 1;
        }

        // 成交量为0但价格变化（非停牌的零成交异常）
        if r.volume == 0 && (r.close - r.open).abs() > 0.01 && r.open > 0.0 {
            issues.push(ValidationIssue {
                severity: "warning".into(),
                category: "zero".into(),
                row_index: Some(i),
                datetime: Some(r.datetime.clone()),
                message: format!("成交量为0但价格有变化: O={:.2} C={:.2}", r.open, r.close),
            });
            warning_count += 1;
        }
    }

    // ─── 2. 日期连续性校验（日线/周线） ───
    if matches!(tf, TimeFrame::D | TimeFrame::W) && records.len() > 1 {
        // 简化的连续性检查：检测连续两个日期之间是否跳过了异常多的交易日
        let consecutive_gaps: Vec<(usize, &KlineRecord, &KlineRecord)> = records.windows(2)
            .enumerate()
            .filter_map(|(i, w)| {
                let d1 = parse_date_str(&w[0].datetime);
                let d2 = parse_date_str(&w[1].datetime);
                match (d1, d2) {
                    (Some(a), Some(b)) => Some((i, (b - a).num_days())),
                    _ => None,
                }
            })
            .filter_map(|(i, gap)| {
                if let Some(idx) = records.get(i) {
                    if let Some(next) = records.get(i + 1) {
                        Some((i, idx, next, gap))
                    } else {
                        None
                    }
                } else {
                    None
                }
            })
            // 需要重新做
            .map(|_| unreachable!())
            .collect();

        // 重新实现连续性检查
        for i in 1..records.len() {
            let prev = &records[i - 1];
            let curr = &records[i];
            let d1 = parse_date_str(&prev.datetime);
            let d2 = parse_date_str(&curr.datetime);

            if let (Some(a), Some(b)) = (d1, d2) {
                let gap_days = (b - a).num_days();

                // 日线：超过 5 个自然日（排除周末）可能有缺失
                // 周线：超过 14 个自然日可能有缺失
                let threshold = match tf {
                    TimeFrame::D => 5,  // 5个自然日(含周末)=5交易日应该够了
                    TimeFrame::W => 14,
                    _ => 7,
                };

                if gap_days > threshold {
                    // 排除节假日（简化：1月1日、5月1日、10月1日前后允许更大间距）
                    let is_holiday_season = prev.datetime.contains("-01-0")
                        || prev.datetime.contains("-05-0")
                        || prev.datetime.contains("-10-0");

                    let real_threshold = if is_holiday_season { threshold * 2 } else { threshold };

                    if gap_days > real_threshold {
                        issues.push(ValidationIssue {
                            severity: "warning".into(),
                            category: "gap".into(),
                            row_index: Some(i),
                            datetime: Some(curr.datetime.clone()),
                            message: format!(
                                "日期间隙过大: {} ~ {} ({}天)",
                                prev.datetime, curr.datetime, gap_days
                            ),
                        });
                        warning_count += 1;
                    }
                }
            }

            // 日期逆序
            if prev.datetime >= curr.datetime {
                issues.push(ValidationIssue {
                    severity: "error".into(),
                    category: "gap".into(),
                    row_index: Some(i),
                    datetime: Some(curr.datetime.clone()),
                    message: format!("日期逆序: {} >= {}", prev.datetime, curr.datetime),
                });
                error_count += 1;
            }
        }
    }

    // ─── 3. 点数合理性校验 ───
    // A 股日K线点数约为 250个/年（约250个交易日）
    if matches!(tf, TimeFrame::D) {
        if let (Some(first), Some(last)) = (records.first(), records.last()) {
            let d1 = parse_date_str(&first.datetime);
            let d2 = parse_date_str(&last.datetime);
            if let (Some(a), Some(b)) = (d1, d2) {
                let years = (b - a).num_days() as f64 / 365.25;
                if years > 0.5 {
                    let expected_min = (years * 200.0) as usize; // 至少200个/年
                    let expected_max = (years * 270.0) as usize; // 最多270个/年
                    if total < expected_min {
                        issues.push(ValidationIssue {
                            severity: "warning".into(),
                            category: "count".into(),
                            row_index: None,
                            datetime: None,
                            message: format!(
                                "日线点数偏少: 实际={} 预期>={} ({:.1}年数据)",
                                total, expected_min, years
                            ),
                        });
                        warning_count += 1;
                    } else if total > expected_max {
                        issues.push(ValidationIssue {
                            severity: "info".into(),
                            category: "count".into(),
                            row_index: None,
                            datetime: None,
                            message: format!(
                                "日线点数偏多: 实际={} 预期<={} ({:.1}年数据)",
                                total, expected_max, years
                            ),
                        });
                    }
                }
            }
        }
    }

    // ─── 4. 重复日期检查 ───
    let mut seen_dates: std::collections::HashSet<String> = std::collections::HashSet::new();
    for (i, r) in records.iter().enumerate() {
        if !seen_dates.insert(r.datetime.clone()) {
            issues.push(ValidationIssue {
                severity: "error".into(),
                category: "gap".into(),
                row_index: Some(i),
                datetime: Some(r.datetime.clone()),
                message: format!("重复日期: {}", r.datetime),
            });
            error_count += 1;
        }
    }

    // ─── 计算评分 ───
    let error_penalty = error_count as f64 * 0.1;
    let warning_penalty = warning_count as f64 * 0.02;
    let score = (1.0 - error_penalty - warning_penalty).max(0.0);

    ValidateLevelResult {
        level: dir_name.to_string(),
        total_rows: total,
        issues,
        score,
    }
}

/// 校验单只股票全部级别
pub fn validate_stock(
    data_dir: &Path,
    symbol: &str,
    levels: &[TimeFrame],
) -> ValidateStockResult {
    let mut level_results = Vec::new();

    for &tf in levels {
        level_results.push(validate_stock_level(data_dir, symbol, tf));
    }

    let overall_score = if level_results.is_empty() {
        0.0
    } else {
        level_results.iter().map(|r| r.score).sum::<f64>() / level_results.len() as f64
    };

    ValidateStockResult {
        symbol: symbol.to_string(),
        levels: level_results,
        overall_score,
    }
}

/// 跨源交叉校验：从两个数据源获取同级别数据，对比差异
pub fn cross_validate_stock(
    code: &str,
    tf: TimeFrame,
) -> ValidateLevelResult {
    let mut issues = Vec::new();
    let level_str = tf_dir_name(tf).to_string();

    let sina_symbol = code_to_sina(code);
    let tencent_symbol = code_to_tencent(code);

    let cfg = match tf.source_config() {
        Some(c) => c,
        None => return ValidateLevelResult {
            level: level_str,
            total_rows: 0,
            issues: vec![ValidationIssue {
                severity: "error".into(),
                category: "cross_source".into(),
                row_index: None,
                datetime: None,
                message: "不支持的时间级别".into(),
            }],
            score: 0.0,
        },
    };

    // 从新浪获取
    let sina_records = if tf != TimeFrame::M {
        fetch_sina_kline(&sina_symbol, cfg.sina_scale, cfg.sina_datalen, None).unwrap_or_default()
    } else {
        Vec::new()
    };

    // 从腾讯获取
    let tencent_records = if let Some(klt) = cfg.tencent_klt {
        fetch_tencent_kline(&tencent_symbol, klt, None).unwrap_or_default()
    } else {
        Vec::new()
    };

    if sina_records.is_empty() && tencent_records.is_empty() {
        return ValidateLevelResult {
            level: level_str,
            total_rows: 0,
            issues: vec![ValidationIssue {
                severity: "error".into(),
                category: "cross_source".into(),
                row_index: None,
                datetime: None,
                message: "所有数据源均无数据".into(),
            }],
            score: 0.0,
        };
    }

    // 如果只有一个源有数据
    if sina_records.is_empty() || tencent_records.is_empty() {
        let available = if sina_records.is_empty() { "tencent" } else { "sina" };
        return ValidateLevelResult {
            level: level_str,
            total_rows: sina_records.len().max(tencent_records.len()),
            issues: vec![ValidationIssue {
                severity: "warning".into(),
                category: "cross_source".into(),
                row_index: None,
                datetime: None,
                message: format!("仅 {} 有数据，无法交叉校验", available),
            }],
            score: 0.5,
        };
    }

    // 构建日期 → 记录的映射
    let sina_map: std::collections::HashMap<&str, &KlineRecord> = sina_records.iter()
        .map(|r| (r.datetime.as_str(), r))
        .collect();
    let tencent_map: std::collections::HashMap<&str, &KlineRecord> = tencent_records.iter()
        .map(|r| (r.datetime.as_str(), r))
        .collect();

    let total = std::cmp::max(sina_records.len(), tencent_records.len());
    let mut mismatch_count = 0u32;

    // 对比共同日期
    for (dt, sina_r) in &sina_map {
        if let Some(tencent_r) = tencent_map.get(dt) {
            let close_diff = (sina_r.close - tencent_r.close).abs();
            let high_diff = (sina_r.high - tencent_r.high).abs();
            let low_diff = (sina_r.low - tencent_r.low).abs();

            // 允许 0.5% 的差异（处理前复权差异）
            let threshold = sina_r.close * 0.005;

            if close_diff > threshold || high_diff > threshold || low_diff > threshold {
                mismatch_count += 1;
                if mismatch_count <= 10 { // 只报告前 10 个差异
                    issues.push(ValidationIssue {
                        severity: "warning".into(),
                        category: "cross_source".into(),
                        row_index: None,
                        datetime: Some(dt.to_string()),
                        message: format!(
                            "数据源差异: sina C={:.2} H={:.2} L={:.2} vs tencent C={:.2} H={:.2} L={:.2}",
                            sina_r.close, sina_r.high, sina_r.low,
                            tencent_r.close, tencent_r.high, tencent_r.low,
                        ),
                    });
                }
            }
        }
    }

    if mismatch_count > 10 {
        issues.push(ValidationIssue {
            severity: "info".into(),
            category: "cross_source".into(),
            row_index: None,
            datetime: None,
            message: format!("共 {} 处数据源差异（仅显示前10处）", mismatch_count),
        });
    }

    // 新浪有但腾讯没有的日期
    let sina_only: Vec<&str> = sina_map.keys()
        .filter(|k| !tencent_map.contains_key(*k))
        .copied()
        .collect();
    if !sina_only.is_empty() {
        issues.push(ValidationIssue {
            severity: "info".into(),
            category: "cross_source".into(),
            row_index: None,
            datetime: None,
            message: format!("新浪独有 {} 个日期, 腾讯独有 {} 个日期",
                sina_only.len(),
                tencent_map.keys().filter(|k| !sina_map.contains_key(*k)).count()
            ),
        });
    }

    let score = if total == 0 {
        0.0
    } else {
        1.0 - (mismatch_count as f64 / total as f64 * 0.5).min(1.0)
    };

    ValidateLevelResult {
        level: level_str,
        total_rows: total,
        issues,
        score,
    }
}

/// 解析日期字符串为 NaiveDate
fn parse_date_str(s: &str) -> Option<NaiveDate> {
    let s = if s.len() > 10 { &s[..10] } else { s };
    // 尝试多种格式
    NaiveDate::parse_from_str(s, "%Y-%m-%d")
        .or_else(|_| NaiveDate::parse_from_str(s, "%Y/%m/%d"))
        .ok()
}

// ═══════════════════════════════════════════════════════════
//  公共 API
// ═══════════════════════════════════════════════════════════

/// TimeFrame 到目录名的映射
fn tf_dir_name(tf: TimeFrame) -> &'static str {
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

/// 同步单只股票 K 线数据（多数据源协同，增量更新）
pub fn sync_stock(
    data_dir: &Path,
    symbol: &str,
    levels: &[TimeFrame],
    start_date: &str,
    force: bool,
) -> SyncStockResult {
    let mut results = Vec::new();
    let cache_dir = data_dir.join("kline_cache");

    // 缓存日线数据供周/月线重采样
    let mut daily_records: Option<Vec<KlineRecord>> = None;

    for &tf in levels {
        let dir_name = tf_dir_name(tf);
        let filepath = cache_dir.join(dir_name).join(format!("{}.parquet", symbol));
        let level_str = dir_name.to_string();

        // ─── 增量逻辑: 非 force 时, 以本地最后日期的前一天作为增量起始点 ───
        let inc_since: Option<String> = if !force {
            get_parquet_last_date(&filepath).map(|last_dt| {
                // 从最后日期前一天开始请求, 确保覆盖修正
                subtract_one_day(&last_dt)
            })
        } else {
            None
        };

        let result = match tf {
            TimeFrame::M => {
                // 月线从日线重采样
                // 日线的增量起始点
                let daily_since = if !force {
                    let daily_path = cache_dir.join("1d").join(format!("{}.parquet", symbol));
                    get_parquet_last_date(&daily_path).map(|last_dt| subtract_one_day(&last_dt))
                } else {
                    None
                };
                if daily_records.is_none() {
                    let fetch = fetch_kline_multi_source(symbol, TimeFrame::D, daily_since.as_deref())
                        .unwrap_or(FetchResult { records: Vec::new(), source: "none".into() });
                    if !fetch.records.is_empty() {
                        daily_records = Some(filter_by_start(&fetch.records, start_date));
                    }
                }
                match daily_records.as_ref().map(|d| resample_to_monthly(d)) {
                    Some(mut recs) => {
                        if !force {
                            if let Ok(old) = load_existing_parquet(&filepath) {
                                // 如果新数据没有新增（与旧数据最后日期相同），则跳过
                                let old_last = old.last().map(|r| r.datetime.as_str()).unwrap_or("");
                                let new_last = recs.last().map(|r| r.datetime.as_str()).unwrap_or("");
                                if old_last == new_last && !old.is_empty() {
                                    results.push(SyncLevelResult {
                                        level: level_str, status: "skip".into(),
                                        count: old.len(), source: "resample_from_daily".into(),
                                        msg: "already up to date".into(),
                                    });
                                    continue;
                                }
                                recs = merge_records(&old, &recs);
                            }
                        }
                        let count = recs.len();
                        match save_parquet(&recs, &filepath) {
                            Ok(()) => SyncLevelResult {
                                level: level_str, status: "ok".into(), count,
                                source: "resample_from_daily".into(), msg: String::new(),
                            },
                            Err(e) => SyncLevelResult {
                                level: level_str, status: "error".into(), count: 0,
                                source: String::new(), msg: e.to_string(),
                            },
                        }
                    }
                    None => SyncLevelResult {
                        level: level_str, status: "fail".into(), count: 0,
                        source: String::new(), msg: "no daily data".into(),
                    },
                }
            }
            TimeFrame::W => {
                // 周线：优先直接获取，失败则从日线重采样
                let fetch = fetch_kline_multi_source(symbol, tf, inc_since.as_deref())
                    .unwrap_or(FetchResult { records: Vec::new(), source: "none".into() });
                let records = if !fetch.records.is_empty() {
                    Some(filter_by_start(&fetch.records, start_date))
                } else {
                    // 从日线重采样 — 需要获取日线增量
                    let daily_since = if !force {
                        let daily_path = cache_dir.join("1d").join(format!("{}.parquet", symbol));
                        get_parquet_last_date(&daily_path).map(|last_dt| subtract_one_day(&last_dt))
                    } else {
                        None
                    };
                    if daily_records.is_none() {
                        let daily_fetch = fetch_kline_multi_source(symbol, TimeFrame::D, daily_since.as_deref())
                            .unwrap_or(FetchResult { records: Vec::new(), source: "none".into() });
                        if !daily_fetch.records.is_empty() {
                            daily_records = Some(filter_by_start(&daily_fetch.records, start_date));
                        }
                    }
                    daily_records.as_ref().map(|d| resample_to_weekly(d))
                };

                match records {
                    Some(mut recs) => {
                        if !force {
                            if let Ok(old) = load_existing_parquet(&filepath) {
                                let old_last = old.last().map(|r| r.datetime.as_str()).unwrap_or("");
                                let new_last = recs.last().map(|r| r.datetime.as_str()).unwrap_or("");
                                if old_last == new_last && !old.is_empty() {
                                    results.push(SyncLevelResult {
                                        level: level_str, status: "skip".into(),
                                        count: old.len(), source: fetch.source.clone(),
                                        msg: "already up to date".into(),
                                    });
                                    continue;
                                }
                                recs = merge_records(&old, &recs);
                            }
                        }
                        let count = recs.len();
                        let source_used = if fetch.records.is_empty() { "resample".into() } else { fetch.source.clone() };
                        match save_parquet(&recs, &filepath) {
                            Ok(()) => SyncLevelResult {
                                level: level_str, status: "ok".into(), count,
                                source: source_used, msg: String::new(),
                            },
                            Err(e) => SyncLevelResult {
                                level: level_str, status: "error".into(), count: 0,
                                source: String::new(), msg: e.to_string(),
                            },
                        }
                    }
                    None => SyncLevelResult {
                        level: level_str, status: "fail".into(), count: 0,
                        source: String::new(), msg: "no data (all sources failed)".into(),
                    },
                }
            }
            // 日线 & 分钟级别
            _ => {
                let fetch = fetch_kline_multi_source(symbol, tf, inc_since.as_deref())
                    .unwrap_or(FetchResult { records: Vec::new(), source: "none".into() });
                if fetch.records.is_empty() {
                    // 如果本地已有数据且无新数据，跳过
                    if !force && filepath.exists() {
                        if let Ok(old) = load_existing_parquet(&filepath) {
                            if !old.is_empty() {
                                results.push(SyncLevelResult {
                                    level: level_str, status: "skip".into(),
                                    count: old.len(), source: "local".into(),
                                    msg: "already up to date".into(),
                                });
                                continue;
                            }
                        }
                    }
                    SyncLevelResult {
                        level: level_str, status: "fail".into(), count: 0,
                        source: String::new(), msg: "no data (all sources failed)".into(),
                    }
                } else {
                    let mut records = filter_by_start(&fetch.records, start_date);
                    // 缓存日线数据
                    if tf == TimeFrame::D {
                        daily_records = Some(records.clone());
                    }
                    if !force {
                        if let Ok(old) = load_existing_parquet(&filepath) {
                            let old_last = old.last().map(|r| r.datetime.as_str()).unwrap_or("");
                            let new_last = records.last().map(|r| r.datetime.as_str()).unwrap_or("");
                            if old_last == new_last && !old.is_empty() {
                                results.push(SyncLevelResult {
                                    level: level_str, status: "skip".into(),
                                    count: old.len(), source: fetch.source.clone(),
                                    msg: "already up to date".into(),
                                });
                                continue;
                            }
                            records = merge_records(&old, &records);
                        }
                    }
                    let count = records.len();
                    match save_parquet(&records, &filepath) {
                        Ok(()) => SyncLevelResult {
                            level: level_str, status: "ok".into(), count,
                            source: fetch.source.clone(), msg: String::new(),
                        },
                        Err(e) => SyncLevelResult {
                            level: level_str, status: "error".into(), count: 0,
                            source: String::new(), msg: e.to_string(),
                        },
                    }
                }
            }
        };

        results.push(result);
    }

    SyncStockResult {
        symbol: symbol.to_string(),
        levels: results,
    }
}

/// 获取数据目录状态
pub fn get_data_status(data_dir: &Path) -> DataStatus {
    let cache_dir = data_dir.join("kline_cache");
    let all_tfs = TimeFrame::all();

    // 统计总股票数：扫描所有级别目录的 parquet 文件，用 HashSet 去重
    let mut all_codes = std::collections::HashSet::new();
    for tf in all_tfs {
        let dir_name = tf_dir_name(*tf);
        let dir = cache_dir.join(dir_name);
        if dir.exists() {
            if let Ok(entries) = std::fs::read_dir(&dir) {
                for entry in entries.flatten() {
                    if entry.path().extension().map(|ext| ext == "parquet").unwrap_or(false) {
                        if let Some(code) = entry.file_name().to_str()
                            .and_then(|n| n.strip_suffix(".parquet"))
                        {
                            all_codes.insert(code.to_string());
                        }
                    }
                }
            }
        }
    }
    let total_stocks = all_codes.len();

    let mut level_stats = Vec::new();

    for tf in all_tfs {
        let dir_name = tf_dir_name(*tf);
        let dir = cache_dir.join(dir_name);

        let file_count = if dir.exists() {
            std::fs::read_dir(&dir)
                .map(|e| e.filter_map(|f| f.ok()).filter(|f| {
                    f.path().extension().map(|ext| ext == "parquet").unwrap_or(false)
                }).count())
                .unwrap_or(0)
        } else {
            0
        };

        // 取一个样本文件
        let (sample_symbol, sample_count, sample_start, sample_end) = if dir.exists() {
            if let Some(entry) = std::fs::read_dir(&dir)
                .ok()
                .and_then(|mut e| e.next())
                .and_then(|e| e.ok())
            {
                let name = entry.file_name().to_string_lossy().to_string();
                let sym = name.strip_suffix(".parquet").unwrap_or(&name).to_string();
                let path = entry.path();
                let count = get_parquet_row_count(&path).ok();
                let (s, e) = get_parquet_date_range(&path);
                (Some(sym), count, s, e)
            } else {
                (None, None, None, None)
            }
        } else {
            (None, None, None, None)
        };

        level_stats.push(LevelStats {
            level: tf.label().to_string(),
            dir_name: dir_name.to_string(),
            file_count,
            sample_symbol,
            sample_count,
            sample_start,
            sample_end,
        });
    }

    // 板块统计
    let boards = get_board_stats(data_dir);

    DataStatus {
        data_dir: data_dir.to_string_lossy().to_string(),
        total_stocks,
        levels: level_stats,
        boards,
    }
}

fn get_parquet_row_count(path: &Path) -> Result<usize> {
    let df = LazyFrame::scan_parquet(path, ScanArgsParquet::default())?
        .collect()?;
    Ok(df.height())
}

fn get_parquet_date_range(path: &Path) -> (Option<String>, Option<String>) {
    let records = load_existing_parquet(path).unwrap_or_default();
    if records.is_empty() {
        return (None, None);
    }
    (
        records.first().map(|r| r.datetime.clone()),
        records.last().map(|r| r.datetime.clone()),
    )
}

/// 获取本地 parquet 文件的最后日期，用于增量同步起始点
fn get_parquet_last_date(filepath: &Path) -> Option<String> {
    if !filepath.exists() {
        return None;
    }
    let (_, end) = get_parquet_date_range(filepath);
    end
}

/// 获取所有股票代码列表（扫描所有级别目录，用 HashSet 去重）
pub fn get_all_stock_codes(data_dir: &Path) -> Vec<String> {
    let cache_dir = data_dir.join("kline_cache");
    let mut all_codes = std::collections::HashSet::new();

    for tf in TimeFrame::all() {
        let dir = cache_dir.join(tf_dir_name(*tf));
        if dir.exists() {
            for code in extract_codes_from_dir(&dir) {
                all_codes.insert(code);
            }
        }
    }

    let mut codes: Vec<String> = all_codes.into_iter().collect();
    codes.sort();
    codes
}

fn extract_codes_from_dir(dir: &Path) -> Vec<String> {
    let mut codes = Vec::new();
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if let Some(code) = name.strip_suffix(".parquet") {
                codes.push(code.to_string());
            }
        }
    }
    codes.sort();
    codes
}

/// 同步指定板块的全部股票
/// 从东方财富在线 API 获取板块股票列表，然后逐只同步
pub fn sync_board(
    data_dir: &Path,
    board: &str,
    levels: &[TimeFrame],
    start_date: &str,
    force: bool,
) -> Vec<SyncStockResult> {
    let codes = fetch_board_stock_codes(board).unwrap_or_else(|e| {
        eprintln!("获取板块 {} 股票列表失败: {}, 尝试本地扫描", board, e);
        get_stock_codes_by_board(data_dir, board)
    });

    codes.iter()
        .map(|sym| sync_stock(data_dir, sym, levels, start_date, force))
        .collect()
}
