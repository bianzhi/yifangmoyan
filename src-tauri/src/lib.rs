//! 墨岩K线分析系统 — Tauri 桌面应用入口

mod commands;
mod state;

use state::AppState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(AppState::new())
        .invoke_handler(tauri::generate_handler![
            commands::get_klines,
            commands::get_chart_data,
            commands::search_stocks,
            commands::get_stock_info,
            commands::get_sub_level_data,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
