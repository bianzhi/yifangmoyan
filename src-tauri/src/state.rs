//! 应用状态管理

use std::sync::{Arc, Mutex};
use yifang_data::KLineManager;

/// 后台同步的状态
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SyncProgress {
    /// 是否正在同步
    pub running: bool,
    /// 当前同步的板块 id
    pub board: String,
    /// 同步的级别
    pub levels: Vec<String>,
    /// 总股票数
    pub total: usize,
    /// 已完成数
    pub completed: usize,
    /// 成功数
    pub success: usize,
    /// 失败列表: [(symbol, level, msg)]
    pub failures: Vec<(String, String, String)>,
    /// 当前是否在自动重试阶段
    pub retrying: bool,
    /// 重试轮次
    pub retry_round: usize,
    /// 是否已被用户取消
    pub cancelled: bool,
}

impl Default for SyncProgress {
    fn default() -> Self {
        Self {
            running: false,
            board: String::new(),
            levels: Vec::new(),
            total: 0,
            completed: 0,
            success: 0,
            failures: Vec::new(),
            retrying: false,
            retry_round: 0,
            cancelled: false,
        }
    }
}

pub struct AppState {
    pub manager: Mutex<KLineManager>,
    /// 后台同步进度（用于前端轮询 + 后台线程共享）
    pub sync_progress: Arc<Mutex<SyncProgress>>,
}

impl AppState {
    pub fn new() -> Self {
        // 可通过环境变量覆盖数据目录
        let data_dir = std::env::var("YIFANG_DATA_DIR").ok();
        Self {
            manager: Mutex::new(KLineManager::new(data_dir.as_deref())),
            sync_progress: Arc::new(Mutex::new(SyncProgress::default())),
        }
    }
}
