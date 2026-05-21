//! K 线数据同步模块 — 多数据源协同下载，带完整性校验
//!
//! 数据源优先级（按级别动态调整，对齐 moyan-project）：
//! - 股票列表: Tushare(P1) → 东方财富(P2) → 新浪(P3)
//! - 日线/周线/月线: Tushare(P1) → 新浪(P2) → 腾讯(P3) → 东方财富(P4,仅日线)
//! - 分钟线: 东方财富(P1) → 新浪(P2) → 腾讯(P3) → Tushare(P4)
//!
//! 数据源特点：
//! 1. Tushare — 数据质量最高，支持全级别（免费token支持日/周/月线）
//! 2. 新浪财经 (sina) — 全级别，速度快，最多 2000 条
//! 3. 腾讯财经 (tencent) — 全级别，稳定，最多 2000 条
//! 4. 东方财富 (eastmoney) — 分钟级最优，最多 10000 条，日线也支持
//!
//! 同步策略：主源失败 → 自动切换备源；多源数据交叉验证

use anyhow::{Context, Result};
use chrono::{Datelike, NaiveDate};
use polars::prelude::*;
use serde::{Deserialize, Serialize};
use std::path::Path;

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
    /// 优先级参考 moyan-project:
    ///   日/周/月线: Tushare(P1) > 新浪(P2) > 腾讯(P3) > 东方财富(P4)
    ///   分钟线:     东方财富(P1) > 新浪(P2) > 腾讯(P3) > Tushare(P4)
    fn source_config(&self) -> Option<DataSourceConfig> {
        match self {
            TimeFrame::M => Some(DataSourceConfig {
                sina_scale: 240, // 新浪没有月线，要从日线重采样
                sina_datalen: 2000,
                tencent_klt: None, // 腾讯也没有月线
                eastmoney_klt: None,  // 东方财富不支持月线
                eastmoney_lmt: 0,
                tushare_freq: Some("M"), // Tushare 月线
            }),
            TimeFrame::W => Some(DataSourceConfig {
                sina_scale: 1200,
                sina_datalen: 600,
                tencent_klt: Some("w"),
                eastmoney_klt: None,  // 东方财富没有周线
                eastmoney_lmt: 0,
                tushare_freq: Some("W"),
            }),
            TimeFrame::D => Some(DataSourceConfig {
                sina_scale: 240,
                sina_datalen: 2000,
                tencent_klt: Some("day"),
                eastmoney_klt: Some("101"), // 东方财富日线 klt=101
                eastmoney_lmt: 5000,
                tushare_freq: Some("D"),
            }),
            TimeFrame::F60 => Some(DataSourceConfig {
                sina_scale: 60,
                sina_datalen: 500,
                tencent_klt: Some("60"),
                eastmoney_klt: Some("60"),
                eastmoney_lmt: 10000,
                tushare_freq: Some("60min"),
            }),
            TimeFrame::F30 => Some(DataSourceConfig {
                sina_scale: 30,
                sina_datalen: 500,
                tencent_klt: Some("30"),
                eastmoney_klt: Some("30"),
                eastmoney_lmt: 10000,
                tushare_freq: Some("30min"),
            }),
            TimeFrame::F15 => Some(DataSourceConfig {
                sina_scale: 15,
                sina_datalen: 500,
                tencent_klt: Some("15"),
                eastmoney_klt: Some("15"),
                eastmoney_lmt: 10000,
                tushare_freq: Some("15min"),
            }),
            TimeFrame::F5 => Some(DataSourceConfig {
                sina_scale: 5,
                sina_datalen: 500,
                tencent_klt: Some("5"),
                eastmoney_klt: Some("5"),
                eastmoney_lmt: 10000,
                tushare_freq: Some("5min"),
            }),
            TimeFrame::F1 => Some(DataSourceConfig {
                sina_scale: 1,
                sina_datalen: 500,
                tencent_klt: Some("1"),
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
/// 对齐 moyan-project 的分类逻辑
pub fn classify_board(code: &str) -> &'static str {
    if code.starts_with("688") || code.starts_with("689") {
        "star" // 科创板
    } else if code.starts_with("300") || code.starts_with("301") {
        "gem" // 创业板
    } else if code.starts_with("60") {
        "sh_main" // 上证主板
    } else if code.starts_with("000") || code.starts_with("001")
        || code.starts_with("002") || code.starts_with("003")
    {
        "sz_main" // 深证主板（含原中小板002/003）
    } else if code.starts_with('4') || code.starts_with('8') || code.starts_with('9') {
        "bse" // 北交所（4xxxxx, 8xxxxx, 9xxxxx）
    } else if code.starts_with('6') || code.starts_with('9') {
        "sh_main" // 上证B股等（6xx/9xx 非科创板）
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
        .timeout(std::time::Duration::from_secs(10))
        .connect_timeout(std::time::Duration::from_secs(5))
        .build()?)
}

/// 从 Tushare 获取股票列表（第一优先，数据质量最高）
/// 对齐 moyan-project 的 fetch_tushare_data() 逻辑
/// Tushare 返回全市场股票（含北交所），数据最完整
pub fn fetch_board_codes_tushare(board: &str) -> Result<Vec<String>> {
    // 板块对应 Tushare 的市场/代码过滤
    let (exchange, market_filter): (&str, Box<dyn Fn(&str) -> bool>) = match board {
        "sh_main" => ("SSE", Box::new(|code: &str| code.starts_with("60"))),
        "sz_main" => ("SZSE", Box::new(|code: &str| code.starts_with("00") || code.starts_with("001") || code.starts_with("002") || code.starts_with("003"))),
        "gem"     => ("SZSE", Box::new(|code: &str| code.starts_with("30") || code.starts_with("301"))),
        "star"    => ("SSE", Box::new(|code: &str| code.starts_with("688") || code.starts_with("689"))),
        "bse"     => ("BSE", Box::new(|code: &str| code.starts_with('4') || code.starts_with('8') || code.starts_with('9'))),
        "all_a"   => ("", Box::new(|_: &str| true) as Box<dyn Fn(&str) -> bool>),
        _ => return Err(anyhow::anyhow!("Tushare不支持板块: {}", board)),
    };

    let params = serde_json::json!({
        "api_name": "stock_basic",
        "token": TUSHARE_TOKEN,
        "params": {
            "exchange": if board == "all_a" { "" } else { exchange },
            "list_status": "L", // 仅上市状态
            "fields": "ts_code,symbol,name,market,list_date,delist_date"
        }
    });

    let client = reqwest::blocking::Client::builder()
        .user_agent("Mozilla/5.0")
        .timeout(std::time::Duration::from_secs(10))
        .connect_timeout(std::time::Duration::from_secs(5))
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
        .context("解析 Tushare 股票列表响应失败")?;

    let code_val = json.get("code").and_then(|c| c.as_i64()).unwrap_or(-1);
    if code_val != 0 {
        let msg = json.get("msg").and_then(|m| m.as_str()).unwrap_or("unknown");
        anyhow::bail!("Tushare stock_basic error: {}", msg);
    }

    let fields = json.get("data")
        .and_then(|d| d.get("fields"))
        .and_then(|f| f.as_array());

    let items = json.get("data")
        .and_then(|d| d.get("items"))
        .and_then(|f| f.as_array());

    let Some(fields) = fields else { return Ok(Vec::new()); };
    let Some(items) = items else { return Ok(Vec::new()); };

    let field_names: Vec<&str> = fields.iter().filter_map(|f| f.as_str()).collect();
    let symbol_idx = field_names.iter().position(|f| *f == "symbol");

    let Some(sym_idx) = symbol_idx else {
        anyhow::bail!("Tushare 响应缺少 symbol 字段");
    };

    // all_a 时获取全市场不过滤交易所
    let codes: Vec<String> = if board == "all_a" {
        items.iter()
            .filter_map(|item| {
                item.as_array()
                    .and_then(|row| row.get(sym_idx))
                    .and_then(|v| v.as_str())
                    .filter(|s| !s.is_empty())
                    .map(|s| s.to_string())
            })
            .filter(|code| {
                // 过滤非A股（如指数、基金等），只保留6位数字代码
                code.len() == 6 && code.chars().all(|c| c.is_ascii_digit())
                    && (code.starts_with('0') || code.starts_with('3')
                        || code.starts_with('6') || code.starts_with('4')
                        || code.starts_with('8') || code.starts_with('9'))
            })
            .collect()
    } else {
        items.iter()
            .filter_map(|item| {
                item.as_array()
                    .and_then(|row| row.get(sym_idx))
                    .and_then(|v| v.as_str())
                    .filter(|s| !s.is_empty())
                    .map(|s| s.to_string())
            })
            .filter(|code| market_filter(code))
            .collect()
    };

    Ok(codes)
}

/// 尝试从新浪财经获取股票列表（第二备案）
/// 新浪 API 每页最多返回 80 条，需要分页获取全部
pub fn fetch_board_codes_sina(board: &str) -> Result<Vec<String>> {
    // 新浪股票列表 API：按市场/板块获取全部
    // node 参数：sh_a=沪主板, sz_a=深主板, cyb=创业板, kcb=科创板
    // 北交所新浪不支持，返回空
    let (node, prefix_filter) = match board {
        "sh_main" => ("sh_a", vec!["60"]),
        "sz_main" => ("sz_a", vec!["00"]),
        "gem"     => ("cyb",  vec!["30"]),
        "star"    => ("kcb",  vec!["68"]),
        "bse"     => return Ok(Vec::new()), // 新浪不支持北交所
        _ => return Err(anyhow::anyhow!("新浪不支持板块: {}", board)),
    };

    let client = build_http_client()?;
    let mut all_codes = Vec::new();
    let page_size = 80; // 新浪每页最多80条
    let mut page = 1;
    let max_pages = 100; // 安全上限

    loop {
        let url = format!(
            "https://vip.stock.finance.sina.com.cn/quotes_service/api/json_v2.php/Market_Center.getHQNodeData?page={}&num={}&sort=symbol&asc=1&node={}",
            page, page_size, node
        );

        let resp = client.get(&url)
            .header("Referer", "https://finance.sina.com.cn/")
            .send()?;
        let body = resp.text()?;

        let list: Vec<serde_json::Value> = if body.starts_with('[') {
            serde_json::from_str(&body).unwrap_or_default()
        } else {
            break;
        };

        if list.is_empty() {
            break;
        }

        for item in &list {
            let symbol = item.get("symbol").and_then(|v| v.as_str()).unwrap_or("");
            // symbol 格式: "sh600000", "sz000001"
            let code = symbol.trim_start_matches("sh")
                             .trim_start_matches("sz")
                             .trim_start_matches("bj");
            if code.is_empty() { continue; }

            let is_match = prefix_filter.iter().any(|p| code.starts_with(p));
            if is_match {
                all_codes.push(code.to_string());
            }
        }

        // 本页不足 page_size 条，说明已经是最后一页
        if list.len() < page_size {
            break;
        }

        page += 1;
        if page > max_pages {
            break;
        }
        // 请求间延迟避免限流
        std::thread::sleep(std::time::Duration::from_millis(100));
    }

    Ok(all_codes)
}

/// 从东方财富在线 API 获取指定板块的股票代码列表
/// 支持多数据源回退：Tushare → 东方财富 → 新浪
/// 支持分页，确保获取全部股票
/// 东方财富板块查询策略（参考 moyan-project 实测可用配置）
struct EastMoneyBoardStrategy {
    name: &'static str,
    fs: &'static str,
    fid: &'static str,
    max_pages: usize,
    /// 过滤函数：返回 true 表示该代码属于此板块
    filter: fn(&str) -> bool,
}

fn get_eastmoney_strategies(board: &str) -> Vec<EastMoneyBoardStrategy> {
    match board {
        "sh_main" => vec![
            // 主策略：精确 fs，total 直接等于沪主板数量
            EastMoneyBoardStrategy {
                name: "沪A主板",
                fs: "m:1+t:2,m:1+t:23",
                fid: "f3",
                max_pages: 25,
                filter: |code| code.starts_with("60") && !code.starts_with("688"),
            },
        ],
        "sz_main" => vec![
            // 主策略：精确 fs
            EastMoneyBoardStrategy {
                name: "深A主板",
                fs: "m:0+t:6,m:0+t:80",
                fid: "f3",
                max_pages: 20,
                filter: |code| code.starts_with("00"),
            },
        ],
        "gem" => vec![
            // 主策略：moyan-project 实测可用的精确创业板参数
            EastMoneyBoardStrategy {
                name: "创业板(精确)",
                fs: "m:0+t:81+s:2048",
                fid: "f3",
                max_pages: 20,
                filter: |code| code.starts_with("30"),
            },
            // 备用策略：深市全量中过滤
            EastMoneyBoardStrategy {
                name: "创业板(备用)",
                fs: "m:0+t:6,m:0+t:80",
                fid: "f3",
                max_pages: 20,
                filter: |code| code.starts_with("30"),
            },
        ],
        "star" => vec![
            // 主策略：moyan-project 实测可用的精确科创板参数
            EastMoneyBoardStrategy {
                name: "科创板(精确)",
                fs: "m:1+t:23+f:!50",
                fid: "f3",
                max_pages: 10,
                filter: |code| code.starts_with("68"),
            },
            // 备用策略：沪市全量中过滤
            EastMoneyBoardStrategy {
                name: "科创板(备用)",
                fs: "m:1+t:23",
                fid: "f3",
                max_pages: 25,
                filter: |code| code.starts_with("68"),
            },
        ],
        "bse" => vec![
            // 主策略：moyan-project 实测可用的北交所参数
            EastMoneyBoardStrategy {
                name: "北交所(精确)",
                fs: "m:0+t:81+s:2048",
                fid: "f3",
                max_pages: 5,
                filter: |code| code.starts_with("8") || code.starts_with("4"),
            },
            // 备用策略
            EastMoneyBoardStrategy {
                name: "北交所(备用)",
                fs: "m:0+t:81",
                fid: "f3",
                max_pages: 5,
                filter: |code| code.starts_with("8") || code.starts_with("4"),
            },
        ],
        "all_a" => vec![
            EastMoneyBoardStrategy {
                name: "全A股",
                fs: "m:0+t:6,m:0+t:80,m:1+t:2,m:1+t:23,m:0+t:81",
                fid: "f3",
                max_pages: 60,
                filter: |_code| true,
            },
        ],
        _ => Vec::new(),
    }
}

pub fn fetch_board_stock_codes(board: &str) -> Result<Vec<String>> {
    // ── 1. Tushare（第一优先，数据最完整，含北交所）──
    match fetch_board_codes_tushare(board) {
        Ok(codes) if !codes.is_empty() => return Ok(codes),
        result => {
            eprintln!("Tushare获取 {} 股票列表失败: {:?}, 尝试东方财富", board, result.err());
        }
    }

    // ── 2. 东方财富（分策略分页获取） ──
    let strategies = get_eastmoney_strategies(board);
    if !strategies.is_empty() {
        match fetch_board_codes_eastmoney_v2(&strategies) {
            Ok(codes) if !codes.is_empty() => return Ok(codes),
            result => {
                eprintln!("东方财富获取 {} 股票列表失败: {:?}, 尝试新浪", board, result.err());
            }
        }
    }

    // ── 3. 新浪 ──
    if board != "all_a" {
        match fetch_board_codes_sina(board) {
            Ok(codes) if !codes.is_empty() => return Ok(codes),
            result => {
                eprintln!("新浪获取 {} 股票列表也失败: {:?}", board, result.err());
            }
        }
    } else {
        // all_a 需要合并各子板块
        let mut all_codes = Vec::new();
        let mut codes_set = std::collections::HashSet::new();
        for sub_board in &["sh_main", "sz_main", "gem", "star", "bse"] {
            match fetch_board_codes_sina(sub_board) {
                Ok(codes) if !codes.is_empty() => {
                    for c in codes {
                        if codes_set.insert(c.clone()) {
                            all_codes.push(c);
                        }
                    }
                    continue;
                }
                _ => {}
            }
            // 新浪不支持北交所，尝试东方财富
            match fetch_board_stock_codes(sub_board) {
                Ok(codes) if !codes.is_empty() => {
                    for c in codes {
                        if codes_set.insert(c.clone()) {
                            all_codes.push(c);
                        }
                    }
                }
                _ => {}
            }
        }
        if !all_codes.is_empty() {
            return Ok(all_codes);
        }
    }

    Err(anyhow::anyhow!("所有数据源(Tushare/东方财富/新浪)均无法获取 {} 的股票列表", board))
}

/// 东方财富分策略分页获取（参考 moyan-project 实测方案）
/// 对每个 strategy 分别分页请求，然后去重合并
fn fetch_board_codes_eastmoney_v2(strategies: &[EastMoneyBoardStrategy]) -> Result<Vec<String>> {
    let client = build_eastmoney_client()?;
    let mut all_codes = std::collections::HashSet::new();

    for strategy in strategies {
        let mut page = 1u64;
        let page_size = 100u64;
        let mut strategy_consecutive_errors = 0u32;

        loop {
            let url = format!(
                "http://push2.eastmoney.com/api/qt/clist/get?pn={}&pz={}&po=1&np=1&ut=bd1d9ddb04089700cf9c27f6f7426281&fltt=2&invt=2&fid={}&fs={}&fields=f12,f14",
                page, page_size, strategy.fid, strategy.fs
            );

            let mut last_err = None;
            let mut codes = Vec::new();
            let mut total = 0usize;

            // 每页最多重试 2 次，快速失败不死等
            for attempt in 0..2 {
                match try_fetch_board_codes_with_total(&client, &url) {
                    Ok((c, t)) => {
                        codes = c;
                        total = t;
                        break;
                    }
                    Err(e) => {
                        last_err = Some(e);
                        let err_msg = last_err.as_ref().unwrap().to_string();
                        let is_connection_error = err_msg.contains("connection")
                            || err_msg.contains("reset")
                            || err_msg.contains("refused")
                            || err_msg.contains("timed out")
                            || err_msg.contains("Empty");
                        if is_connection_error {
                            // 连接级别错误，不重试，直接放弃本策略
                            eprintln!("东方财富 {} 连接失败，跳过: {}", strategy.name, err_msg);
                            strategy_consecutive_errors = 99; // 强制跳出外层循环
                            break;
                        }
                        if attempt == 0 {
                            std::thread::sleep(std::time::Duration::from_millis(200));
                        }
                    }
                }
            }

            if strategy_consecutive_errors >= 99 {
                break; // 连接失败，跳到下一个策略
            }

            if let Some(e) = last_err {
                eprintln!("东方财富 {} 第{}页获取失败: {}", strategy.name, page, e);
                strategy_consecutive_errors += 1;
                if strategy_consecutive_errors >= 2 {
                    break; // 连续2页失败，跳到下一个策略
                }
                page += 1;
                continue;
            }

            strategy_consecutive_errors = 0; // 成功则重置错误计数

            // 用 filter 过滤出属于此板块的代码
            let page_count = codes.len();
            for code in codes {
                if (strategy.filter)(&code) {
                    all_codes.insert(code);
                }
            }

            // 如果已获取到全部数据或本页无数据，退出
            let fetched = (page * page_size) as usize;
            if fetched >= total || page_count == 0 || page as usize >= strategy.max_pages {
                break;
            }

            page += 1;
            // 请求间加短延迟，避免被限流
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
    }

    if all_codes.is_empty() {
        return Err(anyhow::anyhow!("东方财富未获取到任何股票代码"));
    }

    Ok(all_codes.into_iter().collect())
}

fn try_fetch_board_codes_with_total(client: &reqwest::blocking::Client, url: &str) -> Result<(Vec<String>, usize)> {
    let resp = client.get(url)
        .header("Referer", "http://quote.eastmoney.com/")
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
    /// 该板块最新的数据文件修改日期 (YYYY-MM-DD)
    pub latest_date: String,
    /// 按日期统计股票数量 (日期 -> 股票数)
    pub date_distribution: std::collections::HashMap<String, usize>,
}

/// 从在线 API 获取指定板块的股票总数
/// 优先级: Tushare(精确) → 东方财富(轻量total查询) → 新浪(列表计数) → 东方财富(完整列表计数)
/// 任一源成功即返回，不死等限流的源
pub fn fetch_board_online_count(board: &str) -> Result<usize> {
    // ── 1. Tushare（第一优先，数据最精确） ──
    match fetch_board_codes_tushare(board) {
        Ok(codes) if !codes.is_empty() => {
            return Ok(codes.len());
        }
        result => {
            eprintln!("Tushare获取 {} 在线总数失败: {:?}, 尝试东方财富", board, result.err());
        }
    }

    // ── 2. 东方财富轻量级 total 查询（最快） ──
    let strategies = get_eastmoney_strategies(board);
    let client = build_eastmoney_client().ok();

    if let Some(ref client) = client {
        for strategy in &strategies {
            if strategy.fs.contains(",") {
                continue; // 跳过混合策略
            }

            let url = format!(
                "http://push2.eastmoney.com/api/qt/clist/get?pn=1&pz=1&np=1&ut=bd1d9ddb04089700cf9c27f6f7426281&fltt=2&invt=2&fid={}&fs={}&fields=f12",
                strategy.fid, strategy.fs
            );

            // 最多重试 2 次，间隔短（快速失败，不死等）
            for attempt in 0..2 {
                match try_fetch_online_count(client, &url) {
                    Ok(total) if total > 0 => {
                        return Ok(total);
                    }
                    Err(e) => {
                        let is_connection_error = e.to_string().contains("connection")
                            || e.to_string().contains("reset")
                            || e.to_string().contains("refused")
                            || e.to_string().contains("timed out")
                            || e.to_string().contains("Empty");
                        if is_connection_error {
                            eprintln!("东方财富连接失败({}), 切换数据源: {}", strategy.name, e);
                            break;
                        }
                        if attempt == 0 {
                            std::thread::sleep(std::time::Duration::from_millis(300));
                        } else {
                            eprintln!("东方财富获取 {} 在线总数失败: {}", strategy.name, e);
                        }
                    }
                    Ok(_) => {
                        break;
                    }
                }
            }
        }
    }

    eprintln!("东方财富轻量级查询失败，尝试新浪获取列表计数");

    // ── 3. 新浪：获取列表取长度 ──
    if board != "all_a" {
        match fetch_board_codes_sina(board) {
            Ok(codes) if !codes.is_empty() => {
                eprintln!("新浪获取 {} 列表成功: {} 只", board, codes.len());
                return Ok(codes.len());
            }
            result => {
                eprintln!("新浪获取 {} 失败: {:?}", board, result.err());
            }
        }
    } else {
        // all_a 需要合并各子板块
        let mut total = 0usize;
        for sub_board in &["sh_main", "sz_main", "gem", "star", "bse"] {
            match fetch_board_codes_sina(sub_board) {
                Ok(codes) if !codes.is_empty() => {
                    total += codes.len();
                    continue;
                }
                _ => {}
            }
            // 新浪不支持北交所，尝试东方财富
            if *sub_board == "bse" {
                match fetch_board_stock_codes(sub_board) {
                    Ok(codes) if !codes.is_empty() => {
                        total += codes.len();
                    }
                    _ => {}
                }
            }
        }
        if total > 0 {
            return Ok(total);
        }
    }

    // ── 4. 东方财富完整列表兜底 ──
    eprintln!("新浪均失败，回退到东方财富完整列表");
    match fetch_board_stock_codes(board) {
        Ok(codes) => Ok(codes.len()),
        Err(e) => Err(e.context(format!("获取 {} 在线总数失败（所有数据源均失败）", board))),
    }
}

fn try_fetch_online_count(client: &reqwest::blocking::Client, url: &str) -> Result<usize> {
    let resp = client.get(url)
        .header("Referer", "http://quote.eastmoney.com/")
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
    // 该板块最新的数据文件修改日期 (YYYY-MM-DD)
    let mut board_latest: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();
    // board_stock_dates[board_id][code] = 所有级别中最晚的修改日期
    let mut board_stock_dates: std::collections::HashMap<String, std::collections::HashMap<String, String>> =
        std::collections::HashMap::new();

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
                .insert(code.clone());
            // 更新该板块的最新文件修改日期
            if let Ok(metadata) = entry.metadata() {
                if let Ok(modified) = metadata.modified() {
                    let modified_date: chrono::DateTime<chrono::Local> = modified.into();
                    let mod_str = modified_date.format("%Y-%m-%d").to_string();
                    board_latest
                        .entry(board.to_string())
                        .and_modify(|d| {
                            if mod_str > *d { *d = mod_str.clone(); }
                        })
                        .or_insert(mod_str.clone());
                    // 更新 board_stock_dates[board][code] = max(现有, mod_str)
                    board_stock_dates
                        .entry(board.to_string())
                        .or_default()
                        .entry(code)
                        .and_modify(|d| {
                            if mod_str > *d { *d = mod_str.clone(); }
                        })
                        .or_insert(mod_str);
                }
            }
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
        // 计算 date_distribution: 按 code 的最新修改日期归类计数
        let date_distribution = board_stock_dates.remove(*id).unwrap_or_default()
            .into_values()
            .fold(std::collections::HashMap::new(), |mut acc, date| {
                *acc.entry(date).or_insert(0usize) += 1;
                acc
            });
        results.push(BoardOnlineInfo {
            id: id.to_string(),
            name: name.to_string(),
            total_count: online_count,
            local_count,
            level_counts: lv_counts,
            latest_date: board_latest.remove(*id).unwrap_or_default(),
            date_distribution,
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

    // 汇总 date_distribution: merge 各子板块
    let mut all_date_distribution: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for r in &results {
        for (date, cnt) in &r.date_distribution {
            *all_date_distribution.entry(date.clone()).or_insert(0) += cnt;
        }
    }

    results.push(BoardOnlineInfo {
        id: "all_a".to_string(),
        name: "全 A 股".to_string(),
        total_count: all_online_count,
        local_count: all_local_count,
        level_counts: all_level_counts,
        latest_date: results.iter().map(|r| r.latest_date.as_str()).max().unwrap_or("").to_string(),
        date_distribution: all_date_distribution,
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
        .timeout(std::time::Duration::from_secs(8))
        .connect_timeout(std::time::Duration::from_secs(5))
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
//  数据源 3: 东方财富 (分钟级最优)
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
        .timeout(std::time::Duration::from_secs(10))
        .connect_timeout(std::time::Duration::from_secs(5))
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
        .timeout(std::time::Duration::from_secs(10))
        .connect_timeout(std::time::Duration::from_secs(5))
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
    source: String, // "sina" | "tencent" | "eastmoney" | "tushare" | ...
}

/// 多数据源协同获取 K 线数据
/// 按级别动态调整数据源优先级（对齐 moyan-project）：
/// - 日线/周线/月线: Tushare(P1) → 新浪(P2) → 腾讯(P3) → 东方财富(P4,仅日线)
/// - 分钟级: 东方财富(P1) → 新浪(P2) → 腾讯(P3) → Tushare(P4)
/// since: 可选的增量起始日期 (格式 "2024-01-01"，有连字符)，各数据源内部按需转换格式
fn fetch_kline_multi_source(
    code: &str,
    tf: TimeFrame,
    since: Option<&str>,
) -> Result<FetchResult> {
    let sina_symbol = code_to_sina(code);
    let tencent_symbol = code_to_tencent(code);
    let em_secid = code_to_eastmoney(code);
    let ts_code = code_to_tushare(code);

    // 各数据源需要的日期格式不同，提前转换
    // since 格式: "2024-01-01"（有连字符）
    // 东财/Tushare: "20240101"（无连字符）
    let since_nohyphen = since.map(|s| s.replace("-", ""));
    let since_sina = since;                          // "2024-01-01"
    let since_tencent = since;                       // "2024-01-01"
    let since_eastmoney = since_nohyphen.as_deref(); // "20240101"
    let since_tushare = since_nohyphen.as_deref();   // "20240101"

    let cfg = match tf.source_config() {
        Some(c) => c,
        None => return Ok(FetchResult { records: Vec::new(), source: "none".into() }),
    };

    let _is_daily_or_above = matches!(tf, TimeFrame::M | TimeFrame::W | TimeFrame::D);
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
        // ═══ 日线/周线/月线优先级: Tushare → 新浪 → 腾讯 → 东方财富(仅日线) ═══

        // ─── 1. Tushare (全级别，数据质量最高，对齐 moyan-project) ───
        if let Some(freq) = cfg.tushare_freq {
            match fetch_tushare_kline(&ts_code, freq, since_tushare) {
                Ok(data) if !data.is_empty() => {
                    return Ok(FetchResult { records: data, source: "tushare".into() });
                }
                _ => {}
            }
        }

        // ─── 2. 新浪 (月线不支持,需日线重采样) ───
        if tf != TimeFrame::M {
            match fetch_sina_kline(&sina_symbol, cfg.sina_scale, cfg.sina_datalen, since_sina) {
                Ok(data) if !data.is_empty() => {
                    return Ok(FetchResult { records: data, source: "sina".into() });
                }
                _ => {}
            }
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

        // ─── 4. 东方财富 (仅日线) ───
        if let Some(klt) = cfg.eastmoney_klt {
            match fetch_eastmoney_kline(&em_secid, klt, cfg.eastmoney_lmt, since_eastmoney) {
                Ok(data) if !data.is_empty() => {
                    return Ok(FetchResult { records: data, source: "eastmoney".into() });
                }
                _ => {}
            }
        }

        // ─── 5. 兜底: 日线重试（周线失败时也可用日线重采样） ───
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
    // 过滤掉只有日期没有时间部分的旧数据（旧版同步可能将日线混入分钟级文件）
    for r in old {
        // 如果旧数据只有日期没有时间，而新数据有时间部分，说明旧数据是脏数据，跳过
        if !r.datetime.contains(':') && new.iter().any(|n| n.datetime.contains(':')) {
            continue;
        }
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
        let _consecutive_gaps: Vec<(usize, &KlineRecord, &KlineRecord)> = records.windows(2)
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

    // ─── 提前跳过检查: 非 force 时，如果所有级别本地数据都已是最新，直接跳过 ───
    if !force {
        let all_up_to_date = levels.iter().all(|&tf| {
            let dir_name = tf_dir_name(tf);
            let filepath = cache_dir.join(dir_name).join(format!("{}.parquet", symbol));
            is_local_up_to_date(&filepath)
        });
        if all_up_to_date {
            // 所有级别都已是最新，直接返回 skip，不发网络请求
            for &tf in levels {
                let dir_name = tf_dir_name(tf);
                let filepath = cache_dir.join(dir_name).join(format!("{}.parquet", symbol));
                let count = get_parquet_row_count(&filepath).unwrap_or(0);
                results.push(SyncLevelResult {
                    level: dir_name.to_string(), status: "skip".into(),
                    count, source: "local".into(),
                    msg: "already up to date (fast skip)".into(),
                });
            }
            return SyncStockResult {
                symbol: symbol.to_string(),
                levels: results,
            };
        }
    }

    // 缓存日线数据供周/月线重采样
    let mut daily_records: Option<Vec<KlineRecord>> = None;

    for &tf in levels {
        let dir_name = tf_dir_name(tf);
        let filepath = cache_dir.join(dir_name).join(format!("{}.parquet", symbol));
        let level_str = dir_name.to_string();

        // ─── 单级别提前跳过: 如果该级别已是最新，跳过不请求 ───
        if !force && is_local_up_to_date(&filepath) {
            let count = get_parquet_row_count(&filepath).unwrap_or(0);
            results.push(SyncLevelResult {
                level: level_str, status: "skip".into(),
                count, source: "local".into(),
                msg: "already up to date (fast skip)".into(),
            });
            continue;
        }

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
                            // 快速判断：新数据最后日期 vs 本地最后日期
                            let local_last = get_parquet_last_date(&filepath);
                            let new_last = recs.last().map(|r| r.datetime.as_str()).unwrap_or("");
                            if let Some(ref old_last) = local_last {
                                if old_last == new_last && !new_last.is_empty() {
                                    let count = get_parquet_row_count(&filepath).unwrap_or(0);
                                    results.push(SyncLevelResult {
                                        level: level_str, status: "skip".into(),
                                        count, source: "resample_from_daily".into(),
                                        msg: "already up to date".into(),
                                    });
                                    continue;
                                }
                            }
                            // 有新数据，需要 merge
                            if let Ok(old) = load_existing_parquet(&filepath) {
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
                // 周线：直接从数据源获取，不本地重采样
                let fetch = fetch_kline_multi_source(symbol, tf, inc_since.as_deref())
                    .unwrap_or(FetchResult { records: Vec::new(), source: "none".into() });
                let records = if !fetch.records.is_empty() {
                    Some(filter_by_start(&fetch.records, start_date))
                } else {
                    None
                };

                match records {
                    Some(mut recs) => {
                        if !force {
                            // 快速判断：新数据最后日期 vs 本地最后日期
                            let local_last = get_parquet_last_date(&filepath);
                            let new_last = recs.last().map(|r| r.datetime.as_str()).unwrap_or("");
                            if let Some(ref old_last) = local_last {
                                if old_last == new_last && !new_last.is_empty() {
                                    let count = get_parquet_row_count(&filepath).unwrap_or(0);
                                    results.push(SyncLevelResult {
                                        level: level_str, status: "skip".into(),
                                        count, source: fetch.source.clone(),
                                        msg: "already up to date".into(),
                                    });
                                    continue;
                                }
                            }
                            // 有新数据，需要 merge
                            if let Ok(old) = load_existing_parquet(&filepath) {
                                recs = merge_records(&old, &recs);
                            }
                        }
                        let count = recs.len();
                        match save_parquet(&recs, &filepath) {
                            Ok(()) => SyncLevelResult {
                                level: level_str, status: "ok".into(), count,
                                source: fetch.source, msg: String::new(),
                            },
                            Err(e) => SyncLevelResult {
                                level: level_str, status: "error".into(), count: 0,
                                source: String::new(), msg: e.to_string(),
                            },
                        }
                    }
                    None => SyncLevelResult {
                        level: level_str, status: "fail".into(), count: 0,
                        source: String::new(), msg: "no data (all sources failed, refusing local resample)".into(),
                    },
                }
            }
            // 日线 & 分钟级别
            _ => {
                let fetch = fetch_kline_multi_source(symbol, tf, inc_since.as_deref())
                    .unwrap_or(FetchResult { records: Vec::new(), source: "none".into() });
                if fetch.records.is_empty() {
                    // 如果本地已有数据且无新数据，跳过（用快速判断避免全量加载）
                    if !force && filepath.exists() {
                        let count = get_parquet_row_count(&filepath).unwrap_or(0);
                        if count > 0 {
                            results.push(SyncLevelResult {
                                level: level_str, status: "skip".into(),
                                count, source: "local".into(),
                                msg: "already up to date".into(),
                            });
                            continue;
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
                        // 快速判断：新数据最后日期 vs 本地最后日期
                        let local_last = get_parquet_last_date(&filepath);
                        let new_last = records.last().map(|r| r.datetime.as_str()).unwrap_or("");
                        if let Some(ref old_last) = local_last {
                            if old_last == new_last && !new_last.is_empty() {
                                let count = get_parquet_row_count(&filepath).unwrap_or(0);
                                results.push(SyncLevelResult {
                                    level: level_str, status: "skip".into(),
                                    count, source: fetch.source.clone(),
                                    msg: "already up to date".into(),
                                });
                                continue;
                            }
                        }
                        // 有新数据，需要 merge
                        if let Ok(old) = load_existing_parquet(&filepath) {
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
    // 用 Polars LazyFrame 优化查询只获取行数
    use polars::prelude::*;
    let lf = LazyFrame::scan_parquet(path, ScanArgsParquet::default())?;
    let count_df = lf.select([len().alias("count")]).collect()?;
    Ok(count_df.column("count")
        .ok()
        .and_then(|s| s.idx().ok().and_then(|ca| ca.first().map(|v| v as usize)))
        .unwrap_or(0))
}

fn get_parquet_date_range(path: &Path) -> (Option<String>, Option<String>) {
    if !path.exists() {
        return (None, None);
    }

    use polars::prelude::*;

    let lf = match LazyFrame::scan_parquet(path, ScanArgsParquet::default()) {
        Ok(lf) => lf,
        Err(_) => return (None, None),
    };

    // 先 collect 1行来获取 schema
    let sample = match lf.clone().slice(0, 1).collect() {
        Ok(df) => df,
        Err(_) => return (None, None),
    };

    let dt_name = match sample.get_column_names_str().iter()
        .find(|c| ["dt", "datetime", "date", "time", "timestamp"].contains(c))
    {
        Some(n) => n.to_string(),
        None => return (None, None),
    };

    // 只选 datetime 列，排序后取首尾各1行
    let first = lf.clone()
        .select([col(&dt_name)])
        .sort([dt_name.as_str()], SortMultipleOptions::default())
        .slice(0, 1)
        .collect()
        .ok();

    let last = lf
        .select([col(&dt_name)])
        .sort([dt_name.as_str()], SortMultipleOptions::default().with_order_descending(true))
        .slice(0, 1)
        .collect()
        .ok();

    let extract = |df: Option<DataFrame>| -> Option<String> {
        let df = df?;
        if df.height() == 0 { return None; }
        let s = df.column(&dt_name).ok()?;
        let ca = s.str().ok()?;
        let v = ca.get(0)?;
        Some(if v.len() > 10 { v[..10].to_string() } else { v.to_string() })
    };

    (extract(first), extract(last))
}

/// 利用 Parquet 文件的 metadata 快速获取最后日期（不需要加载全量数据）
/// 使用 Polars LazyFrame 只选 datetime 列 + 倒序取1行，避免全量加载
/// 返回 None 表示文件不存在或无法读取
fn get_parquet_last_date(filepath: &Path) -> Option<String> {
    if !filepath.exists() {
        return None;
    }

    use polars::prelude::*;

    let lf = LazyFrame::scan_parquet(filepath, ScanArgsParquet::default()).ok()?;

    // 先 collect 1行来获取 schema
    let sample = lf.clone().slice(0, 1).collect().ok()?;
    let dt_name = sample.get_column_names_str().iter()
        .find(|c| ["dt", "datetime", "date", "time", "timestamp"].contains(c))
        .map(|s| s.to_string())?;

    // 只选 datetime 列，倒序取最后1行
    let df = lf
        .select([col(&dt_name)])
        .sort([dt_name.as_str()], SortMultipleOptions::default().with_order_descending(true))
        .slice(0, 1)
        .collect()
        .ok()?;

    if df.height() > 0 {
        if let Ok(s) = df.column(&dt_name) {
            if let Ok(ca) = s.str() {
                if let Some(v) = ca.get(0) {
                    let dt = if v.len() > 10 { &v[..10] } else { v };
                    return Some(dt.to_string());
                }
            }
        }
    }

    None
}

/// 快速检查一个板块的本地数据是否已是最新（纯本地文件检查，无网络请求）
/// 返回 (本地股票数, 今日更新数, 是否全部最新, 最近同步日期)
/// 仅使用文件修改时间判断，不解析 parquet，极快
pub fn quick_check_board_up_to_date(
    data_dir: &Path,
    board: &str,
    levels: &[TimeFrame],
) -> (usize, usize, bool, String) {
    let cache_dir = data_dir.join("kline_cache");
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();

    // 收集该板块的所有本地股票代码（从第一个级别目录扫描）
    let first_dir_name = tf_dir_name(levels[0]);
    let first_dir = cache_dir.join(first_dir_name);
    let local_codes: Vec<String> = if first_dir.exists() {
        extract_codes_from_dir(&first_dir)
            .into_iter()
            .filter(|code| classify_board(code) == board)
            .collect()
    } else {
        return (0, 0, false, String::new());
    };

    let total = local_codes.len();
    if total == 0 {
        return (0, 0, false, String::new());
    }

    // 检查每只股票的每个级别文件修改时间
    let mut today_count = 0usize;
    let mut latest_date = String::new();

    for code in &local_codes {
        let mut all_today = true;
        for &tf in levels {
            let dir_name = tf_dir_name(tf);
            let filepath = cache_dir.join(dir_name).join(format!("{}.parquet", code));
            if let Ok(metadata) = std::fs::metadata(&filepath) {
                if let Ok(modified) = metadata.modified() {
                    let modified_date: chrono::DateTime<chrono::Local> = modified.into();
                    let mod_str = modified_date.format("%Y-%m-%d").to_string();
                    if mod_str > latest_date {
                        latest_date = mod_str.clone();
                    }
                    if mod_str != today {
                        // 不是今天修改，用 is_local_up_to_date 进一步检查
                        if !is_local_up_to_date(&filepath) {
                            all_today = false;
                        }
                    }
                } else {
                    all_today = false;
                }
            } else {
                all_today = false;
            }
        }
        if all_today {
            today_count += 1;
        }
    }

    let all_up_to_date = today_count == total && total > 0;
    (total, today_count, all_up_to_date, latest_date)
}

/// 快速检查一只股票在所有指定级别是否已是最新（不需要发网络请求）
/// 用于批量预过滤，避免对已最新的股票发无意义的网络请求
pub fn is_stock_up_to_date(data_dir: &Path, symbol: &str, levels: &[TimeFrame]) -> bool {
    let cache_dir = data_dir.join("kline_cache");
    levels.iter().all(|&tf| {
        let dir_name = tf_dir_name(tf);
        let filepath = cache_dir.join(dir_name).join(format!("{}.parquet", symbol));
        is_local_up_to_date(&filepath)
    })
}

/// 快速检查本地数据文件是否已是最新（不需要发网络请求）
/// 优先使用文件修改时间快速判断（极快，无 IO 解析）
/// 如果文件今天修改过，则认为是最新；否则用 get_parquet_last_date 精确判断，
/// 判断最后日期是否在工作日范围内（排除周末）
fn is_local_up_to_date(filepath: &Path) -> bool {
    if !filepath.exists() {
        return false; // 文件不存在，不是最新
    }

    // 方法1: 文件修改时间检查（极快，不解析 parquet）
    if let Ok(metadata) = std::fs::metadata(filepath) {
        if let Ok(modified) = metadata.modified() {
            let modified_date: chrono::DateTime<chrono::Local> = modified.into();
            let now = chrono::Local::now();
            let today = now.format("%Y-%m-%d").to_string();
            let mod_date = modified_date.format("%Y-%m-%d").to_string();
            // 文件今天被修改过，很大概率数据已是最新
            if mod_date == today {
                return true;
            }
        }
    }

    // 方法2: 精确检查——读取最后日期，判断是否在合理范围内
    let last_date = match get_parquet_last_date(filepath) {
        Some(d) => d,
        None => return false,
    };

    let last_dt = if last_date.len() > 10 { &last_date[..10] } else { &last_date };

    let today = chrono::Local::now().format("%Y-%m-%d").to_string();

    // 如果最后日期就是今天，必定最新
    if last_dt == today {
        return true;
    }

    // 计算最近的工作日日期（排除周末）
    // 如果最后日期 >= 最近工作日，则认为数据已是最新
    let last_naive = chrono::NaiveDate::parse_from_str(last_dt, "%Y-%m-%d");
    if let Ok(last) = last_naive {
        let today_naive = chrono::Local::now().date_naive();
        // 从今天往前找最近的工作日
        let mut check = today_naive;
        for _ in 0..3 {
            let weekday = check.weekday().num_days_from_monday();
            if weekday < 5 {
                // 是工作日
                break;
            }
            // 周末，往前推一天
            check = check - chrono::Duration::try_days(1).unwrap_or_default();
        }
        // 最后数据日期 >= 最近工作日 → 最新
        return last >= check;
    }

    // fallback: 检查是否是昨天
    let yesterday = (chrono::Local::now() - chrono::Duration::try_days(1).unwrap_or_default())
        .format("%Y-%m-%d")
        .to_string();
    last_dt == yesterday
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

/// 清空所有 K 线数据（删除 kline_cache 目录）
pub fn clear_all_data(data_dir: &Path) -> Result<usize> {
    let cache_dir = data_dir.join("kline_cache");
    if !cache_dir.exists() {
        return Ok(0);
    }
    let mut count = 0usize;
    if let Ok(entries) = std::fs::read_dir(&cache_dir) {
        for entry in entries.flatten() {
            if let Ok(dir_entries) = std::fs::read_dir(entry.path()) {
                for file in dir_entries.flatten() {
                    if file.path().extension().map(|e| e == "parquet").unwrap_or(false) {
                        count += 1;
                    }
                }
            }
        }
    }
    std::fs::remove_dir_all(&cache_dir)?;
    Ok(count)
}

/// 数据裁剪结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrimResult {
    pub trimmed_files: usize,
    pub removed_files: usize,
    pub rows_before: usize,
    pub rows_after: usize,
}

/// 裁剪过期数据
///
/// retention_months: 各级别保留月数的配置，key 为 tf_dir_name（如 "1m", "5m"），value 为保留月数
/// 保留月数为 0 表示不限制
pub fn trim_old_data(data_dir: &Path, retention_months: &std::collections::HashMap<String, u32>) -> Result<TrimResult> {
    let cache_dir = data_dir.join("kline_cache");
    if !cache_dir.exists() {
        return Ok(TrimResult {
            trimmed_files: 0,
            removed_files: 0,
            rows_before: 0,
            rows_after: 0,
        });
    }

    let now = chrono::Local::now();
    let mut trimmed_files = 0usize;
    let mut removed_files = 0usize;
    let mut rows_before = 0usize;
    let mut rows_after = 0usize;

    for (tf_dir, months) in retention_months {
        if *months == 0 {
            continue; // 0 表示不限制
        }
        let cutoff = now - chrono::Duration::days((*months as i64) * 30);
        let cutoff_str = cutoff.format("%Y-%m-%d").to_string();

        let tf_path = cache_dir.join(tf_dir);
        if !tf_path.exists() {
            continue;
        }

        if let Ok(entries) = std::fs::read_dir(&tf_path) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().map(|e| e == "parquet").unwrap_or(false) {
                    match load_existing_parquet(&path) {
                        Ok(records) => {
                            let orig_len = records.len();
                            rows_before += orig_len;
                            let filtered: Vec<_> = records
                                .into_iter()
                                .filter(|r| r.datetime >= cutoff_str)
                                .collect();
                            rows_after += filtered.len();

                            if filtered.is_empty() {
                                // 全部过期，删除文件
                                let _ = std::fs::remove_file(&path);
                                removed_files += 1;
                            } else if filtered.len() < orig_len {
                                // 有数据被裁剪，保存剩余
                                if save_parquet(&filtered, &path).is_ok() {
                                    trimmed_files += 1;
                                }
                            }
                        }
                        Err(_) => {
                            // 无法加载的文件跳过
                        }
                    }
                }
            }
        }
    }

    // 清理空目录
    let _ = crate::kline_manager::try_remove_empty_dirs(&cache_dir);

    Ok(TrimResult {
        trimmed_files,
        removed_files,
        rows_before,
        rows_after,
    })
}

/// 从在线数据源获取全市场当前在市（list_status=L）的 A 股代码
/// 用于与本地列表对比，发现退市股和新增股
pub fn fetch_all_listed_codes() -> Result<Vec<String>> {
    // 直接用 Tushare 的 all_a 获取全市场在市股票
    fetch_board_codes_tushare("all_a")
        .or_else(|_| {
            // Tushare 失败，则合并各板块从东方财富/新浪获取
            let mut all_codes = Vec::new();
            let mut codes_set = std::collections::HashSet::new();
            for board in &["sh_main", "sz_main", "gem", "star", "bse"] {
                if let Ok(codes) = fetch_board_stock_codes(board) {
                    for c in codes {
                        if codes_set.insert(c.clone()) {
                            all_codes.push(c);
                        }
                    }
                }
            }
            if all_codes.is_empty() {
                Err(anyhow::anyhow!("所有数据源均无法获取在市股票列表"))
            } else {
                Ok(all_codes)
            }
        })
}

/// 清理退市股数据：对比本地已有股票与在线在市列表，删除已退市股票的所有 K 线文件
///
/// 返回 (删除的股票代码列表, 删除的文件数)
pub fn clean_delisted_stocks(data_dir: &Path) -> Result<(Vec<String>, usize)> {
    let local_codes = get_all_stock_codes(data_dir);
    if local_codes.is_empty() {
        return Ok((Vec::new(), 0));
    }

    let online_codes = fetch_all_listed_codes()?;
    let online_set: std::collections::HashSet<_> = online_codes.iter().collect();

    // 本地有但在线没有的 = 退市
    let delisted: Vec<String> = local_codes
        .into_iter()
        .filter(|code| !online_set.contains(code))
        .collect();

    if delisted.is_empty() {
        return Ok((Vec::new(), 0));
    }

    eprintln!("[清理退市] 发现 {} 只退市股票: {:?}", delisted.len(), delisted);

    let cache_dir = data_dir.join("kline_cache");
    let mut removed_files = 0usize;

    for code in &delisted {
        // 删除该股票在所有级别下的 parquet 文件
        if let Ok(entries) = std::fs::read_dir(&cache_dir) {
            for tf_entry in entries.flatten() {
                let tf_path = tf_entry.path();
                if tf_path.is_dir() {
                    let parquet_path = tf_path.join(format!("{}.parquet", code));
                    if parquet_path.exists() {
                        if let Err(e) = std::fs::remove_file(&parquet_path) {
                            eprintln!("[清理退市] 删除 {} 失败: {}", parquet_path.display(), e);
                        } else {
                            removed_files += 1;
                        }
                    }
                }
            }
        }
    }

    // 清理空目录
    let _ = crate::kline_manager::try_remove_empty_dirs(&cache_dir);

    Ok((delisted, removed_files))
}

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
