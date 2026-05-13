//! Tauri IPC 命令

use std::path::PathBuf;
use tauri::State;

use crate::state::AppState;
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

    Ok(ChartData {
        symbol,
        name,
        timeframe: tf,
        klines,
        macd,
        czsc,
        wyckoff,
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
    let manager = state.manager.lock().map_err(|e| e.to_string())?;
    let data_dir = manager.data_dir();

    let tf_list: Vec<TimeFrame> = levels
        .iter()
        .filter_map(|s| parse_timeframe(s))
        .collect();

    if tf_list.is_empty() {
        return Err("未指定有效的 K 线级别".into());
    }

    let start = start_date.unwrap_or_else(|| "2023-01-01".into());

    Ok(yifang_data::sync_stock(data_dir, &symbol, &tf_list, &start, force))
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
    let manager = state.manager.lock().map_err(|e| e.to_string())?;
    let data_dir = manager.data_dir();

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
        .map(|sym| yifang_data::sync_stock(data_dir, sym, &tf_list, &start, force))
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
    let manager = state.manager.lock().map_err(|e| e.to_string())?;
    let data_dir = manager.data_dir();
    let start = start_date.unwrap_or_default();
    Ok(yifang_data::sync_board(data_dir, &board, &tfs, &start, force))
}
