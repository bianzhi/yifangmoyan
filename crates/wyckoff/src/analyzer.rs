//! 威科夫分析器 — 整合所有威科夫分析步骤
//!
//! 完整流程：
//! 1. 努力与结果分析 → 量价关系
//! 2. 事件识别 → SC/AR/ST/Spring/SOS/LPS/JOC/PSY/BC/UTAD/SOW/LPSY
//! 3. 交易区间识别 → 由事件驱动的结构化TR
//! 4. 阶段识别 → 吸筹5阶段 + 派发5阶段
//! 5. 供需线绘制 → 供给线/需求线/冰线
//! 6. 标注整合 → WyckoffResult

use yifang_data::{KLine, WyckoffResult};
use crate::annotation::generate_wyckoff_result;

/// 威科夫分析器
pub struct WyckoffAnalyzer;

impl WyckoffAnalyzer {
    /// 对 K 线序列进行完整威科夫分析
    pub fn analyze(klines: &[KLine]) -> WyckoffResult {
        if klines.len() < 20 {
            return WyckoffResult::default();
        }
        generate_wyckoff_result(klines)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use yifang_data::TimeFrame;

    fn make_kline(open: f64, close: f64, high: f64, low: f64, vol: f64, idx: usize) -> KLine {
        KLine {
            symbol: "test".to_string(),
            timeframe: TimeFrame::D,
            dt: format!("d{}", idx),
            id: idx as u64,
            open, close, high, low, vol,
            amount: vol * (open + close) / 2.0,
        }
    }

    #[test]
    fn test_wyckoff_analyzer_accumulation() {
        // 构造吸筹场景：下跌 → SC → AR → ST → Spring → SOS → LPS
        let klines = vec![
            // 下跌趋势 (Phase A 前)
            make_kline(25.0, 24.0, 25.0, 23.0, 400.0, 0),
            make_kline(24.0, 22.5, 24.5, 22.0, 450.0, 1),
            make_kline(22.5, 21.0, 23.0, 20.5, 480.0, 2),
            make_kline(21.0, 19.5, 21.5, 19.0, 520.0, 3),
            make_kline(19.5, 18.0, 20.0, 17.5, 550.0, 4),
            // PS + SC
            make_kline(18.0, 16.0, 18.5, 15.0, 800.0, 5),  // PS
            make_kline(16.0, 12.0, 17.0, 10.0, 2000.0, 6),  // SC: 宽幅巨量收回
            make_kline(12.0, 14.0, 14.5, 11.5, 1500.0, 7),  // AR
            make_kline(14.0, 13.0, 14.5, 12.5, 600.0, 8),   // 回调
            make_kline(13.0, 12.5, 13.5, 10.5, 500.0, 9),   // ST
            // Phase B 横盘
            make_kline(12.5, 14.0, 14.5, 12.0, 400.0, 10),
            make_kline(14.0, 13.0, 14.0, 12.5, 350.0, 11),
            make_kline(13.0, 12.0, 13.5, 11.5, 380.0, 12),
            make_kline(12.0, 13.5, 14.0, 11.8, 420.0, 13),
            // Phase C: Spring
            make_kline(13.5, 12.0, 13.5, 9.5, 450.0, 14),   // Spring: 跌破SC低点后收回
            // Phase D: SOS + LPS
            make_kline(12.0, 15.0, 15.5, 11.5, 1200.0, 15), // SOS
            make_kline(15.0, 14.0, 15.5, 13.5, 500.0, 16),  // LPS
            make_kline(14.0, 16.0, 16.5, 13.5, 1000.0, 17), // JOC
            make_kline(16.0, 17.0, 17.5, 15.5, 800.0, 18),
            make_kline(17.0, 18.0, 18.5, 16.5, 900.0, 19),
        ];

        let result = WyckoffAnalyzer::analyze(&klines);

        // 应检测到事件
        assert!(!result.events.is_empty(), "应检测到威科夫事件");

        // 应有 SC
        let sc_events: Vec<_> = result.events.iter().filter(|e| e.event_type == "SC").collect();
        assert!(!sc_events.is_empty(), "应检测到SC");

        // 应有阶段标注
        assert!(!result.phase_labels.is_empty(), "应有阶段标注");

        // 应有努力与结果分析
        assert!(!result.effort_results.is_empty(), "应有努力与结果分析");
    }

    #[test]
    fn test_wyckoff_analyzer_small_data() {
        let klines: Vec<KLine> = (0..10)
            .map(|i| make_kline(10.0, 10.5, 11.0, 10.0, 500.0, i))
            .collect();

        let result = WyckoffAnalyzer::analyze(&klines);
        assert!(result.events.is_empty(), "数据不足不应有事件");
    }

    #[test]
    fn test_wyckoff_analyzer_uptrend() {
        // 纯上涨趋势应检测到 PSY/BC 等派发事件
        let klines: Vec<KLine> = (0..25)
            .map(|i| make_kline(
                10.0 + (i as f64) * 0.5,
                10.5 + (i as f64) * 0.5,
                11.0 + (i as f64) * 0.5,
                10.0 + (i as f64) * 0.5,
                1000.0,
                i,
            ))
            .collect();

        let result = WyckoffAnalyzer::analyze(&klines);
        // 上涨趋势中不应有 SC
        let sc_events: Vec<_> = result.events.iter().filter(|e| e.event_type == "SC").collect();
        assert!(sc_events.is_empty(), "纯上涨不应有SC");
    }
}
