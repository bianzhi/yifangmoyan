//! 威科夫分析器 — 整合所有威科夫分析步骤

use yifang_data::{KLine, WyckoffResult};

use crate::pattern::{detect_events, detect_trading_ranges};
use crate::annotation::generate_annotations;

/// 威科夫分析器
pub struct WyckoffAnalyzer;

impl WyckoffAnalyzer {
    /// 对 K 线序列进行完整威科夫分析
    pub fn analyze(klines: &[KLine]) -> WyckoffResult {
        if klines.len() < 20 {
            return WyckoffResult::default();
        }

        // 1. 识别威科夫事件
        let events = detect_events(klines);

        // 2. 识别交易区间
        let trading_ranges = detect_trading_ranges(klines);

        // 3. 生成标注
        generate_annotations(klines, events, trading_ranges)
    }
}
