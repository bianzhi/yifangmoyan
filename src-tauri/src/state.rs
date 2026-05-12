//! 应用状态管理

use std::sync::Mutex;
use yifang_data::KLineManager;

pub struct AppState {
    pub manager: Mutex<KLineManager>,
}

impl AppState {
    pub fn new() -> Self {
        // 可通过环境变量覆盖数据目录
        let data_dir = std::env::var("YIFANG_DATA_DIR").ok();
        Self {
            manager: Mutex::new(KLineManager::new(data_dir.as_deref())),
        }
    }
}
