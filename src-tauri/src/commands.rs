//! Tauri IPC 命令

use tauri::State;

use crate::state::AppState;
use yifang_data::{ChartData, DataSource, KLine, StockInfo, TimeFrame};
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
