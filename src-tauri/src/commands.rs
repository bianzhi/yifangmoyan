//! Tauri IPC 命令

use std::path::PathBuf;
use tauri::State;

use crate::state::AppState;
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
    let manager = state.manager.lock().map_err(|e| e.to_string())?;
    manager.get_klines(&symbol, tf, None, None).map_err(|e| e.to_string())
}

/// 获取完整图表数据（K线 + MACD + 缠论 + 威科夫）
#[tauri::command]
pub fn get_chart_data(
    state: State<'_, AppState>,
    symbol: String,
    timeframe: String,
    enable_czsc: bool,
    enable_wyckoff: bool,
) -> Result<ChartData, String> {
    let tf = parse_timeframe(&timeframe).ok_or_else(|| format!("无效的时间周期: {}", timeframe))?;

    let manager = state.manager.lock().map_err(|e| e.to_string())?;

    // 1. 获取 K 线数据
    let klines = manager.get_klines(&symbol, tf, None, None).map_err(|e| e.to_string())?;
    let name = manager.get_stock_info(&symbol).map(|i| i.name).unwrap_or_default();

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
    let manager = state.manager.lock().map_err(|e| e.to_string())?;
    manager.search_stocks(&keyword).map_err(|e| e.to_string())
}

/// 获取股票信息
#[tauri::command]
pub fn get_stock_info(
    state: State<'_, AppState>,
    symbol: String,
) -> Result<StockInfo, String> {
    let manager = state.manager.lock().map_err(|e| e.to_string())?;
    manager.get_stock_info(&symbol).map_err(|e| e.to_string())
}

/// 获取线段对应的次级别走势数据（架构预留）
#[tauri::command]
pub fn get_sub_level_data(
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

    let manager = state.manager.lock().map_err(|e| e.to_string())?;

    // 获取次级别 K 线
    let all_klines = manager.get_klines(&symbol, sub_tf, Some(&start_dt), Some(&end_dt))
        .map_err(|e| e.to_string())?;

    let name = manager.get_stock_info(&symbol).map(|i| i.name).unwrap_or_default();

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
pub fn get_data_status(state: State<'_, AppState>) -> Result<DataStatus, String> {
    let manager = state.manager.lock().map_err(|e| e.to_string())?;
    let data_dir = manager.data_dir();
    Ok(yifang_data::get_data_status(data_dir))
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
            let manager = state.manager.lock().map_err(|e| e.to_string())?;
            manager.data_dir().to_path_buf()
        };

    let tf_list: Vec<TimeFrame> = levels
        .iter()
        .filter_map(|s| parse_timeframe(s))
        .collect();

    if tf_list.is_empty() {
        return Err("未指定有效的 K 线级别".into());
    }

    let start = start_date.unwrap_or_else(|| "2023-01-01".into());

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
            let manager = state.manager.lock().map_err(|e| e.to_string())?;
            manager.data_dir().to_path_buf()
        };

    let tf_list: Vec<TimeFrame> = levels
        .iter()
        .filter_map(|s| parse_timeframe(s))
        .collect();

    if tf_list.is_empty() {
        return Err("未指定有效的 K 线级别".into());
    }

    let start = start_date.unwrap_or_else(|| "2023-01-01".into());

    let results: Vec<SyncStockResult> = symbols
        .iter()
        .map(|sym| yifang_data::sync_stock(&data_dir, sym, &tf_list, &start, force))
        .collect();

    Ok(results)
}

/// 获取所有股票代码列表
#[tauri::command]
pub fn get_all_stock_codes(state: State<'_, AppState>) -> Result<Vec<String>, String> {
    let manager = state.manager.lock().map_err(|e| e.to_string())?;
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
    let manager = state.manager.lock().map_err(|e| e.to_string())?;
    let data_dir = manager.data_dir();

    let tf_list: Vec<TimeFrame> = levels
        .iter()
        .filter_map(|s| parse_timeframe(s))
        .collect();

    if tf_list.is_empty() {
        return Err("未指定有效的 K 线级别".into());
    }

    Ok(yifang_data::validate_stock(data_dir, &symbol, &tf_list))
}

/// 校验单只股票单级别数据
#[tauri::command]
pub fn validate_stock_level(
    state: State<'_, AppState>,
    symbol: String,
    level: String,
) -> Result<ValidateLevelResult, String> {
    let manager = state.manager.lock().map_err(|e| e.to_string())?;
    let data_dir = manager.data_dir();

    let tf = parse_timeframe(&level).ok_or_else(|| format!("无效的时间周期: {}", level))?;

    Ok(yifang_data::validate_stock_level(data_dir, &symbol, tf))
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
    let manager = state.manager.lock().map_err(|e| e.to_string())?;
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

    let mut manager = state.manager.lock().map_err(|e| e.to_string())?;
    manager.set_data_dir(&path);
    Ok(manager.data_dir().to_string_lossy().to_string())
}

/// 移动数据到新目录（并切换数据目录）
#[tauri::command]
pub fn move_data_dir(state: State<'_, AppState>, new_path: String) -> Result<MoveDataResult, String> {
    let old_path = {
        let manager = state.manager.lock().map_err(|e| e.to_string())?;
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
        let mut manager = state.manager.lock().map_err(|e| e.to_string())?;
        manager.set_data_dir(&new_path);
    }

    Ok(MoveDataResult { moved, failed, errors })
}

/// 获取各板块统计
#[tauri::command]
pub fn get_board_stats(state: State<'_, AppState>) -> Result<Vec<BoardStats>, String> {
    let manager = state.manager.lock().map_err(|e| e.to_string())?;
    let data_dir = manager.data_dir();
    Ok(yifang_data::get_board_stats(data_dir))
}

/// 获取板块在线信息（各板块在线股票数 + 本地已有数）
#[tauri::command]
pub fn get_board_online_info(state: State<'_, AppState>) -> Result<Vec<BoardOnlineInfo>, String> {
    let manager = state.manager.lock().map_err(|e| e.to_string())?;
    let data_dir = manager.data_dir();
    Ok(yifang_data::get_board_online_info(data_dir))
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
            let manager = state.manager.lock().map_err(|e| e.to_string())?;
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
/// 同步完成后自动重试失败项，直到 0 失败或被取消。
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
        let manager = state.manager.lock().map_err(|e| e.to_string())?;
        manager.data_dir().to_path_buf()
    };
    let start = start_date.unwrap_or_else(|| "2023-01-01".into());

    // 获取股票列表
    let codes = yifang_data::fetch_board_stock_codes(&board)
        .map_err(|e| format!("获取板块 {} 股票列表失败: {}", board, e))?;

    if codes.is_empty() {
        return Err(format!("板块 {} 没有可同步的股票", board));
    }

    // 初始化进度
    {
        let mut progress = state.sync_progress.lock().map_err(|e| e.to_string())?;
        *progress = SyncProgress {
            running: true,
            board: board.clone(),
            levels: levels.clone(),
            total: codes.len(),
            completed: 0,
            success: 0,
            failures: Vec::new(),
            retrying: false,
            retry_round: 0,
            cancelled: false,
        };
    }

    // 在后台线程中执行同步
    let progress_state = state.sync_progress.clone();
    std::thread::spawn(move || {
        let board_label = board.clone();
        let tf_list = tfs;

        // ── 第一轮：全量同步 ──
        for (i, symbol) in codes.iter().enumerate() {
            // 检查是否被取消
            {
                let p = progress_state.lock().unwrap();
                if p.cancelled {
                    let mut p = progress_state.lock().unwrap();
                    p.running = false;
                    return;
                }
            }

            let result = yifang_data::sync_stock(&data_dir, symbol, &tf_list, &start, force);

            // 更新进度
            {
                let mut p = progress_state.lock().unwrap();
                p.completed = i + 1;
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
        }

        // ── 自动重试失败项 ──
        let max_retry_rounds = 5u32;
        for round in 1..=max_retry_rounds {
            let failed_symbols: Vec<String> = {
                let p = progress_state.lock().unwrap();
                p.failures.iter().map(|(sym, _lv, _msg): &(String, String, String)| sym.clone()).collect()
            };
            // 去重
            let mut unique: Vec<String> = failed_symbols.into_iter().collect::<std::collections::HashSet<_>>().into_iter().collect();
            unique.sort();

            if unique.is_empty() {
                break;
            }

            {
                let mut p = progress_state.lock().unwrap();
                if p.cancelled {
                    p.running = false;
                    return;
                }
                p.retrying = true;
                p.retry_round = round as usize;
                // 重置进度：total = 失败数, completed = 0
                p.total = unique.len();
                p.completed = 0;
                p.success = 0;
                p.failures.clear();
            }

            eprintln!("[后台同步] 第 {} 轮重试: {} 只股票", round, unique.len());

            for (i, symbol) in unique.iter().enumerate() {
                {
                    let p = progress_state.lock().unwrap();
                    if p.cancelled {
                        let mut p = progress_state.lock().unwrap();
                        p.running = false;
                        return;
                    }
                }

                let result = yifang_data::sync_stock(&data_dir, symbol, &tf_list, &start, true);

                {
                    let mut p = progress_state.lock().unwrap();
                    p.completed = i + 1;
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

                // 重试间隔长一些，避免被封
                std::thread::sleep(std::time::Duration::from_millis(200));
            }
        }

        {
            let mut p = progress_state.lock().unwrap();
            p.retrying = false;
            p.running = false;
        }

        eprintln!("[后台同步] 板块 {} 同步完成", board_label);
    });

    Ok(())
}

/// 获取后台同步的当前状态
#[tauri::command]
pub fn get_sync_status(state: State<'_, AppState>) -> Result<SyncProgress, String> {
    let progress = state.sync_progress.lock().map_err(|e| e.to_string())?;
    Ok(progress.clone())
}

/// 在系统文件管理器中打开数据存储目录
#[tauri::command]
pub async fn open_data_dir(state: State<'_, AppState>) -> Result<(), String> {
    let dir = {
        let manager = state.manager.lock().map_err(|e| e.to_string())?;
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
    Ok(())
}
