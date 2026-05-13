//! 威科夫标注生成
//!
//! 整合所有分析结果，生成完整的 WyckoffResult。
//!
//! 包含：
//! - 趋势线（兼容旧接口）
//! - 供需线
//! - 事件标注
//! - 交易区间
//! - 阶段标注
//! - 努力与结果分析

use yifang_data::{
    KLine, WyckoffEvent, TrendLine, WyckoffResult,
};

use crate::effort::analyze_effort_result;
use crate::phase::identify_phases;
use crate::pattern::detect_events;
use crate::trading_range::detect_trading_ranges_structured;
use crate::supply_demand::draw_supply_demand_lines;

/// 从事件和交易区间生成趋势线（兼容）
fn generate_trend_lines(klines: &[KLine], _events: &[WyckoffEvent]) -> Vec<TrendLine> {
    let mut lines = Vec::new();
    if klines.len() < 20 {
        return lines;
    }

    // 支撑趋势线：连接最近的两个显著低点
    let lows = find_significant_points(klines, true);
    if lows.len() >= 2 {
        let (i1, p1) = lows[lows.len() - 2];
        let (i2, p2) = lows[lows.len() - 1];
        lines.push(TrendLine {
            line_type: "support".to_string(),
            start_index: i1 as u64,
            end_index: i2 as u64,
            start_price: p1,
            end_price: p2,
        });
    }

    // 阻力趋势线
    let highs = find_significant_points(klines, false);
    if highs.len() >= 2 {
        let (i1, p1) = highs[highs.len() - 2];
        let (i2, p2) = highs[highs.len() - 1];
        lines.push(TrendLine {
            line_type: "resistance".to_string(),
            start_index: i1 as u64,
            end_index: i2 as u64,
            start_price: p1,
            end_price: p2,
        });
    }

    lines
}

/// 生成完整威科夫分析结果
pub fn generate_wyckoff_result(klines: &[KLine]) -> WyckoffResult {
    // 1. 努力与结果分析
    let effort_results = analyze_effort_result(klines);

    // 2. 事件识别
    let events = detect_events(klines);

    // 3. 交易区间识别
    let trading_ranges = detect_trading_ranges_structured(klines, &events);

    // 4. 阶段识别
    let phase_labels = identify_phases(klines);

    // 5. 供需线绘制
    let supply_demand_lines = draw_supply_demand_lines(klines, &trading_ranges);

    // 6. 趋势线（兼容旧接口）
    let trend_lines = generate_trend_lines(klines, &events);

    WyckoffResult {
        phase_labels,
        events,
        trading_ranges,
        trend_lines,
        supply_demand_lines,
        effort_results,
    }
}

/// 找出显著的高点或低点
fn find_significant_points(klines: &[KLine], find_lows: bool) -> Vec<(usize, f64)> {
    let window = 5;
    let mut points = Vec::new();

    for i in window..klines.len().saturating_sub(window) {
        let slice = &klines[i - window..=i + window];
        let val = if find_lows {
            slice.iter().map(|k| k.low).fold(f64::MAX, f64::min)
        } else {
            slice.iter().map(|k| k.high).fold(f64::MIN, f64::max)
        };

        let kline_val = if find_lows { klines[i].low } else { klines[i].high };
        if (val - kline_val).abs() < 0.01 {
            points.push((i, kline_val));
        }
    }

    points
}
