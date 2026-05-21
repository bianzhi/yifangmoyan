//! 应用状态管理

use std::sync::{Arc, Mutex, RwLock};
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
    /// 当前正在同步的股票代码（多线程时为最后一批）
    pub current_symbols: Vec<String>,
    /// 是否正在获取股票列表（预热阶段）
    pub preparing: bool,
    /// 获取列表失败的错误信息
    pub prepare_error: String,
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
            current_symbols: Vec::new(),
            preparing: false,
            prepare_error: String::new(),
        }
    }
}

pub struct AppState {
    pub manager: RwLock<KLineManager>,
    /// 后台同步进度（用于前端轮询 + 后台线程共享）
    pub sync_progress: Arc<Mutex<SyncProgress>>,
}

impl AppState {
    pub fn new() -> Self {
        // 可通过环境变量覆盖数据目录
        let data_dir = std::env::var("YIFANG_DATA_DIR").ok();
        Self {
            manager: RwLock::new(KLineManager::new(data_dir.as_deref())),
            sync_progress: Arc::new(Mutex::new(SyncProgress::default())),
        }
    }
}
