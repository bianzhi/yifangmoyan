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
    let manager = state.manager.read().map_err(|e| e.to_string())?;
    manager.get_klines(&symbol, tf, None, None).map_err(|e| e.to_string())
}

/// 获取完整图表数据（K线 + MACD + 缠论 + 威科夫）
/// 关键优化：只在读取数据时持有读锁，分析计算时释放锁，
/// 这样同步不会阻塞图表切换
#[tauri::command]
pub fn get_chart_data(
    state: State<'_, AppState>,
    symbol: String,
    timeframe: String,
    enable_czsc: bool,
    enable_wyckoff: bool,
) -> Result<ChartData, String> {
    let tf = parse_timeframe(&timeframe).ok_or_else(|| format!("无效的时间周期: {}", timeframe))?;

    // ── 阶段1：只持有读锁，获取数据后立即释放 ──
    let (klines, name) = {
        let manager = state.manager.read().map_err(|e| e.to_string())?;
        let klines = manager.get_klines(&symbol, tf, None, None).map_err(|e| e.to_string())?;
        let name = manager.get_stock_info(&symbol).map(|i| i.name).unwrap_or_default();
        (klines, name)
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

    // 阶段1：只持有读锁
    let (all_klines, name) = {
        let manager = state.manager.read().map_err(|e| e.to_string())?;
        let klines = manager.get_klines(&symbol, sub_tf, Some(&start_dt), Some(&end_dt))
            .map_err(|e| e.to_string())?;
        let name = manager.get_stock_info(&symbol).map(|i| i.name).unwrap_or_default();
        (klines, name)
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
        run_sync_parallel(&progress_state, &data_dir, &codes, &tfs, &start, force, 4);
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

/// 启动时自动同步 — 对所有板块做增量同步，失败自动重试
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
            run_sync_parallel(&progress_state, &data_dir_for_bg, &initial, &tf_list, &start, force, 4);
            eprintln!("[启动同步] 引导数据同步完成");
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
        run_sync_parallel(&progress_state, &data_dir_for_bg, &online_codes, &tf_list, &start, force, 4);
        eprintln!("[启动同步] 增量同步完成");
    });

    Ok(())
}

/// 内部函数：并行执行同步并自动重试失败项
/// concurrency: 并发线程数
fn run_sync_parallel(
    progress_state: &std::sync::Arc<std::sync::Mutex<SyncProgress>>,
    data_dir: &std::path::Path,
    codes: &[String],
    tf_list: &[TimeFrame],
    start: &str,
    force: bool,
    concurrency: usize,
) {
    // ── 非 force 模式下，先批量过滤已最新的股票 ──
    let (skip_count, need_sync): (usize, Vec<String>) = if !force {
        let (skipped, needed): (Vec<_>, Vec<_>) = codes.iter().partition(|code| {
            yifang_data::is_stock_up_to_date(data_dir, code, tf_list)
        });
        let skip_count = skipped.len();
        // 为跳过的股票批量更新进度
        if skip_count > 0 {
            let mut p = progress_state.lock().unwrap();
            p.completed += skip_count;
            p.success += skip_count;
            p.skipped_count = skip_count;
        }
        (skip_count, needed.into_iter().cloned().collect())
    } else {
        (0, codes.to_vec())
    };

    if skip_count > 0 {
        eprintln!("[批量跳过] {} 只股票数据已是最新，跳过网络请求", skip_count);
    }

    if need_sync.is_empty() {
        // 全部跳过，直接完成
        let mut p = progress_state.lock().unwrap();
        p.running = false;
        p.all_skipped = true;
        p.skipped_count = skip_count;
        return;
    }

    let codes_vec = need_sync;
    let total = codes_vec.len();
    let idx_lock = std::sync::Arc::new(std::sync::Mutex::new(0usize));
    let mut handles = Vec::new();

    // ── 第一轮：全量并行同步，每只股票后加小间隔防限流 ──
    for _ in 0..concurrency {
        let idx_lock = idx_lock.clone();
        let codes_ref = codes_vec.clone();
        let progress_state = progress_state.clone();
        let data_dir = data_dir.to_path_buf();
        let tf_list = tf_list.to_vec();
        let start = start.to_string();

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
                    *idx += 1;
                    current
                };

                if i >= total {
                    return;
                }

                let symbol = &codes_ref[i];

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

                // 只对实际请求了网络的股票（非 skip）加 50ms 间隔防限流
                let all_skipped = result.levels.iter().all(|lv| lv.status == "skip");
                if !all_skipped {
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

    // ── 自动重试失败项（最多2轮，而非5轮，避免无限等待） ──
    let max_retry_rounds = 2u32;
    for round in 1..=max_retry_rounds {
        let failed_symbols: Vec<String> = {
            let p = progress_state.lock().unwrap();
            p.failures.iter().map(|(sym, _lv, _msg)| sym.clone()).collect()
        };
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
            p.total = unique.len();
            p.completed = 0;
            p.success = 0;
            p.failures.clear();
        }

        eprintln!("[自动重试] 第 {} 轮：{} 只股票需重试", round, unique.len());

        // 并行重试（2线程），每只之间间隔 300ms
        let retry_concurrency = 2usize.min(unique.len());
        let retry_total = unique.len();
        let retry_idx = std::sync::Arc::new(std::sync::Mutex::new(0usize));
        let mut retry_handles = Vec::new();

        for _ in 0..retry_concurrency {
            let retry_idx = retry_idx.clone();
            let unique_ref = unique.clone();
            let progress_state = progress_state.clone();
            let data_dir = data_dir.to_path_buf();
            let tf_list = tf_list.to_vec();
            let start = start.to_string();

            let handle = std::thread::spawn(move || {
                loop {
                    {
                        let p = progress_state.lock().unwrap();
                        if p.cancelled {
                            return;
                        }
                    }

                    let i = {
                        let mut idx = retry_idx.lock().unwrap();
                        let current = *idx;
                        *idx += 1;
                        current
                    };

                    if i >= retry_total {
                        return;
                    }

                    let symbol = &unique_ref[i];

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

                    // 重试间隔 300ms，防止被限流
                    std::thread::sleep(std::time::Duration::from_millis(300));
                }
            });
            retry_handles.push(handle);
        }

        for h in retry_handles {
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
    }

    {
        let mut p = progress_state.lock().unwrap();
        p.retrying = false;
        p.running = false;
    }
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
