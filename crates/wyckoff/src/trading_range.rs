//! 交易区间识别（结构驱动）
//!
//! **严格对齐威科夫交易区间 (Trading Range) 定义**
//!
//! 交易区间是威科夫理论的"因"（因果法则），是吸筹/派发的核心结构。
//!
//! **定义**：
//! - 交易区间是一个横盘的价格区间，由上下沿界定
//! - 吸筹区间：由 SC 的低点和 AR 的高点界定
//! - 派发区间：由 BC 的高点和 AR 的低点界定
//! - 冰线 (ICE)：区间中间的关键价格水平
//!
//! **识别方法**：
//! 1. 从事件序列中找到 SC/BC 和 AR
//! 2. SC.low 和 AR.high 形成吸筹区间
//! 3. BC.high 和 AR.low 形成派发区间
//! 4. 区间的结束点由 SOS/UTAD/SOW 标志
//! 5. 如果没有事件，用局部极值点识别横盘区域
//!
//! **扩展**：
//! - 区间可能被 Spring/UTAD 临时突破但随后收回
//! - Phase B 中可能有多次在区间内的波动
//! - 区间结束由离开事件标志：SOS 突破上沿或 SOW 跌破下沿

use yifang_data::{KLine, WyckoffEvent, TradingRange};
use crate::phase::find_swing_points;

/// 从事件序列中识别交易区间（结构驱动）
///
/// 优先使用 SC/BC + AR 事件定义区间，
/// 如果没有事件则基于极值点识别。
pub fn detect_trading_ranges_structured(klines: &[KLine], events: &[WyckoffEvent]) -> Vec<TradingRange> {
    let mut ranges = Vec::new();

    // 1. 先尝试从事件序列中识别
    let event_ranges = detect_from_events(klines, events);
    ranges.extend(event_ranges);

    // 2. 如果事件法没有找到区间，用极值点法
    if ranges.is_empty() {
        let extrema_ranges = detect_from_swing_points(klines);
        ranges.extend(extrema_ranges);
    }

    ranges
}

/// 从事件序列构建交易区间
///
/// - SC + AR → 吸筹区间 (SC.low ~ AR.high)
/// - BC + AR → 派发区间 (AR.low ~ BC.high)
fn detect_from_events(klines: &[KLine], events: &[WyckoffEvent]) -> Vec<TradingRange> {
    let mut ranges = Vec::new();

    // 找 SC + AR 组合（吸筹区间）
    let sc_events: Vec<_> = events.iter().filter(|e| e.event_type == "SC").collect();
    let ar_events: Vec<_> = events.iter().filter(|e| e.event_type == "AR").collect();

    for sc in &sc_events {
        // 找 SC 之后的第一个 AR
        if let Some(ar) = ar_events.iter().find(|ar| ar.index > sc.index && ar.index - sc.index <= 10) {
            let sc_idx = sc.index as usize;
            let _ar_idx = ar.index as usize;
            
            let lower = sc.price; // SC 的低点
            let upper = ar.price;  // AR 的高点

            if upper > lower {
                // 找区间的结束点——SOS 或 SOW 或序列末端
                let end_idx = find_range_end(klines, events, ar.index, upper, lower);

                let ice_line = (upper + lower) / 2.0;

                ranges.push(TradingRange {
                    start_index: sc_idx as u64,
                    end_index: end_idx as u64,
                    upper,
                    lower,
                    ice_line,
                });
            }
        }
    }

    // 找 BC + AR 组合（派发区间）
    let bc_events: Vec<_> = events.iter().filter(|e| e.event_type == "BC").collect();

    for bc in &bc_events {
        if let Some(ar) = ar_events.iter().find(|ar| ar.index > bc.index && ar.index - bc.index <= 10) {
            let bc_idx = bc.index as usize;
            let _ar_idx = ar.index as usize;

            let upper = bc.price;  // BC 的高点
            let lower = ar.price;  // AR 的低点（派发中的 AR 是回落）

            if upper > lower {
                let end_idx = find_range_end(klines, events, ar.index, upper, lower);
                let ice_line = (upper + lower) / 2.0;

                ranges.push(TradingRange {
                    start_index: bc_idx as u64,
                    end_index: end_idx as u64,
                    upper,
                    lower,
                    ice_line,
                });
            }
        }
    }

    // 合并重叠的区间
    merge_overlapping_ranges(ranges)
}

/// 从极值点识别交易区间
///
/// 当没有明显的 SC/BC 事件时，用 swing points 识别横盘区域。
///
/// 方法：
/// 1. 找出所有局部高点和低点
/// 2. 将相近的高点归组（阻力线），相近的低点归组（支撑线）
/// 3. 阻力线和支撑线之间构成交易区间
fn detect_from_swing_points(klines: &[KLine]) -> Vec<TradingRange> {
    let swing_points = find_swing_points(klines, 5);
    let highs: Vec<_> = swing_points.iter().filter(|p| p.is_high).collect();
    let lows: Vec<_> = swing_points.iter().filter(|p| !p.is_high).collect();

    if highs.len() < 2 || lows.len() < 2 {
        return Vec::new();
    }

    // 找阻力线：相近的高点（价格差异 < 5%）
    let resistance_groups = group_by_price(&highs, 0.05);
    // 找支撑线：相近的低点
    let support_groups = group_by_price(&lows, 0.05);

    let mut ranges = Vec::new();

    for res_group in &resistance_groups {
        for sup_group in &support_groups {
            let res_avg = avg_price(res_group);
            let sup_avg = avg_price(sup_group);

            if res_avg > sup_avg {
                let range_pct = (res_avg - sup_avg) / sup_avg;
                // 交易区间波幅不宜过大（<20%）
                if range_pct < 0.20 {
                    let start_idx = res_group.iter().chain(sup_group.iter()).map(|p| p.index).min().unwrap();
                    let end_idx = res_group.iter().chain(sup_group.iter()).map(|p| p.index).max().unwrap();

                    ranges.push(TradingRange {
                        start_index: start_idx as u64,
                        end_index: end_idx as u64,
                        upper: res_avg,
                        lower: sup_avg,
                        ice_line: (res_avg + sup_avg) / 2.0,
                    });
                }
            }
        }
    }

    merge_overlapping_ranges(ranges)
}

/// 找到交易区间的结束索引
///
/// 区间结束的标志：
/// - SOS 突破上沿 → 区间结束
/// - SOW 跌破下沿 → 区间结束
/// - 否则延伸到数据末端
fn find_range_end(klines: &[KLine], events: &[WyckoffEvent], after_index: u64, upper: f64, lower: f64) -> usize {
    // 检查是否有 SOS/SOW 事件
    let exit_events: Vec<_> = events.iter()
        .filter(|e| e.index > after_index)
        .filter(|e| e.event_type == "SOS" || e.event_type == "SOW" || e.event_type == "UTAD")
        .collect();

    if let Some(first_exit) = exit_events.first() {
        return first_exit.index as usize;
    }

    // 检查价格是否持续在区间外
    for i in (after_index as usize)..klines.len() {
        if klines[i].close > upper * 1.02 || klines[i].close < lower * 0.98 {
            // 持续 3 根 K 线在区间外
            if i + 3 <= klines.len() {
                let all_outside = klines[i..i + 3].iter().all(|k| {
                    k.close > upper * 1.02 || k.close < lower * 0.98
                });
                if all_outside {
                    return i;
                }
            }
        }
    }

    klines.len().saturating_sub(1)
}

/// 按价格相近度分组
fn group_by_price<'a>(points: &[&'a crate::phase::SwingPoint], threshold: f64) -> Vec<Vec<&'a crate::phase::SwingPoint>> {
    if points.is_empty() {
        return Vec::new();
    }

    let mut groups: Vec<Vec<&'a crate::phase::SwingPoint>> = Vec::new();
    let mut current_group = vec![points[0]];

    for i in 1..points.len() {
        let group_avg = avg_price(&current_group);
        if (points[i].price - group_avg).abs() / group_avg < threshold {
            current_group.push(points[i]);
        } else {
            if current_group.len() >= 2 {
                groups.push(current_group);
            }
            current_group = vec![points[i]];
        }
    }
    if current_group.len() >= 2 {
        groups.push(current_group);
    }

    groups
}

/// 计算一组极值点的平均价格
fn avg_price(points: &[&crate::phase::SwingPoint]) -> f64 {
    if points.is_empty() { return 0.0; }
    points.iter().map(|p| p.price).sum::<f64>() / points.len() as f64
}

/// 合并重叠的交易区间
fn merge_overlapping_ranges(ranges: Vec<TradingRange>) -> Vec<TradingRange> {
    if ranges.len() <= 1 {
        return ranges;
    }

    let mut sorted = ranges;
    sorted.sort_by_key(|r| r.start_index);

    let mut merged = Vec::new();
    let mut current = sorted[0].clone();

    for r in sorted.iter().skip(1) {
        // 判断是否重叠
        if r.start_index <= current.end_index {
            // 合并
            current.end_index = current.end_index.max(r.end_index);
            current.upper = current.upper.max(r.upper);
            current.lower = current.lower.min(r.lower);
            current.ice_line = (current.upper + current.lower) / 2.0;
        } else {
            merged.push(current);
            current = r.clone();
        }
    }
    merged.push(current);

    merged
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
    fn test_detect_tr_range_from_events() {
        let klines: Vec<KLine> = (0..30)
            .map(|i| {
                if i < 15 {
                    // 下跌
                    make_kline(20.0 - (i as f64), 19.0 - (i as f64), 20.5 - (i as f64), 18.5 - (i as f64), 500.0, i)
                } else {
                    // 横盘震荡
                    make_kline(10.0, 10.0 + ((i - 15) as f64 % 3.0 - 1.0) as f64, 12.0, 8.0, 300.0, i)
                }
            })
            .collect();

        let events = crate::pattern::detect_events(&klines);
        let ranges = detect_trading_ranges_structured(&klines, &events);
        // 结果取决于事件检测结果
        // 至少不应该 panic
    }

    #[test]
    fn test_detect_from_swing_points() {
        // 构造横盘
        let klines: Vec<KLine> = (0..25)
            .map(|i| {
                let price = 10.0 + ((i as f64) * 0.3).sin() * 1.0;
                make_kline(price - 0.2, price, price + 0.5, price - 0.5, 500.0, i)
            })
            .collect();

        let ranges = detect_from_swing_points(&klines);
        // 横盘区域应该能识别出交易区间
        // 至少不 panic
    }
}
