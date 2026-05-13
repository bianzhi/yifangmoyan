//! 墨岩K线分析系统 — Tauri 桌面应用入口

mod commands;
mod state;

use state::AppState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState::new())
        .invoke_handler(tauri::generate_handler![
            commands::get_klines,
            commands::get_chart_data,
            commands::search_stocks,
            commands::get_stock_info,
            commands::get_sub_level_data,
            commands::get_data_status,
            commands::sync_stock,
            commands::sync_stocks_batch,
            commands::sync_board,
            commands::start_sync_board,
            commands::get_sync_status,
            commands::cancel_sync,
            commands::get_all_stock_codes,
            commands::validate_stock,
            commands::validate_stock_level,
            commands::cross_validate_stock,
            commands::get_data_dir,
            commands::set_data_dir,
            commands::move_data_dir,
            commands::get_board_stats,
            commands::get_board_online_info,
            commands::get_stock_codes_by_board,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
