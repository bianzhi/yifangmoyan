//! 供需线绘制 (Supply / Demand Lines)
//!
//! 威科夫理论中的供需线是价格结构的边界：
//!
//! **供给线 (Supply Line)**:
//! - 连接反弹高点（swing highs）
//! - 等同传统技术分析的下降趋势线
//! - 价格触及供给线 → 遇到卖出压力
//! - 有效突破供给线 → 需求克服供给（看涨信号）
//!
//! **需求线 (Demand Line)**:
//! - 连接回调低点（swing lows）
//! - 等同传统技术分析的上升趋势线
//! - 价格触及需求线 → 遇到买入支撑
//! - 有效跌破需求线 → 供给压倒需求（看跌信号）
//!
//! **有效突破判断**：
//! 1. 突破幅度 > 3%
//! 2. 突破时成交量放大
//! 3. 突破后不回落到线以下
//!
//! **通道确认**：
//! - 供给线和需求线应大致平行
//! - 形成价格通道，通道宽度反映波动率
//! - 通道变窄 → 可能突破
//! - 通道变宽 → 波动加大

use yifang_data::{KLine, SupplyDemandLine, TradingRange};
use crate::phase::find_swing_points;

/// 绘制供需线
///
/// 从 K 线极值点中绘制供给线和需求线。
/// 也从交易区间中推导冰线和区间边界线。
pub fn draw_supply_demand_lines(klines: &[KLine], trading_ranges: &[TradingRange]) -> Vec<SupplyDemandLine> {
    let mut lines = Vec::new();

    // 1. 从极值点绘制趋势线
    let swing_points = find_swing_points(klines, 5);
    let highs: Vec<_> = swing_points.iter().filter(|p| p.is_high).collect();
    let lows: Vec<_> = swing_points.iter().filter(|p| !p.is_high).collect();

    // 供给线：连接最近两个高点（高点下降 → 下降供给线）
    if highs.len() >= 2 {
        if let Some(line) = draw_line_from_points(&highs, "supply") {
            lines.push(line);
        }
    }

    // 需求线：连接最近两个低点（低点上升 → 上升需求线）
    if lows.len() >= 2 {
        if let Some(line) = draw_line_from_points(&lows, "demand") {
            lines.push(line);
        }
    }

    // 2. 从交易区间推导供需线
    for tr in trading_ranges {
        let range_width = tr.upper - tr.lower;
        let width_pct = if tr.lower > 0.0 { range_width / tr.lower * 100.0 } else { 0.0 };

        // 区间上沿 = 供给线水平段（由交易区间上沿界定）
        lines.push(SupplyDemandLine {
            line_type: "supply".to_string(),
            start_index: tr.start_index,
            end_index: tr.end_index,
            start_price: tr.upper,
            end_price: tr.upper,
            slope: 0.0,
            reason: format!(
                "供给线(区间上沿): 交易区间[#{:#?}~#{:#?}]上沿={:.2}, 由AR高点确立的供给边界",
                tr.start_index, tr.end_index, tr.upper
            ),
        });

        // 区间下沿 = 需求线水平段（由交易区间下沿界定）
        lines.push(SupplyDemandLine {
            line_type: "demand".to_string(),
            start_index: tr.start_index,
            end_index: tr.end_index,
            start_price: tr.lower,
            end_price: tr.lower,
            slope: 0.0,
            reason: format!(
                "需求线(区间下沿): 交易区间[#{:#?}~#{:#?}]下沿={:.2}, 由SC低点确立的需求边界",
                tr.start_index, tr.end_index, tr.lower
            ),
        });

        // 冰线（ICE Line）: 交易区间中位线，是吸筹转为拉升的关键分界
        // 威科夫原著：冰线由AR低点连线形成，跌破冰线意味着供给压倒需求
        lines.push(SupplyDemandLine {
            line_type: "ice_line".to_string(),
            start_index: tr.start_index,
            end_index: tr.end_index,
            start_price: tr.ice_line,
            end_price: tr.ice_line,
            slope: 0.0,
            reason: format!(
                "冰线(ICE): 交易区间中位线={:.2}, 区间宽={:.2}({:.1}%), 跌破冰线→供给压倒需求",
                tr.ice_line, range_width, width_pct
            ),
        });
    }

    lines
}

/// 从极值点绘制趋势线
///
/// 选择最能代表趋势的前两个极值点。
/// reason 说明：连接哪两个 swing point 构成的线，以及线的类型含义。
fn draw_line_from_points(points: &[&crate::phase::SwingPoint], line_type: &str) -> Option<SupplyDemandLine> {
    if points.len() < 2 {
        return None;
    }

    // 取最后两个点
    let n = points.len();
    let p1 = &points[n - 2];
    let p2 = &points[n - 1];

    let index_diff = p2.index as f64 - p1.index as f64;
    let slope = if index_diff > 0.0 {
        (p2.price - p1.price) / index_diff
    } else {
        0.0
    };

    let (type_cn, role) = match line_type {
        "supply" => ("供给线", "连接反弹高点，等同传统下降趋势线；触及→卖出压力，突破→需求克服供给(看涨)"),
        "demand" => ("需求线", "连接回调低点，等同传统上升趋势线；触及→买入支撑，跌破→供给压倒需求(看跌)"),
        _ => (line_type, ""),
    };

    let trend_desc = if slope > 0.001 {
        "上升"
    } else if slope < -0.001 {
        "下降"
    } else {
        "水平"
    };

    Some(SupplyDemandLine {
        line_type: line_type.to_string(),
        start_index: p1.index as u64,
        end_index: p2.index as u64,
        start_price: p1.price,
        end_price: p2.price,
        slope,
        reason: format!(
            "{}(趋势线): 连接swing点[#{:#?}@{:.2}→#{:#?}@{:.2}], 斜率={:.4}({}), {}",
            type_cn, p1.index, p1.price, p2.index, p2.price, slope, trend_desc, role
        ),
    })
}

/// 判断价格是否有效突破供给线
///
/// 条件：
/// 1. 收盘价 > 供给线当前值 * 1.03
/// 2. 突破时放量
pub fn is_supply_line_breakout(klines: &[KLine], line: &SupplyDemandLine, idx: usize) -> bool {
    let k = &klines[idx];
    let avg_vol: f64 = klines.iter().map(|k| k.vol).sum::<f64>() / klines.len() as f64;

    let line_level = extrapolate_line(line, idx);
    let breakout_pct = (k.close - line_level) / line_level;

    breakout_pct > 0.03 && k.vol > avg_vol * 1.3
}

/// 判断价格是否有效跌破需求线
///
/// 条件：
/// 1. 收盘价 < 需求线当前值 * 0.97
/// 2. 跌破时放量
pub fn is_demand_line_breakdown(klines: &[KLine], line: &SupplyDemandLine, idx: usize) -> bool {
    let k = &klines[idx];
    let avg_vol: f64 = klines.iter().map(|k| k.vol).sum::<f64>() / klines.len() as f64;

    let line_level = extrapolate_line(line, idx);
    let breakdown_pct = (line_level - k.close) / line_level;

    breakdown_pct > 0.03 && k.vol > avg_vol * 1.3
}

/// 外推趋势线到指定索引
fn extrapolate_line(line: &SupplyDemandLine, idx: usize) -> f64 {
    let steps = idx as f64 - line.start_index as f64;
    line.start_price + line.slope * steps
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
    fn test_supply_demand_lines() {
        // 构造有波动的序列（有 swing points）
        let klines: Vec<KLine> = (0..25)
            .map(|i| {
                let base = 10.0 + (i as f64 * 0.5).sin() * 2.0;
                make_kline(base - 0.1, base + 0.1, base + 0.5, base - 0.5, 500.0, i)
            })
            .collect();

        let lines = draw_supply_demand_lines(&klines, &[]);
        // 极值点法可能生成供需线，也可能不（取决于 swing point 检测）
        // 至少不应 panic
    }

    #[test]
    fn test_tr_range_lines() {
        let klines: Vec<KLine> = (0..20)
            .map(|i| make_kline(10.0, 10.5, 11.0, 9.0, 500.0, i))
            .collect();

        let tr = TradingRange {
            start_index: 5,
            end_index: 15,
            upper: 11.0,
            lower: 9.0,
            ice_line: 10.0,
        };

        let lines = draw_supply_demand_lines(&klines, &vec![tr]);
        let supply_lines: Vec<_> = lines.iter().filter(|l| l.line_type == "supply").collect();
        let demand_lines: Vec<_> = lines.iter().filter(|l| l.line_type == "demand").collect();
        let ice_lines: Vec<_> = lines.iter().filter(|l| l.line_type == "ice_line").collect();

        assert!(!supply_lines.is_empty());
        assert!(!demand_lines.is_empty());
        assert!(!ice_lines.is_empty());
    }
}
