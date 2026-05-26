//! Tauri IPC 命令

use std::path::PathBuf;
use tauri::State;

use crate::state::{AppState, SyncFailureRecord, SingleSyncState};
use crate::fusion;
use yifang_data::{ChartData, DataSource, KLine, StockInfo, TimeFrame, SyncStockResult, DataStatus, BoardStats, BoardOnlineInfo, ValidateStockResult, ValidateLevelResult, MoveDataResult};
use yifang_czsc::CzscAnalyzer;
use yifang_wyckoff::WyckoffAnalyzer;
use yifang_indicator::calc_macd;

/// 获取 K 线数据
#[tauri::command]
pub fn get_klines(
    state: State<'_, AppState>,
    symbol: String,
    timeframe: String,
) -> Result<Vec<KLine>, String> {
    let tf = parse_timeframe(&timeframe).ok_or_else(|| format!("无效的时间周期: {}", timeframe))?;
    let manager = state.manager.read().map_err(|e| e.to_string())?;
    manager.get_klines(&symbol, tf, None, None).map_err(|e| e.to_string())
}

/// 获取完整图表数据（K线 + MACD + 缠论 + 威科夫）
/// 如果本地无缓存，自动从网络同步该股票该级别的数据
/// 关键优化：只在读取数据时持有读锁，分析计算时释放锁，
/// 这样同步不会阻塞图表切换
#[tauri::command]
pub async fn get_chart_data(
    state: State<'_, AppState>,
    symbol: String,
    timeframe: String,
    enable_czsc: bool,
    enable_wyckoff: bool,
) -> Result<ChartData, String> {
    let tf = parse_timeframe(&timeframe).ok_or_else(|| format!("无效的时间周期: {}", timeframe))?;

    // ── 阶段1：只持有读锁，获取数据后立即释放 ──
    let (klines, name, need_sync) = {
        let manager = state.manager.read().map_err(|e| e.to_string())?;
        let klines = manager.get_klines(&symbol, tf, None, None).map_err(|e| e.to_string())?;
        let name = manager.get_stock_info(&symbol).map(|i| i.name).unwrap_or_default();
        // 分钟级数据可能存在脏数据（日线混入分钟文件），过滤后变空，
        // 此时需要重新同步覆盖脏数据
        // 或者数据为空、或缓存不是最新，都需要同步
        if klines.is_empty() {
            (klines, name, true)
        } else {
            // 检查缓存是否最新
            let up_to_date = yifang_data::is_stock_up_to_date(
                &manager.data_dir().join("kline_cache"),
                &symbol,
                &[tf],
            );
            (klines, name, !up_to_date)
        }
    };

    // ── 阶段2：如果本地无数据（本地没文件，或分钟级文件含脏数据被过滤后为空），自动同步 ──
    let klines = if need_sync {
        let data_dir = {
            let manager = state.manager.read().map_err(|e| e.to_string())?;
            manager.data_dir().to_path_buf()
        };

        // 用 force=true 强制覆盖（处理脏数据场景：文件存在但内容是日线数据）
        let sym = symbol.clone();
        let tf_val = tf;
        let sync_result = tauri::async_runtime::spawn_blocking(move || {
            yifang_data::sync_stock(&data_dir, &sym, &[tf_val], "2020-01-01", true)
        }).await.map_err(|e| e.to_string())?;

        eprintln!("[自动同步] {} {:?} 结果: {:?}", symbol, tf, sync_result.levels.iter().map(|l| format!("{}={}", l.level, l.status)).collect::<Vec<_>>());

        // 同步完成后重新读取
        let manager = state.manager.read().map_err(|e| e.to_string())?;
        let klines = manager.get_klines(&symbol, tf, None, None).map_err(|e| e.to_string())?;
        klines
    } else {
        klines
    };
    // ── 读锁已释放，后续分析不持有任何锁 ──

    // 2. 计算 MACD
    let macd = calc_macd(&klines, 12, 26, 9);

    // 3. 缠论分析
    let czsc = if enable_czsc {
        Some(CzscAnalyzer::analyze(&klines, &macd))
    } else {
        None
    };

    // 4. 威科夫分析
    let wyckoff = if enable_wyckoff {
        Some(WyckoffAnalyzer::analyze(&klines))
    } else {
        None
    };

    // 5. 融合分析
    let fusion = match (&czsc, &wyckoff) {
        (Some(c), Some(w)) => Some(fusion::analyze_fusion(c, w)),
        _ => None,
    };

    Ok(ChartData {
        symbol,
        name,
        timeframe: tf,
        klines,
        macd,
        czsc,
        wyckoff,
        fusion,
    })
}

/// 搜索股票
#[tauri::command]
pub fn search_stocks(
    state: State<'_, AppState>,
    keyword: String,
) -> Result<Vec<StockInfo>, String> {
    let manager = state.manager.read().map_err(|e| e.to_string())?;
    manager.search_stocks(&keyword).map_err(|e| e.to_string())
}

/// 获取股票信息
#[tauri::command]
pub fn get_stock_info(
    state: State<'_, AppState>,
    symbol: String,
) -> Result<StockInfo, String> {
    let manager = state.manager.read().map_err(|e| e.to_string())?;
    manager.get_stock_info(&symbol).map_err(|e| e.to_string())
}

/// 获取线段对应的次级别走势数据
/// 如果本地无缓存，自动从网络同步
#[tauri::command]
pub async fn get_sub_level_data(
    state: State<'_, AppState>,
    symbol: String,
    timeframe: String,
    start_dt: String,
    end_dt: String,
    enable_czsc: bool,
) -> Result<ChartData, String> {
    let tf = parse_timeframe(&timeframe).ok_or_else(|| format!("无效的时间周期: {}", timeframe))?;

    // 获取次级别周期
    let sub_tf = tf.sub_level().ok_or_else(|| format!("{:?} 无次级别周期", tf))?;

    // 阶段1：只持有读锁
    let (all_klines, name, need_sync) = {
        let manager = state.manager.read().map_err(|e| e.to_string())?;
        let klines = manager.get_klines(&symbol, sub_tf, Some(&start_dt), Some(&end_dt))
            .map_err(|e| e.to_string())?;
        let name = manager.get_stock_info(&symbol).map(|i| i.name).unwrap_or_default();
        // 如果该级别完全没有本地数据，需要同步
        let need_sync = {
            let full_klines = manager.get_klines(&symbol, sub_tf, None, None)
                .map_err(|e| e.to_string())?;
            full_klines.is_empty()
        };
        (klines, name, need_sync)
    };

    // 阶段2：如果本地无数据，自动同步
    let all_klines = if need_sync {
        let data_dir = {
            let manager = state.manager.read().map_err(|e| e.to_string())?;
            manager.data_dir().to_path_buf()
        };

        let sym = symbol.clone();
        let sub_tf_val = sub_tf;
        let sync_result = tauri::async_runtime::spawn_blocking(move || {
            yifang_data::sync_stock(&data_dir, &sym, &[sub_tf_val], "2020-01-01", true)
        }).await.map_err(|e| e.to_string())?;

        eprintln!("[次级别自动同步] {} {:?} 结果: {:?}", symbol, sub_tf, sync_result.levels.iter().map(|l| format!("{}={}", l.level, l.status)).collect::<Vec<_>>());

        // 同步完成后重新读取（带日期过滤）
        let manager = state.manager.read().map_err(|e| e.to_string())?;
        manager.get_klines(&symbol, sub_tf, Some(&start_dt), Some(&end_dt))
            .map_err(|e| e.to_string())?
    } else {
        all_klines
    };

    // 计算 MACD
    let macd = calc_macd(&all_klines, 12, 26, 9);

    // 缠论分析
    let czsc = if enable_czsc {
        Some(CzscAnalyzer::analyze(&all_klines, &macd))
    } else {
        None
    };

    Ok(ChartData {
        symbol,
        name,
        timeframe: sub_tf,
        klines: all_klines,
        macd,
        czsc,
        wyckoff: None,
        fusion: None,
    })
}

/// 保存分析判定报告到文件
///
/// 在程序运行目录创建 analysis_reports 文件夹，
/// 生成以"股票名称_时间级别.md"命名的判定报告，
/// 记录每个买卖点的判定理由。
#[tauri::command]
pub fn save_analysis_report(
    symbol: String,
    name: String,
    timeframe: String,
    chart_data: ChartData,
) -> Result<String, String> {
    use std::io::Write;

    // 确定保存目录：桌面下的 analysis_reports 文件夹
    // macOS 打包后 current_dir() 指向只读的 App Bundle，必须用可写目录
    let report_dir = match std::env::var("HOME") {
        Ok(home) => std::path::PathBuf::from(home).join("Desktop").join("analysis_reports"),
        Err(_) => {
            let cwd = std::env::current_dir().map_err(|e| e.to_string())?;
            cwd.join("analysis_reports")
        }
    };
    std::fs::create_dir_all(&report_dir).map_err(|e| e.to_string())?;

    // 生成文件名：股票名称_时间级别_日期.md
    let today = chrono::Local::now().format("%Y%m%d").to_string();
    let tf_label = match timeframe.as_str() {
        "d" => "日线", "w" => "周线", "m" => "月线",
        "f60" => "60分钟", "f30" => "30分钟", "f15" => "15分钟",
        "f5" => "5分钟", "f1" => "1分钟",
        _ => &timeframe,
    };
    let filename = format!("{}_{}_{}.md", name, tf_label, today);
    let filepath = report_dir.join(&filename);

    let mut f = std::fs::File::create(&filepath).map_err(|e| e.to_string())?;

    // ── 报告头部 ──
    writeln!(f, "# 缠论买卖点判定报告").map_err(|e| e.to_string())?;
    writeln!(f).map_err(|e| e.to_string())?;
    writeln!(f, "| 项目 | 内容 |").map_err(|e| e.to_string())?;
    writeln!(f, "|------|------|").map_err(|e| e.to_string())?;
    writeln!(f, "| 股票 | {} ({}) |", name, symbol).map_err(|e| e.to_string())?;
    writeln!(f, "| 时间级别 | {} |", tf_label).map_err(|e| e.to_string())?;
    writeln!(f, "| 生成时间 | {} |", chrono::Local::now().format("%Y-%m-%d %H:%M:%S")).map_err(|e| e.to_string())?;
    writeln!(f, "| K线根数 | {} |", chart_data.klines.len()).map_err(|e| e.to_string())?;
    writeln!(f).map_err(|e| e.to_string())?;

    // ── 缠论分析结果 ──
    if let Some(ref czsc) = chart_data.czsc {
        writeln!(f, "## 缠论分析概览").map_err(|e| e.to_string())?;
        writeln!(f).map_err(|e| e.to_string())?;
        writeln!(f, "| 指标 | 数量 |").map_err(|e| e.to_string())?;
        writeln!(f, "|------|------|").map_err(|e| e.to_string())?;
        writeln!(f, "| 笔 | {} |", czsc.bi.len()).map_err(|e| e.to_string())?;
        writeln!(f, "| 线段 | {} |", czsc.xd.len()).map_err(|e| e.to_string())?;
        writeln!(f, "| 笔中枢 | {} |", czsc.bi_zs.len()).map_err(|e| e.to_string())?;
        writeln!(f, "| 线段中枢 | {} |", czsc.xd_zs.len()).map_err(|e| e.to_string())?;
        writeln!(f, "| 背驰 | {} |", czsc.beichi.len()).map_err(|e| e.to_string())?;
        writeln!(f, "| 买卖点 | {} |", czsc.buy_sell.len()).map_err(|e| e.to_string())?;
        writeln!(f, "| 走势 | {} |", czsc.zoushi.len()).map_err(|e| e.to_string())?;
        writeln!(f).map_err(|e| e.to_string())?;

        // ── 笔列表 ──
        writeln!(f, "## 笔列表").map_err(|e| e.to_string())?;
        writeln!(f).map_err(|e| e.to_string())?;
        writeln!(f, "| # | 方向 | 起点 | 终点 | 起始价 | 结束价 | 起始日 | 结束日 |").map_err(|e| e.to_string())?;
        writeln!(f, "|---|------|------|------|--------|--------|--------|--------|").map_err(|e| e.to_string())?;
        for (i, bi) in czsc.bi.iter().enumerate() {
            writeln!(f, "| {} | {} | {} | {} | {:.2} | {:.2} | {} | {} |",
                i, bi.direction, bi.start_index, bi.end_index,
                bi.start_price, bi.end_price, bi.start_dt, bi.end_dt
            ).map_err(|e| e.to_string())?;
        }
        writeln!(f).map_err(|e| e.to_string())?;

        // ── 笔中枢列表 ──
        if !czsc.bi_zs.is_empty() {
            writeln!(f, "## 笔中枢列表").map_err(|e| e.to_string())?;
            writeln!(f).map_err(|e| e.to_string())?;
            writeln!(f, "| # | ZG | ZD | GG | DD | 区间(idx) | 区间(日期) |").map_err(|e| e.to_string())?;
            writeln!(f, "|---|------|------|------|------|-----------|------------|").map_err(|e| e.to_string())?;
            for (i, zs) in czsc.bi_zs.iter().enumerate() {
                writeln!(f, "| {} | {:.2} | {:.2} | {:.2} | {:.2} | {}-{} | {}-{} |",
                    i, zs.zg, zs.zd, zs.gg, zs.dd,
                    zs.start_index, zs.end_index, zs.start_dt, zs.end_dt
                ).map_err(|e| e.to_string())?;
            }
            writeln!(f).map_err(|e| e.to_string())?;
        }

        // ── 买卖点详情 ──
        writeln!(f, "## 买卖点详情").map_err(|e| e.to_string())?;
        writeln!(f).map_err(|e| e.to_string())?;

        // 按类型分组
        let bs_types = ["1buy","2buy","2buy_break","3buy","2+3buy","2+3buy_break",
                        "1sell","2sell","2sell_break","3sell","2+3sell","2+3sell_break"];
        let bs_labels: std::collections::HashMap<&str, &str> = [
            ("1buy","一买"),("2buy","二买"),("2buy_break","破位二买"),("3buy","三买"),
            ("2+3buy","二三买重合"),("2+3buy_break","二三买重合(破位)"),
            ("1sell","一卖"),("2sell","二卖"),("2sell_break","破位二卖"),("3sell","三卖"),
            ("2+3sell","二三卖重合"),("2+3sell_break","二三卖重合(破位)"),
        ].iter().cloned().collect();

        let mut has_any = false;
        for bs_type in &bs_types {
            let points: Vec<_> = czsc.buy_sell.iter()
                .filter(|p| p.bs_type == *bs_type)
                .collect();
            if points.is_empty() { continue; }
            has_any = true;

            let label = bs_labels.get(bs_type).unwrap_or(bs_type);
            writeln!(f, "### {}", label).map_err(|e| e.to_string())?;
            writeln!(f).map_err(|e| e.to_string())?;
            writeln!(f, "| # | 日期 | 价格 | 判定理由 |").map_err(|e| e.to_string())?;
            writeln!(f, "|---|------|------|----------|").map_err(|e| e.to_string())?;
            for (i, bs) in points.iter().enumerate() {
                writeln!(f, "| {} | {} | {:.2} | {} |",
                    i + 1, bs.dt, bs.price, bs.reason
                ).map_err(|e| e.to_string())?;
            }
            writeln!(f).map_err(|e| e.to_string())?;
        }

        if !has_any {
            writeln!(f, "（该级别无买卖点信号）").map_err(|e| e.to_string())?;
            writeln!(f).map_err(|e| e.to_string())?;
        }
    } else {
        writeln!(f, "（缠论分析未启用）").map_err(|e| e.to_string())?;
    }

    Ok(filepath.to_string_lossy().to_string())
}

/// 解析时间周期字符串
fn parse_timeframe(s: &str) -> Option<TimeFrame> {
    match s.to_lowercase().as_str() {
        "m" | "月线" | "month" => Some(TimeFrame::M),
        "w" | "周线" | "week" => Some(TimeFrame::W),
        "d" | "日线" | "day" => Some(TimeFrame::D),
        "f60" | "60f" | "60分钟" => Some(TimeFrame::F60),
        "f30" | "30f" | "30分钟" => Some(TimeFrame::F30),
        "f15" | "15f" | "15分钟" => Some(TimeFrame::F15),
        "f5" | "5f" | "5分钟" => Some(TimeFrame::F5),
        "f1" | "1f" | "1分钟" => Some(TimeFrame::F1),
        _ => None,
    }
}

// ===== 数据同步命令 =====

/// 获取数据目录整体状态
#[tauri::command]
pub async fn get_data_status(state: State<'_, AppState>) -> Result<DataStatus, String> {
    let data_dir = {
        let manager = state.manager.read().map_err(|e| e.to_string())?;
        manager.data_dir().to_path_buf()
    };
    // 释放读写锁后，在阻塞线程执行文件扫描，避免阻塞 Tauri 主线程
    tauri::async_runtime::spawn_blocking(move || {
        yifang_data::get_data_status(&data_dir)
    }).await.map_err(|e| e.to_string())
}

/// 同步单只股票的 K 线数据
#[tauri::command]
pub fn sync_stock(
    state: State<'_, AppState>,
    symbol: String,
    levels: Vec<String>,
    start_date: Option<String>,
    force: bool,
) -> Result<SyncStockResult, String> {
    let data_dir = {
        let manager = state.manager.read().map_err(|e| e.to_string())?;
        manager.data_dir().to_path_buf()
    };

    let tf_list: Vec<TimeFrame> = levels
        .iter()
        .filter_map(|s| parse_timeframe(s))
        .collect();

    if tf_list.is_empty() {
        return Err("未指定有效的 K 线级别".into());
    }

    let start = start_date.unwrap_or_else(|| "2024-01-01".into());

    Ok(yifang_data::sync_stock(&data_dir, &symbol, &tf_list, &start, force))
}

/// 批量同步股票数据（按股票代码列表）
#[tauri::command]
pub fn sync_stocks_batch(
    state: State<'_, AppState>,
    symbols: Vec<String>,
    levels: Vec<String>,
    start_date: Option<String>,
    force: bool,
) -> Result<Vec<SyncStockResult>, String> {
    let data_dir = {
        let manager = state.manager.read().map_err(|e| e.to_string())?;
        manager.data_dir().to_path_buf()
    };

    let tf_list: Vec<TimeFrame> = levels
        .iter()
        .filter_map(|s| parse_timeframe(s))
        .collect();

    if tf_list.is_empty() {
        return Err("未指定有效的 K 线级别".into());
    }

    let start = start_date.unwrap_or_else(|| "2024-01-01".into());

    let results: Vec<SyncStockResult> = symbols
        .iter()
        .map(|sym| yifang_data::sync_stock(&data_dir, sym, &tf_list, &start, force))
        .collect();

    Ok(results)
}

/// 获取所有股票代码列表
#[tauri::command]
pub fn get_all_stock_codes(state: State<'_, AppState>) -> Result<Vec<String>, String> {
    let manager = state.manager.read().map_err(|e| e.to_string())?;
    let data_dir = manager.data_dir();
    Ok(yifang_data::get_all_stock_codes(data_dir))
}

// ===== 数据校验命令 =====

/// 校验单只股票的数据完整性
#[tauri::command]
pub fn validate_stock(
    state: State<'_, AppState>,
    symbol: String,
    levels: Vec<String>,
) -> Result<ValidateStockResult, String> {
    let data_dir = {
        let manager = state.manager.read().map_err(|e| e.to_string())?;
        manager.data_dir().to_path_buf()
    };

    let tf_list: Vec<TimeFrame> = levels
        .iter()
        .filter_map(|s| parse_timeframe(s))
        .collect();

    if tf_list.is_empty() {
        return Err("未指定有效的 K 线级别".into());
    }

    Ok(yifang_data::validate_stock(&data_dir, &symbol, &tf_list))
}

/// 校验单只股票单级别数据
#[tauri::command]
pub fn validate_stock_level(
    state: State<'_, AppState>,
    symbol: String,
    level: String,
) -> Result<ValidateLevelResult, String> {
    let data_dir = {
        let manager = state.manager.read().map_err(|e| e.to_string())?;
        manager.data_dir().to_path_buf()
    };

    let tf = parse_timeframe(&level).ok_or_else(|| format!("无效的时间周期: {}", level))?;

    Ok(yifang_data::validate_stock_level(&data_dir, &symbol, tf))
}

/// 跨数据源交叉校验
#[tauri::command]
pub fn cross_validate_stock(
    symbol: String,
    level: String,
) -> Result<ValidateLevelResult, String> {
    let tf = parse_timeframe(&level).ok_or_else(|| format!("无效的时间周期: {}", level))?;

    Ok(yifang_data::cross_validate_stock(&symbol, tf))
}

/// 获取当前数据目录
#[tauri::command]
pub fn get_data_dir(state: State<'_, AppState>) -> Result<String, String> {
    let manager = state.manager.read().map_err(|e| e.to_string())?;
    Ok(manager.data_dir().to_string_lossy().to_string())
}

/// 设置数据目录（运行时切换，不移动数据）
#[tauri::command]
pub fn set_data_dir(state: State<'_, AppState>, path: String) -> Result<String, String> {
    let new_path = PathBuf::from(&path);
    if !new_path.exists() {
        std::fs::create_dir_all(&new_path)
            .map_err(|e| format!("创建目录失败: {}", e))?;
    }

    let mut manager = state.manager.write().map_err(|e| e.to_string())?;
    manager.set_data_dir(&path);
    Ok(manager.data_dir().to_string_lossy().to_string())
}

/// 移动数据到新目录（并切换数据目录）
#[tauri::command]
pub fn move_data_dir(state: State<'_, AppState>, new_path: String) -> Result<MoveDataResult, String> {
    let old_path = {
        let manager = state.manager.read().map_err(|e| e.to_string())?;
        manager.data_dir().to_path_buf()
    };

    let new_path_buf = PathBuf::from(&new_path);
    if !new_path_buf.exists() {
        std::fs::create_dir_all(&new_path_buf)
            .map_err(|e| format!("创建目标目录失败: {}", e))?;
    }

    let (moved, failed, errors) = yifang_data::KLineManager::move_data_to(&old_path, &new_path_buf)
        .map_err(|e| e.to_string())?;

    // 切换到新目录
    {
        let mut manager = state.manager.write().map_err(|e| e.to_string())?;
        manager.set_data_dir(&new_path);
    }

    Ok(MoveDataResult { moved, failed, errors })
}

/// 获取各板块统计
#[tauri::command]
pub fn get_board_stats(state: State<'_, AppState>) -> Result<Vec<BoardStats>, String> {
    let manager = state.manager.read().map_err(|e| e.to_string())?;
    let data_dir = manager.data_dir();
    Ok(yifang_data::get_board_stats(data_dir))
}

/// 获取板块在线信息（各板块在线股票数 + 本地已有数）
#[tauri::command]
pub async fn get_board_online_info(state: State<'_, AppState>) -> Result<Vec<BoardOnlineInfo>, String> {
    let data_dir = {
        let manager = state.manager.read().map_err(|e| e.to_string())?;
        manager.data_dir().to_path_buf()
    };
    // 释放读写锁后，在阻塞线程执行网络请求，避免阻塞 Tauri 主线程
    tauri::async_runtime::spawn_blocking(move || {
        yifang_data::get_board_online_info(&data_dir)
    }).await.map_err(|e| e.to_string())
}

/// 获取指定板块的股票代码列表（从在线 API 获取）
#[tauri::command]
pub fn get_stock_codes_by_board(board: String) -> Result<Vec<String>, String> {
    yifang_data::fetch_board_stock_codes(&board).map_err(|e| e.to_string())
}

/// 同步指定板块全部股票
#[tauri::command]
pub fn sync_board(
    state: State<'_, AppState>,
    board: String,
    levels: Vec<String>,
    start_date: Option<String>,
    force: bool,
) -> Result<Vec<SyncStockResult>, String> {
    let tfs: Vec<TimeFrame> = levels.iter().filter_map(|s| parse_timeframe(s)).collect();
    if tfs.is_empty() {
        return Err("未指定有效的 K 线级别".into());
    }
    let data_dir = {
        let manager = state.manager.read().map_err(|e| e.to_string())?;
        manager.data_dir().to_path_buf()
    };
    let start = start_date.unwrap_or_default();
    Ok(yifang_data::sync_board(&data_dir, &board, &tfs, &start, force))
}

// ═══════════════════════════════════════════════════════════
//  后台异步同步（支持自动重试，前端轮询进度）
// ═══════════════════════════════════════════════════════════

use crate::state::SyncProgress;

/// 启动后台异步同步（非阻塞）。前端通过 `get_sync_status` 轮询进度。
/// 所有耗时操作（快速检查、网络获取列表、同步）全在后台线程，command 线程立即返回。
#[tauri::command]
pub fn start_sync_board(
    state: State<'_, AppState>,
    board: String,
    levels: Vec<String>,
    start_date: Option<String>,
    force: bool,
) -> Result<(), String> {
    // 检查是否已在同步
    {
        let progress = state.sync_progress.lock().map_err(|e| e.to_string())?;
        if progress.running {
            return Err("已有同步任务在运行".into());
        }
    }

    let tfs: Vec<TimeFrame> = levels.iter().filter_map(|s| parse_timeframe(s)).collect();
    if tfs.is_empty() {
        return Err("未指定有效的 K 线级别".into());
    }

    let data_dir = {
        let manager = state.manager.read().map_err(|e| e.to_string())?;
        manager.data_dir().to_path_buf()
    };
    let start = start_date.unwrap_or_else(|| "2024-01-01".into());

    // 初始化进度：进入 preparing 阶段，后台线程完成所有工作
    {
        let mut progress = state.sync_progress.lock().map_err(|e| e.to_string())?;
        *progress = SyncProgress {
            running: true,
            board: board.clone(),
            levels: levels.clone(),
            total: 0,
            completed: 0,
            success: 0,
            failures: Vec::new(),
            retrying: false,
            retry_round: 0,
            cancelled: false,
            current_symbols: Vec::new(),
            preparing: true,
            prepare_error: String::new(),
            all_skipped: false,
            skipped_count: 0,
            latest_date: String::new(),
        };
    }

    // 所有工作在后台线程完成，command 线程立即返回
    let progress_state = state.sync_progress.clone();
    let last_failures_state = state.last_sync_failures.clone();
    std::thread::spawn(move || {
        // ── 阶段0：快速本地检查（非 force 模式） ──
        if !force {
            let (total, today_count, all_up_to_date, latest_date) =
                yifang_data::quick_check_board_up_to_date(&data_dir, &board, &tfs);

            if all_up_to_date && total > 0 {
                eprintln!("[快速检查] 板块 {} 本地 {} 只股票已是最新（最近同步: {}），跳过", board, total, latest_date);
                let mut p = progress_state.lock().unwrap();
                *p = SyncProgress {
                    running: false,
                    board: board.clone(),
                    levels: levels.clone(),
                    total,
                    completed: total,
                    success: total,
                    failures: Vec::new(),
                    retrying: false,
                    retry_round: 0,
                    cancelled: false,
                    current_symbols: Vec::new(),
                    preparing: false,
                    prepare_error: String::new(),
                    all_skipped: true,
                    skipped_count: total,
                    latest_date,
                };
                return;
            }

            if total > 0 {
                eprintln!("[快速检查] 板块 {} 本地 {}/{} 只已是最新，需要增量同步", board, today_count, total);
            }
        }

        // ── 阶段1：获取股票列表（网络请求） ──
        let codes = match yifang_data::fetch_board_stock_codes(&board) {
            Ok(codes) => codes,
            Err(e) => {
                let mut p = progress_state.lock().unwrap();
                p.running = false;
                p.preparing = false;
                p.prepare_error = format!("获取板块 {} 股票列表失败: {}", board, e);
                eprintln!("[同步] {}", p.prepare_error);
                return;
            }
        };

        if codes.is_empty() {
            let mut p = progress_state.lock().unwrap();
            p.running = false;
            p.preparing = false;
            p.prepare_error = format!("板块 {} 没有可同步的股票", board);
            return;
        }

        // ── 阶段2：更新进度，退出 preparing 状态，开始同步 ──
        {
            let mut p = progress_state.lock().unwrap();
            p.preparing = false;
            p.total = codes.len();
        }

        eprintln!("[后台同步] 板块 {} 获取到 {} 只股票，开始同步", board, codes.len());
        run_sync_parallel(&progress_state, &last_failures_state, &data_dir, &codes, &tfs, &start, force, 4);
        eprintln!("[后台同步] 板块 {} 同步完成", board);
    });

    Ok(())
}

/// 获取后台同步的当前状态
#[tauri::command]
pub fn get_sync_status(state: State<'_, AppState>) -> Result<SyncProgress, String> {
    let progress = state.sync_progress.lock().map_err(|e| e.to_string())?;
    Ok(progress.clone())
}

/// 启动时自动同步 — 对所有板块做增量同步
/// 与 start_sync_board 类似，但会按板块依次同步
/// 自动清理退市股数据 + 发现新股并同步
/// 所有网络请求（获取股票列表等）均在后台线程中完成，不阻塞 Tauri command 线程
#[tauri::command]
pub fn auto_sync_on_startup(
    state: State<'_, AppState>,
    levels: Vec<String>,
) -> Result<(), String> {
    // 检查是否已在同步
    {
        let progress = state.sync_progress.lock().map_err(|e| e.to_string())?;
        if progress.running {
            return Ok(()); // 已在同步，不重复启动
        }
    }

    let tfs: Vec<TimeFrame> = levels.iter().filter_map(|s| parse_timeframe(s)).collect();
    if tfs.is_empty() {
        return Ok(());
    }

    let data_dir = {
        let manager = state.manager.read().map_err(|e| e.to_string())?;
        manager.data_dir().to_path_buf()
    };
    let start = "2024-01-01".to_string();

    // 初始化进度：先进入 preparing 阶段
    {
        let mut progress = state.sync_progress.lock().map_err(|e| e.to_string())?;
        *progress = SyncProgress {
            running: true,
            board: "全部(增量)".into(),
            levels: levels.clone(),
            total: 0,
            completed: 0,
            success: 0,
            failures: Vec::new(),
            retrying: false,
            retry_round: 0,
            cancelled: false,
            current_symbols: Vec::new(),
            preparing: true,
            prepare_error: String::new(),
            all_skipped: false,
            skipped_count: 0,
            latest_date: String::new(),
        };
    }

    // ── 1. 清理退市股（异步不阻塞，失败不影响同步） ──
    let delisted_dir = data_dir.clone();
    std::thread::spawn(move || {
        match yifang_data::clean_delisted_stocks(&delisted_dir) {
            Ok((codes, files)) if !codes.is_empty() => {
                eprintln!("[启动同步] 清理退市股: {} 只, 删除 {} 个文件", codes.len(), files);
            }
            Ok(_) => eprintln!("[启动同步] 无退市股需要清理"),
            Err(e) => eprintln!("[启动同步] 清理退市股失败(不影响同步): {}", e),
        }
    });

    // ── 2. 所有网络获取 + 同步 在后台线程中完成 ──
    let progress_state = state.sync_progress.clone();
    let last_failures_state = state.last_sync_failures.clone();
    let tf_list = tfs;
    let data_dir_for_bg = {
        let manager = state.manager.read().map_err(|e| e.to_string())?;
        manager.data_dir().to_path_buf()
    };
    std::thread::spawn(move || {
        // 获取全量在市股票列表
        let online_codes = yifang_data::fetch_all_listed_codes().unwrap_or_else(|e| {
            eprintln!("[启动同步] 获取在线股票列表失败: {}, 回退到本地列表", e);
            yifang_data::get_all_stock_codes(&data_dir_for_bg)
        });

        if online_codes.is_empty() {
            // 本地没数据，从沪主板同步前100只作为引导
            eprintln!("[启动同步] 本地无数据，从沪主板同步引导数据...");
            let codes = yifang_data::fetch_board_stock_codes("sh_main").unwrap_or_default();
            let initial: Vec<String> = codes.into_iter().take(100).collect();
            if initial.is_empty() {
                let mut p = progress_state.lock().unwrap();
                p.running = false;
                p.preparing = false;
                p.prepare_error = "无法获取引导数据".into();
                return;
            }

            // 更新进度（引导数据）
            {
                let mut p = progress_state.lock().unwrap();
                p.preparing = false;
                p.board = "sh_main(引导)".into();
                p.total = initial.len();
            }

            let force = false;
            run_sync_parallel(&progress_state, &last_failures_state, &data_dir_for_bg, &initial, &tf_list, &start, force, 4);
            eprintln!("[启动同步] 引导数据同步完成");
            // 引导模式下也同步指数
            yifang_data::sync_all_indices(&data_dir_for_bg, &tf_list, &start, false);
            return;
        }

        // 本地有数据，增量更新
        eprintln!("[启动同步] 增量同步 {} 只股票（含新股）...", online_codes.len());
        {
            let mut p = progress_state.lock().unwrap();
            p.preparing = false;
            p.total = online_codes.len();
        }

        let force = false;
        run_sync_parallel(&progress_state, &last_failures_state, &data_dir_for_bg, &online_codes, &tf_list, &start, force, 4);
        eprintln!("[启动同步] 增量同步完成");

        // ── 3. 同步指数 & 板块指数 ──
        eprintln!("[启动同步] 开始同步指数及板块指数...");
        {
            let mut p = progress_state.lock().unwrap();
            p.board = "指数/板块".into();
            p.total = 0; // 不显示具体进度
        }
        yifang_data::sync_all_indices(&data_dir_for_bg, &tf_list, &start, false);
        eprintln!("[启动同步] 指数同步完成");
    });

    Ok(())
}

/// 内部函数：并行执行同步并自动重试失败项
/// concurrency: 并发线程数
fn run_sync_parallel(
    progress_state: &std::sync::Arc<std::sync::Mutex<SyncProgress>>,
    last_failures_state: &std::sync::Arc<std::sync::Mutex<Vec<SyncFailureRecord>>>,
    data_dir: &std::path::Path,
    codes: &[String],
    tf_list: &[TimeFrame],
    start: &str,
    force: bool,
    concurrency: usize,
) {
    // ── 将批量过滤合并到主同步循环中，让进度条实时可见 ──
    let total = codes.len();
    let idx_lock = std::sync::Arc::new(std::sync::Mutex::new(0usize));
    let all_skipped = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
    let mut handles = Vec::new();

    for _ in 0..concurrency {
        let idx_lock = idx_lock.clone();
        let codes_ref = codes.to_vec();
        let progress_state = progress_state.clone();
        let data_dir = data_dir.to_path_buf();
        let tf_list = tf_list.to_vec();
        let start = start.to_string();
        let all_skipped = all_skipped.clone();

        let handle = std::thread::spawn(move || {
            loop {
                // 检查取消
                {
                    let p = progress_state.lock().unwrap();
                    if p.cancelled {
                        return;
                    }
                }

                // 取下一个任务
                let i = {
                    let mut idx = idx_lock.lock().unwrap();
                    let current = *idx;
                    if current >= total {
                        return;
                    }
                    *idx += 1;
                    current
                };

                let symbol = &codes_ref[i];

                // ── 快速检查：数据是否已最新（含1小时快速路径 + parquet精确检查） ──
                if !force && yifang_data::is_stock_up_to_date(&data_dir, symbol, &tf_list) {
                    // 跳过，更新进度
                    let mut p = progress_state.lock().unwrap();
                    p.completed += 1;
                    p.success += 1;
                    p.skipped_count += 1;
                    continue;
                }

                // 有股票需要实际同步 → 不是全量跳过
                all_skipped.store(false, std::sync::atomic::Ordering::Relaxed);

                // 更新当前正在同步的股票
                {
                    let mut p = progress_state.lock().unwrap();
                    p.current_symbols.push(symbol.clone());
                }

                let result = yifang_data::sync_stock(&data_dir, symbol, &tf_list, &start, force);

                // 更新进度
                {
                    let mut p = progress_state.lock().unwrap();
                    p.current_symbols.retain(|s| s != symbol);
                    p.completed += 1;
                    let mut has_failure = false;
                    for lv in &result.levels {
                        if lv.status != "ok" && lv.status != "skip" {
                            has_failure = true;
                            p.failures.push((result.symbol.clone(), lv.level.clone(), lv.msg.clone()));
                        }
                    }
                    if !has_failure {
                        p.success += 1;
                    }
                }

                // 只对实际请求了网络的股票（非 skip）加小间隔防限流
                let all_skipped_levels = result.levels.iter().all(|lv| lv.status == "skip");
                if !all_skipped_levels {
                    std::thread::sleep(std::time::Duration::from_millis(50));
                }
            }
        });
        handles.push(handle);
    }

    for h in handles {
        let _ = h.join();
    }

    {
        let p = progress_state.lock().unwrap();
        if p.cancelled {
            let mut p = progress_state.lock().unwrap();
            p.running = false;
            return;
        }
    }

    // ── 主同步完成，保存失败记录到 last_sync_failures ──
    {
        let mut p = progress_state.lock().unwrap();
        let failed: Vec<SyncFailureRecord> = p.failures.iter().map(|(sym, lv, msg)| SyncFailureRecord {
            symbol: sym.clone(),
            level: lv.clone(),
            msg: msg.clone(),
        }).collect();

        // 保存失败记录（用户可稍后通过 retry_failed_syncs 重试）
        let mut last = last_failures_state.lock().unwrap();
        *last = failed;

        p.retrying = false;
        p.running = false;
        p.all_skipped = all_skipped.load(std::sync::atomic::Ordering::Relaxed);
        // 如果全部跳过，skipped_count 已在循环中累计
        // 同步完成后更新 latest_date（从本地文件 mtime 推算最新日期）
        if p.latest_date.is_empty() {
            p.latest_date = get_latest_mtime_date(data_dir);
        }
    }
}

/// 扫描 kline_cache 目录，返回最新的 parquet 文件修改日期 (YYYY-MM-DD)
fn get_latest_mtime_date(data_dir: &std::path::Path) -> String {
    let cache_dir = data_dir.join("kline_cache");
    if !cache_dir.exists() { return String::new(); }
    let mut latest = String::new();
    if let Ok(entries) = std::fs::read_dir(&cache_dir) {
        for level_dir in entries.flatten() {
            let level_path = level_dir.path();
            if !level_path.is_dir() { continue; }
            if let Ok(files) = std::fs::read_dir(&level_path) {
                for file in files.flatten() {
                    let path = file.path();
                    if path.extension().and_then(|e| e.to_str()) != Some("parquet") { continue; }
                    if let Ok(meta) = file.metadata() {
                        if let Ok(modified) = meta.modified() {
                            let dt: chrono::DateTime<chrono::Local> = modified.into();
                            let date_str = dt.format("%Y-%m-%d").to_string();
                            if date_str > latest {
                                latest = date_str;
                            }
                        }
                    }
                }
            }
        }
    }
    latest
}

/// 在系统文件管理器中打开数据存储目录
#[tauri::command]
pub async fn open_data_dir(state: State<'_, AppState>) -> Result<(), String> {
    let dir = {
        let manager = state.manager.read().map_err(|e| e.to_string())?;
        manager.data_dir().to_path_buf()
    };

    if !dir.exists() {
        std::fs::create_dir_all(&dir)
            .map_err(|e| format!("创建目录失败: {}", e))?;
    }

    // 使用 tauri-plugin-opener 的 open_path 命令
    tauri_plugin_opener::open_path(dir, None::<&str>)
        .map_err(|e| format!("打开目录失败: {}", e))
}

/// 取消后台同步
#[tauri::command]
pub fn cancel_sync(state: State<'_, AppState>) -> Result<(), String> {
    let mut progress = state.sync_progress.lock().map_err(|e| e.to_string())?;
    progress.cancelled = true;
    progress.running = false;
    Ok(())
}

// ═══════════════════════════════════════════════════════════
//  单股票按需同步（图表切换 / 历史扩展）
// ═══════════════════════════════════════════════════════════

/// 触发后台同步单只股票单级别数据。
/// 用于图表切换时无数据自动同步，以及光标左移时扩展历史数据。
/// 返回后可通过 `poll_single_sync` 轮询进度。
#[tauri::command]
pub async fn trigger_single_sync(
    state: State<'_, AppState>,
    symbol: String,
    timeframe: String,
    start_date: Option<String>,  // None = 同步到最新; Some("2015-01-01") = 扩展历史
) -> Result<(), String> {
    let tf = parse_timeframe(&timeframe).ok_or_else(|| format!("无效的时间周期: {}", timeframe))?;
    let data_dir = {
        let manager = state.manager.read().map_err(|e| e.to_string())?;
        manager.data_dir().to_path_buf()
    };

    // 设置状态: running
    {
        let mut states = state.single_sync_states.lock().map_err(|e| e.to_string())?;
        // 移除旧的同 symbol+timeframe 状态
        states.retain(|s| !(s.symbol == symbol && s.timeframe == timeframe));
        states.push(SingleSyncState {
            symbol: symbol.clone(),
            timeframe: timeframe.clone(),
            running: true,
            done: false,
            status: String::new(),
            count: 0,
            msg: String::new(),
        });
    }

    let sym = symbol.clone();
    let tf_str = timeframe.clone();
    let states_arc = state.single_sync_states.clone();
    let start = start_date.unwrap_or_else(|| "2020-01-01".to_string());

    // 后台线程同步
    tauri::async_runtime::spawn_blocking(move || {
        let result = yifang_data::sync_stock(&data_dir, &sym, &[tf], &start, true);
        eprintln!("[按需同步] {} {:?} start={} result={:?}", sym, tf, start,
            result.levels.iter().map(|l| format!("{}={}", l.level, l.status)).collect::<Vec<_>>());

        // 更新状态: done
        if let Ok(mut states) = states_arc.lock() {
            if let Some(s) = states.iter_mut().find(|s| s.symbol == sym && s.timeframe == tf_str) {
                s.running = false;
                s.done = true;
                if let Some(level) = result.levels.first() {
                    s.status = level.status.clone();
                    s.count = level.count;
                    s.msg = level.msg.clone();
                } else {
                    s.status = "fail".to_string();
                    s.msg = "no level result".to_string();
                }
            }
        }
    });

    Ok(())
}

/// 轮询单股票同步状态。完成后前端重新加载图表数据。
#[tauri::command]
pub fn poll_single_sync(
    state: State<'_, AppState>,
    symbol: String,
    timeframe: String,
) -> Result<SingleSyncState, String> {
    let states = state.single_sync_states.lock().map_err(|e| e.to_string())?;
    let found = states.iter().find(|s| s.symbol == symbol && s.timeframe == timeframe);
    match found {
        Some(s) => Ok(s.clone()),
        None => Ok(SingleSyncState {
            symbol,
            timeframe,
            running: false,
            done: false,
            status: String::new(),
            count: 0,
            msg: String::new(),
        }),
    }
}

/// 清理退市股数据：对比本地与在线在市列表，删除已退市股票的所有 K 线文件
/// 返回 { delisted_codes: string[], removed_files: number }
#[tauri::command]
pub fn clean_delisted_stocks(state: State<'_, AppState>) -> Result<serde_json::Value, String> {
    let data_dir = {
        let manager = state.manager.read().map_err(|e| e.to_string())?;
        manager.data_dir().to_path_buf()
    };
    let (codes, files) = yifang_data::clean_delisted_stocks(&data_dir).map_err(|e| e.to_string())?;
    Ok(serde_json::json!({
        "delisted_codes": codes,
        "removed_files": files,
    }))
}

/// 清空所有 K 线数据
#[tauri::command]
pub fn clear_all_data(state: State<'_, AppState>) -> Result<usize, String> {
    let data_dir = {
        let manager = state.manager.read().map_err(|e| e.to_string())?;
        manager.data_dir().to_path_buf()
    };
    yifang_data::clear_all_data(&data_dir).map_err(|e| e.to_string())
}

/// 清理过期数据
/// retention: { tf_dir_name: months } 例如 { "1m": 3, "5m": 3, "15m": 6, "30m": 6 }
#[tauri::command]
pub fn trim_old_data(state: State<'_, AppState>, retention: std::collections::HashMap<String, u32>) -> Result<yifang_data::TrimResult, String> {
    let data_dir = {
        let manager = state.manager.read().map_err(|e| e.to_string())?;
        manager.data_dir().to_path_buf()
    };
    yifang_data::trim_old_data(&data_dir, &retention).map_err(|e| e.to_string())
}

// ═══════════════════════════════════════════════════════════
//  失败重试（独立于主同步流程）
// ═══════════════════════════════════════════════════════════

/// 获取上一次同步的失败列表（持久化，同步结束后仍保留）
#[tauri::command]
pub fn get_last_sync_failures(state: State<'_, AppState>) -> Result<Vec<SyncFailureRecord>, String> {
    let failures = state.last_sync_failures.lock().map_err(|e| e.to_string())?;
    Ok(failures.clone())
}

/// 清除失败记录
#[tauri::command]
pub fn clear_sync_failures(state: State<'_, AppState>) -> Result<(), String> {
    let mut failures = state.last_sync_failures.lock().map_err(|e| e.to_string())?;
    failures.clear();
    Ok(())
}

/// 重试失败列表中的所有项。在后台线程中执行，前端通过 `get_sync_status` 轮询进度。
/// 重试成功的项从失败列表中移除。
#[tauri::command]
pub fn retry_failed_syncs(state: State<'_, AppState>, start_date: Option<String>) -> Result<(), String> {
    // 检查是否已在同步
    {
        let progress = state.sync_progress.lock().map_err(|e| e.to_string())?;
        if progress.running {
            return Err("已有同步任务在运行".into());
        }
    }

    // 取出待重试的失败记录
    let failures: Vec<SyncFailureRecord> = {
        let f = state.last_sync_failures.lock().map_err(|e| e.to_string())?;
        f.clone()
    };
    if failures.is_empty() {
        return Ok(()); // 没有失败项，直接返回
    }

    // 按 symbol 分组，提取唯一代码列表和对应的级别
    let mut symbol_levels: std::collections::HashMap<String, Vec<TimeFrame>> = std::collections::HashMap::new();
    for rec in &failures {
        if let Some(tf) = parse_timeframe(&rec.level) {
            symbol_levels.entry(rec.symbol.clone()).or_default().push(tf);
        }
    }
    let codes: Vec<String> = symbol_levels.keys().cloned().collect();
    if codes.is_empty() {
        return Ok(());
    }

    let data_dir = {
        let manager = state.manager.read().map_err(|e| e.to_string())?;
        manager.data_dir().to_path_buf()
    };
    let start = start_date.unwrap_or_else(|| "2024-01-01".into());

    // 初始化进度
    {
        let mut progress = state.sync_progress.lock().map_err(|e| e.to_string())?;
        *progress = SyncProgress {
            running: true,
            board: "失败重试".into(),
            levels: failures.iter().map(|f| f.level.clone()).collect::<std::collections::HashSet<_>>().into_iter().collect(),
            total: codes.len(),
            completed: 0,
            success: 0,
            failures: Vec::new(),
            retrying: true,
            retry_round: 1,
            cancelled: false,
            current_symbols: Vec::new(),
            preparing: false,
            prepare_error: String::new(),
            all_skipped: false,
            skipped_count: 0,
            latest_date: String::new(),
        };
    }

    // 为每个 symbol 构建其需要重试的 tf_list
    let symbol_levels = symbol_levels;
    let progress_state = state.sync_progress.clone();
    let last_failures_state = state.last_sync_failures.clone();

    std::thread::spawn(move || {
        let total = codes.len();
        let idx_lock = std::sync::Arc::new(std::sync::Mutex::new(0usize));
        let mut handles = Vec::new();

        // 2 线程并发重试
        for _ in 0..2usize.min(total) {
            let idx_lock = idx_lock.clone();
            let codes_ref = codes.clone();
            let symbol_levels = symbol_levels.clone();
            let progress_state = progress_state.clone();
            let data_dir = data_dir.clone();
            let start = start.clone();

            let handle = std::thread::spawn(move || {
                loop {
                    {
                        let p = progress_state.lock().unwrap();
                        if p.cancelled { return; }
                    }

                    let i = {
                        let mut idx = idx_lock.lock().unwrap();
                        let current = *idx;
                        *idx += 1;
                        current
                    };

                    if i >= total { return; }

                    let symbol = &codes_ref[i];
                    let tf_list = symbol_levels.get(symbol).cloned().unwrap_or_default();
                    if tf_list.is_empty() { continue; }

                    {
                        let mut p = progress_state.lock().unwrap();
                        p.current_symbols.push(symbol.clone());
                    }

                    let result = yifang_data::sync_stock(&data_dir, symbol, &tf_list, &start, true);

                    {
                        let mut p = progress_state.lock().unwrap();
                        p.current_symbols.retain(|s| s != symbol);
                        p.completed += 1;
                        let mut has_failure = false;
                        for lv in &result.levels {
                            if lv.status != "ok" && lv.status != "skip" {
                                has_failure = true;
                                p.failures.push((result.symbol.clone(), lv.level.clone(), lv.msg.clone()));
                            }
                        }
                        if !has_failure {
                            p.success += 1;
                        }
                    }

                    // 重试间隔 300ms
                    std::thread::sleep(std::time::Duration::from_millis(300));
                }
            });
            handles.push(handle);
        }

        for h in handles {
            let _ = h.join();
        }

        // 重试完成：更新 last_sync_failures（移除已成功的，保留仍失败的）
        {
            let mut p = progress_state.lock().unwrap();
            if p.cancelled {
                p.running = false;
                return;
            }

            // 仍然失败的项
            let still_failed: Vec<SyncFailureRecord> = p.failures.iter().map(|(sym, lv, msg)| SyncFailureRecord {
                symbol: sym.clone(),
                level: lv.clone(),
                msg: msg.clone(),
            }).collect();

            let mut last = last_failures_state.lock().unwrap();
            *last = still_failed;

            p.retrying = false;
            p.running = false;
        }
    });

    Ok(())
}
