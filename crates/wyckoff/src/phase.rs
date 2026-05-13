//! 威科夫阶段识别
//!
//! **严格对齐威科夫原著的阶段定义**
//!
//! 威科夫市场周期由四个主要阶段组成：
//! 1. 吸筹 (Accumulation): 大资金逐步买入
//! 2. 拉升 (Markup): 需求主导，价格上涨
//! 3. 派发 (Distribution): 大资金逐步卖出
//! 4. 下跌 (Markdown): 供给主导，价格下跌
//!
//! **吸筹五阶段**:
//! - Phase A: 止跌 — PS → SC → AR → ST
//! - Phase B: 蓄力 — 多次围绕 TR 上下波动，量能递减
//! - Phase C: 测试 — Spring/Shakeout，主力最后一次测试供给
//! - Phase D: 启动 — SOS → LPS → JOC
//! - Phase E: 离开 — 突破交易区间上沿
//!
//! **派发五阶段**:
//! - Phase A: 停涨 — PSY → BC → AR → ST
//! - Phase B: 派发 — 多次围绕 TR 上下波动，量能递减
//! - Phase C: 出货 — UTAD，主力最后一次测试需求
//! - Phase D: 破位 — SOW → LPSY → ICE
//! - Phase E: 离开 — 跌破交易区间下沿
//!
//! **识别方法**：
//! 1. 先识别局部极值点（swing highs/lows）
//! 2. 根据极值点的走势结构判断大阶段
//! 3. 在交易区间内，根据事件序列判断子阶段
//! 4. 量价关系确认

use yifang_data::{KLine, WyckoffPhase, PhaseLabel};
use crate::pattern::{detect_events, EventContext};
use crate::trading_range::detect_trading_ranges_structured;

/// 识别 K 线序列的威科夫阶段（结构化方法）
///
/// 基于价格结构 + 成交量特征 + 事件序列进行阶段判断。
pub fn identify_phases(klines: &[KLine]) -> Vec<PhaseLabel> {
    if klines.len() < 20 {
        return Vec::new();
    }

    let swing_points = find_swing_points(klines, 5);
    let events = detect_events(klines);
    let trading_ranges = detect_trading_ranges_structured(klines, &events);

    let mut labels = Vec::new();

    // 如果没有交易区间，使用简化的趋势判断
    if trading_ranges.is_empty() {
        classify_by_trend(klines, &swing_points, &mut labels);
    } else {
        // 根据交易区间和事件进行结构化阶段识别
        classify_by_structure(klines, &swing_points, &events, &trading_ranges, &mut labels);
    }

    labels
}

/// 找出局部极值点（Swing Points）
///
/// 使用 N 根 K 线窗口寻找局部高点和低点。
/// - 局部高点：比左右各 N 根 K 线的高点都高
/// - 局部低点：比左右各 N 根 K 线的低点都低
pub fn find_swing_points(klines: &[KLine], n: usize) -> Vec<SwingPoint> {
    let mut points = Vec::new();

    if klines.len() < 2 * n + 1 {
        return points;
    }

    for i in n..klines.len() - n {
        let k = &klines[i];

        // 检查局部高点
        let is_high = klines[i - n..=i + n].iter().all(|x| x.high <= k.high);
        if is_high {
            points.push(SwingPoint {
                index: i,
                price: k.high,
                is_high: true,
            });
        }

        // 检查局部低点
        let is_low = klines[i - n..=i + n].iter().all(|x| x.low >= k.low);
        if is_low {
            points.push(SwingPoint {
                index: i,
                price: k.low,
                is_high: false,
            });
        }
    }

    points
}

/// 极值点结构
#[derive(Debug, Clone)]
pub struct SwingPoint {
    pub index: usize,
    pub price: f64,
    pub is_high: bool,
}

/// 无交易区间时的简化趋势判断
fn classify_by_trend(klines: &[KLine], _swing_points: &[SwingPoint], labels: &mut Vec<PhaseLabel>) {
    let avg_vol: f64 = klines.iter().map(|k| k.vol).sum::<f64>() / klines.len() as f64;

    for (i, k) in klines.iter().enumerate() {
        let phase = if i < 10 {
            WyckoffPhase::Markdown
        } else {
            let window_start = if i >= 20 { i - 20 } else { 0 };
            let window = &klines[window_start..=i];
            let first_close = window.first().map(|k| k.close).unwrap_or(k.close);
            let last_close = k.close;
            let price_trend = last_close - first_close;

            let vol_high = k.vol > avg_vol * 1.5;

            if price_trend > 0.0 {
                if vol_high {
                    WyckoffPhase::Markup
                } else {
                    // 上涨但量缩 → 可能进入派发
                    WyckoffPhase::Distribution
                }
            } else if price_trend < 0.0 {
                if vol_high {
                    WyckoffPhase::Markdown
                } else {
                    // 下跌但量缩 → 可能进入吸筹
                    WyckoffPhase::Accumulation
                }
            } else {
                WyckoffPhase::Accumulation
            }
        };

        labels.push(PhaseLabel {
            index: i as u64,
            dt: k.dt.clone(),
            phase,
            sub_phase: String::new(),
        });
    }
}

/// 结构化阶段分类
///
/// 当检测到交易区间时，根据事件序列进行精细阶段识别。
fn classify_by_structure(
    klines: &[KLine],
    _swing_points: &[SwingPoint],
    events: &[yifang_data::WyckoffEvent],
    trading_ranges: &[yifang_data::TradingRange],
    labels: &mut Vec<PhaseLabel>,
) {
    // 判断交易区间是吸筹还是派发：
    // - 区间之前是下跌 → 吸筹
    // - 区间之前是上涨 → 派发
    for (i, k) in klines.iter().enumerate() {
        let (phase, sub_phase) = determine_phase_at(klines, events, trading_ranges, i);
        labels.push(PhaseLabel {
            index: i as u64,
            dt: k.dt.clone(),
            phase,
            sub_phase,
        });
    }
}

/// 确定某根 K 线处的阶段
fn determine_phase_at(
    klines: &[KLine],
    events: &[yifang_data::WyckoffEvent],
    trading_ranges: &[yifang_data::TradingRange],
    idx: usize,
) -> (WyckoffPhase, String) {
    // 找包含当前索引的交易区间
    let in_range = trading_ranges.iter().find(|tr| {
        idx >= tr.start_index as usize && idx <= tr.end_index as usize
    });

    if let Some(tr) = in_range {
        // 在交易区间内 — 判断是吸筹还是派发
        let prior_trend = calc_prior_trend(klines, tr.start_index as usize);
        let ctx = if prior_trend < 0.0 {
            EventContext::Accumulation
        } else {
            EventContext::Distribution
        };

        // 在区间内找到之前的事件，判断子阶段
        let events_before: Vec<_> = events.iter()
            .filter(|e| e.index <= idx as u64 && e.index >= tr.start_index)
            .collect();

        let sub_phase = classify_sub_phase(&events_before, ctx, idx, tr);

        let phase = match ctx {
            EventContext::Accumulation => WyckoffPhase::Accumulation,
            EventContext::Distribution => WyckoffPhase::Distribution,
        };

        (phase, sub_phase)
    } else {
        // 不在交易区间内 — 根据趋势判断 Markup / Markdown
        let avg_vol: f64 = klines.iter().map(|k| k.vol).sum::<f64>() / klines.len() as f64;
        let _vol_high = klines[idx].vol > avg_vol * 1.2;
        
        // 找最近的交易区间
        let nearest_before = trading_ranges.iter()
            .filter(|tr| tr.end_index < idx as u64)
            .max_by_key(|tr| tr.end_index);

        if let Some(prev_tr) = nearest_before {
            // 交易区间之后的走势
            let prior_ctx = if calc_prior_trend(klines, prev_tr.start_index as usize) < 0.0 {
                EventContext::Accumulation
            } else {
                EventContext::Distribution
            };

            match prior_ctx {
                EventContext::Accumulation => {
                    if klines[idx].close > prev_tr.upper {
                        (WyckoffPhase::Markup, "E".to_string())
                    } else {
                        (WyckoffPhase::Accumulation, "D".to_string())
                    }
                }
                EventContext::Distribution => {
                    if klines[idx].close < prev_tr.lower {
                        (WyckoffPhase::Markdown, "E".to_string())
                    } else {
                        (WyckoffPhase::Distribution, "D".to_string())
                    }
                }
            }
        } else {
            // 没有前置交易区间，使用简单趋势
            if idx >= 20 {
                let trend = klines[idx].close - klines[idx - 20].close;
                if trend > 0.0 {
                    (WyckoffPhase::Markup, String::new())
                } else {
                    (WyckoffPhase::Markdown, String::new())
                }
            } else {
                (WyckoffPhase::Markdown, String::new())
            }
        }
    }
}

/// 计算交易区间前的趋势方向
///
/// 返回 < 0 表示下跌，> 0 表示上涨
fn calc_prior_trend(klines: &[KLine], start_idx: usize) -> f64 {
    let lookback = 20.min(start_idx);
    if lookback == 0 {
        return 0.0;
    }
    let start = start_idx - lookback;
    klines[start_idx].close - klines[start].close
}

/// 根据事件序列判断子阶段
fn classify_sub_phase(
    events: &[&yifang_data::WyckoffEvent],
    ctx: EventContext,
    _idx: usize,
    _tr: &yifang_data::TradingRange,
) -> String {
    let has_sc_or_bc = events.iter().any(|e| e.event_type == "SC" || e.event_type == "BC");
    let has_ar = events.iter().any(|e| e.event_type == "AR");
    let has_st = events.iter().any(|e| e.event_type == "ST");
    let has_spring_or_utad = events.iter().any(|e| e.event_type == "Spring" || e.event_type == "UTAD");
    let has_sos_or_sow = events.iter().any(|e| e.event_type == "SOS" || e.event_type == "SOW");
    let has_lps_or_lpsy = events.iter().any(|e| e.event_type == "LPS" || e.event_type == "LPSY");

    match ctx {
        EventContext::Accumulation => {
            if has_lps_or_lpsy { "D".to_string() }
            else if has_sos_or_sow { "D".to_string() }
            else if has_spring_or_utad { "C".to_string() }
            else if has_st { "B".to_string() }
            else if has_ar { "A".to_string() }
            else if has_sc_or_bc { "A".to_string() }
            else { "B".to_string() }
        }
        EventContext::Distribution => {
            if has_lps_or_lpsy { "D".to_string() }
            else if has_sos_or_sow { "D".to_string() }
            else if has_spring_or_utad { "C".to_string() }
            else if has_st { "B".to_string() }
            else if has_ar { "A".to_string() }
            else if has_sc_or_bc { "A".to_string() }
            else { "B".to_string() }
        }
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
    fn test_find_swing_points() {
        // 构造有明确极值的序列
        let klines = vec![
            make_kline(10.0, 11.0, 11.0, 10.0, 1000.0, 0),
            make_kline(11.0, 12.0, 12.0, 11.0, 1000.0, 1),
            make_kline(12.0, 13.0, 15.0, 12.0, 1000.0, 2), // 高点
            make_kline(13.0, 12.0, 13.0, 11.0, 1000.0, 3),
            make_kline(12.0, 8.0, 12.0, 8.0, 1000.0, 4),  // 低点
            make_kline(8.0, 10.0, 10.0, 8.0, 1000.0, 5),
            make_kline(10.0, 11.0, 11.0, 10.0, 1000.0, 6),
            make_kline(11.0, 10.0, 11.0, 10.0, 1000.0, 7),
            make_kline(10.0, 9.0, 10.0, 9.0, 1000.0, 8),
            make_kline(9.0, 10.0, 10.0, 9.0, 1000.0, 9),
            make_kline(10.0, 11.0, 11.0, 10.0, 1000.0, 10),
        ];
        let points = find_swing_points(&klines, 2);
        assert!(!points.is_empty(), "应找到极值点");
    }

    #[test]
    fn test_identify_phases_basic() {
        // 构造简化下跌序列
        let klines: Vec<KLine> = (0..30)
            .map(|i| make_kline(
                20.0 - (i as f64) * 0.3,
                20.0 - (i as f64) * 0.3 - 0.2,
                20.0 - (i as f64) * 0.3 + 0.5,
                20.0 - (i as f64) * 0.3 - 0.5,
                1000.0,
                i,
            ))
            .collect();

        let labels = identify_phases(&klines);
        assert!(labels.len() > 0, "应返回阶段标注");
    }
}
