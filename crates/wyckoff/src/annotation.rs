//! 威科夫标注生成 — 趋势线、冰线、支撑阻力

use yifang_data::{KLine, TrendLine, WyckoffEvent, TradingRange, WyckoffResult};

/// 从事件和交易区间生成威科夫分析结果
pub fn generate_annotations(
    klines: &[KLine],
    events: Vec<WyckoffEvent>,
    trading_ranges: Vec<TradingRange>,
) -> WyckoffResult {
    let trend_lines = generate_trend_lines(klines, &events);

    WyckoffResult {
        trend_lines,
        events,
        trading_ranges,
    }
}

/// 从 K 线数据生成趋势线
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

    // 阻力趋势线：连接最近的两个显著高点
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

/// 找出显著的高点或低点
fn find_significant_points(klines: &[KLine], find_lows: bool) -> Vec<(usize, f64)> {
    let window = 5;
    let mut points = Vec::new();

    for i in window..klines.len() - window {
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
