//! 缠论线段构建（特征序列分型破坏法）
//!
//! 严格依据缠论第71课、77课原文：
//!
//! 1. 线段由至少3笔构成
//! 2. 特征序列 = 与线段方向相反的笔
//! 3. 特征序列包含处理方向 = 线段方向
//!    - 上升线段：high=max, low=max
//!    - 下降线段：high=min, low=min
//! 4. 顶分型终结上升线段，底分型终结下降线段
//! 5. 无缺口：直接终结
//! 6. 有缺口：需要后续笔确认（反向创新低/高不确认）

use yifang_data::{Bi, XianDuan};

const DEFAULT_MIN_XD_LEN: usize = 3;

pub fn build_xd(bis: &[Bi]) -> Vec<XianDuan> {
    build_xd_with_min_len(bis, None)
}

pub fn build_xd_with_min_len(bis: &[Bi], min_xd_len: Option<usize>) -> Vec<XianDuan> {
    let min_len = min_xd_len.unwrap_or(DEFAULT_MIN_XD_LEN);
    build_xd_impl(bis, min_len)
}

// ─── 特征序列元素 ──────────────────────────────────────

#[derive(Debug, Clone)]
struct FeatureElement {
    high: f64,
    low: f64,
    /// 极值所在笔的索引（用于回溯线段终点）
    bi_index: usize,
}

impl FeatureElement {
    fn from_bi(bi: &Bi, bi_index: usize) -> Self {
        Self {
            high: bi.start_price.max(bi.end_price),
            low: bi.start_price.min(bi.end_price),
            bi_index,
        }
    }
}

// ─── 包含处理 ─────────────────────────────────────────

/// 缠论71课：特征序列的包含处理方向 = 特征序列方向（而非线段方向）
/// - 上升线段的特征序列由下降笔组成 → 下降方向处理：high=min, low=min
/// - 下降线段的特征序列由上升笔组成 → 上升方向处理：high=max, low=max
///
/// 包含合并时 bi_index 保留极值所在笔：
/// - 下降方向（低低）：保留低点所在笔
/// - 上升方向（高高）：保留高点所在笔
fn feature_seq_push(feature_seq: &mut Vec<FeatureElement>, elem: FeatureElement, is_xd_up: bool) {
    if feature_seq.is_empty() {
        feature_seq.push(elem);
        return;
    }

    let last = feature_seq.last().unwrap();
    // 包含关系：一根完全包含另一根
    let a_contains_b = last.high >= elem.high && last.low <= elem.low;
    let b_contains_a = elem.high >= last.high && elem.low <= last.low;

    if a_contains_b || b_contains_a {
        let last = feature_seq.last_mut().unwrap();
        if is_xd_up {
            // 上升线段的特征序列方向=下降 → 下降方向处理：低低
            let keep_last_idx = last.low <= elem.low;
            last.high = last.high.min(elem.high);
            last.low = last.low.min(elem.low);
            if !keep_last_idx {
                last.bi_index = elem.bi_index;
            }
        } else {
            // 下降线段的特征序列方向=上升 → 上升方向处理：高高
            let keep_last_idx = last.high >= elem.high;
            last.high = last.high.max(elem.high);
            last.low = last.low.max(elem.low);
            if !keep_last_idx {
                last.bi_index = elem.bi_index;
            }
        }
    } else {
        feature_seq.push(elem);
    }
}

// ─── 分型检测 ────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq)]
enum FenxingType {
    Top,    // 顶分型：prev.high < curr.high > next.high
    Bottom, // 底分型：prev.low  > curr.low  < next.low
}

fn check_fenxing(prev: &FeatureElement, curr: &FeatureElement, next: &FeatureElement) -> Option<FenxingType> {
    let is_top = prev.high < curr.high && curr.high > next.high;
    let is_bottom = prev.low > curr.low && curr.low < next.low;

    if is_top {
        Some(FenxingType::Top)
    } else if is_bottom {
        Some(FenxingType::Bottom)
    } else {
        None
    }
}

// ─── 缺口检测 ────────────────────────────────────────

/// 缺口 = 分型第一元素和第二元素之间没有价格重叠
fn has_gap(prev: &FeatureElement, curr: &FeatureElement, is_xd_up: bool) -> bool {
    if is_xd_up {
        // 上升线段中，前一个特征（下降笔）的低 > 后一个特征的高 → 缺口
        prev.low > curr.high
    } else {
        // 下降线段中，前一个特征（上升笔）的高 < 后一个特征的低 → 缺口
        prev.high < curr.low
    }
}

// ─── 前三笔重叠检查 ──────────────────────────────────

/// 缠论：前三笔必须有重叠区域，否则不能构成线段
fn check_overlap_of_first_3(bis: &[Bi], start: usize) -> bool {
    if start + 3 > bis.len() {
        return false;
    }

    let mut max_low = f64::MIN;
    let mut min_high = f64::MAX;

    for i in start..start + 3 {
        let bi = &bis[i];
        let high = bi.start_price.max(bi.end_price);
        let low = bi.start_price.min(bi.end_price);
        max_low = max_low.max(low);
        min_high = min_high.min(high);
    }

    max_low <= min_high
}

// ─── 核心算法 ─────────────────────────────────────────

fn build_xd_impl(bis: &[Bi], min_len: usize) -> Vec<XianDuan> {
    if bis.len() < min_len {
        return Vec::new();
    }

    let mut xds: Vec<XianDuan> = Vec::new();
    let mut xd_start: usize = 0;

    while xd_start < bis.len() {
        // 线段方向
        // 首段：取起始笔的方向
        // 后续段：与前一段相反（线段首尾相连）
        let is_xd_up = if xds.is_empty() {
            bis[xd_start].direction.as_str() == "up"
        } else {
            xds.last().unwrap().direction.as_str() != "up"
        };

        // 前三笔重叠检查
        if !check_overlap_of_first_3(bis, xd_start) {
            if xd_start + 3 >= bis.len() {
                break;
            }
            xd_start += 1;
            continue;
        }

        let mut feature_seq: Vec<FeatureElement> = Vec::new();
        let mut found_break = false;

        for i in (xd_start + 1)..bis.len() {
            let bi = &bis[i];
            let xd_dir = if is_xd_up { "up" } else { "down" };

            // 特征序列 = 与线段方向相反的笔
            if bi.direction.as_str() != xd_dir {
                let elem = FeatureElement::from_bi(bi, i);
                feature_seq_push(&mut feature_seq, elem, is_xd_up);
            }

            // 分型检测（最后3个特征元素）
            if feature_seq.len() >= 3 {
                let n = feature_seq.len();
                let prev = &feature_seq[n - 3];
                let curr = &feature_seq[n - 2];
                let next = &feature_seq[n - 1];

                let fenxing = check_fenxing(prev, curr, next);

                // 上升线段被顶分型终结，下降线段被底分型终结
                let is_break = match fenxing {
                    Some(FenxingType::Top) => is_xd_up,
                    Some(FenxingType::Bottom) => !is_xd_up,
                    None => false,
                };

                if is_break {
                    // 分型中间元素的 bi_index = 特征笔索引（反向笔）
                    // 线段终点 = 该反向笔的前一笔（同向笔）
                    let break_bi_idx = curr.bi_index;
                    if break_bi_idx == 0 {
                        continue;
                    }
                    let end_bi_idx = break_bi_idx - 1;

                    // 至少 min_len 笔
                    if end_bi_idx < xd_start || end_bi_idx - xd_start + 1 < min_len {
                        continue;
                    }

                    let gap = has_gap(prev, curr, is_xd_up);

                    if !gap {
                        // 无缺口：直接终结
                        push_xd(&mut xds, bis, xd_start, end_bi_idx, is_xd_up, true);
                        xd_start = end_bi_idx;
                        found_break = true;
                        break;
                    } else {
                        // 有缺口：需要确认——反向没有创新低/高
                        let reverse_innovation = check_reverse_innovation(
                            bis, break_bi_idx, !is_xd_up,
                        );
                        if !reverse_innovation {
                            push_xd(&mut xds, bis, xd_start, end_bi_idx, is_xd_up, true);
                            xd_start = end_bi_idx;
                            found_break = true;
                            break;
                        }
                        // 有缺口且反向创新低/高，说明原线段延续，不终结
                    }
                }
            }
        }

        if !found_break {
            break;
        }
    }

    // 未完成线段
    if xd_start < bis.len() {
        let is_xd_up = if xds.is_empty() {
            bis[xd_start].direction.as_str() == "up"
        } else {
            xds.last().unwrap().direction.as_str() != "up"
        };

        if check_overlap_of_first_3(bis, xd_start) || bis.len() - xd_start < 3 {
            let start_bi = &bis[xd_start];
            let end_bi = &bis[bis.len() - 1];

            // start_price = 起点笔的起点价，end_price = 最后一笔的终点价
            // 这样前端画线时，线段折线的 Y 值与 K 线价格吻合
            xds.push(XianDuan {
                direction: if is_xd_up { "up" } else { "down" }.to_string(),
                start_index: start_bi.start_index,
                end_index: end_bi.end_index,
                start_dt: start_bi.start_dt.clone(),
                end_dt: end_bi.end_dt.clone(),
                start_price: start_bi.start_price,
                end_price: end_bi.end_price,
                is_finished: false,
            });
        }
    }

    xds
}

/// 检查从 break_bi_idx 开始，反向线段是否创新低/高
/// 返回 true = 反向创新了，说明原线段延续（缺口被补，不能终结）
fn check_reverse_innovation(bis: &[Bi], break_bi_idx: usize, reverse_is_xd_up: bool) -> bool {
    let _reverse_dir = if reverse_is_xd_up { "up" } else { "down" };

    // 上升线段创新高 = 高点超过 break_bi_idx 处的高点
    // 下降线段创新低 = 低点低于 break_bi_idx 处的低点
    let baseline_high = bis[break_bi_idx].start_price.max(bis[break_bi_idx].end_price);
    let baseline_low = bis[break_bi_idx].start_price.min(bis[break_bi_idx].end_price);

    for i in (break_bi_idx + 1)..bis.len() {
        let bi = &bis[i];
        let bi_high = bi.start_price.max(bi.end_price);
        let bi_low = bi.start_price.min(bi.end_price);

        if reverse_is_xd_up && bi_high > baseline_high {
            return true;
        }
        if !reverse_is_xd_up && bi_low < baseline_low {
            return true;
        }
    }
    false
}

/// 添加一个完成的线段
fn push_xd(
    xds: &mut Vec<XianDuan>,
    bis: &[Bi],
    start_bi_idx: usize,
    end_bi_idx: usize,
    is_xd_up: bool,
    is_finished: bool,
) {
    let start_bi = &bis[start_bi_idx];
    let end_bi = &bis[end_bi_idx];

    // start_price = 起点笔的起点价, end_price = 终点笔的终点价
    // 前端用这两个值画折线，必须对应K线上的实际价格
    xds.push(XianDuan {
        direction: if is_xd_up { "up" } else { "down" }.to_string(),
        start_index: start_bi.start_index,
        end_index: end_bi.end_index,
        start_dt: start_bi.start_dt.clone(),
        end_dt: end_bi.end_dt.clone(),
        start_price: start_bi.start_price,
        end_price: end_bi.end_price,
        is_finished,
    });
}

// ═════════════════════════════════════════════════════════
// 测试
// ═════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    fn make_bi(id: usize, dir: &str, start_price: f64, end_price: f64, start_index: u64, end_index: u64) -> Bi {
        Bi {
            direction: dir.to_string(),
            start_index,
            end_index,
            start_dt: format!("2024-01-{:02}", id + 1),
            end_dt: format!("2024-01-{:02}", id + 2),
            start_price,
            end_price,
            is_finished: true,
        }
    }

    fn gen_klines(seed: u64, n: usize) -> Vec<yifang_data::KLine> {
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let base_id = COUNTER.fetch_add(n as u64, Ordering::SeqCst);

        let mut rng_state = seed;
        let mut rng = || {
            rng_state = rng_state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            (rng_state >> 33) as f64 / (1u64 << 31) as f64
        };

        let mut klines = Vec::with_capacity(n);
        let mut price = 100.0;
        for i in 0..n {
            price += (rng() - 0.48) * 3.0;
            if price < 5.0 { price = 5.0; }
            let high = price + rng() * 2.0;
            let low = price - rng() * 2.0;
            let open = low + rng() * (high - low);
            let close = low + rng() * (high - low);
            klines.push(yifang_data::KLine {
                symbol: format!("TEST{}", seed),
                timeframe: yifang_data::TimeFrame::D,
                dt: format!("2024-01-{:02}", (i % 28) + 1),
                id: base_id + i as u64,
                open,
                close,
                high,
                low,
                vol: 1000.0 + rng() * 5000.0,
                amount: 0.0,
            });
        }
        klines
    }

    #[test]
    fn test_fenxing_detection() {
        // 顶分型：h递增 → h最高 → h递减
        let prev = FeatureElement { high: 10.0, low: 8.0, bi_index: 0 };
        let curr = FeatureElement { high: 12.0, low: 9.0, bi_index: 1 };
        let next = FeatureElement { high: 11.0, low: 7.0, bi_index: 2 };
        assert_eq!(check_fenxing(&prev, &curr, &next), Some(FenxingType::Top));

        // 底分型：l递减 → l最低 → l递增
        let prev = FeatureElement { high: 10.0, low: 8.0, bi_index: 0 };
        let curr = FeatureElement { high: 9.0, low: 6.0, bi_index: 1 };
        let next = FeatureElement { high: 11.0, low: 7.0, bi_index: 2 };
        assert_eq!(check_fenxing(&prev, &curr, &next), Some(FenxingType::Bottom));

        // 无分型
        let prev = FeatureElement { high: 10.0, low: 8.0, bi_index: 0 };
        let curr = FeatureElement { high: 12.0, low: 9.0, bi_index: 1 };
        let next = FeatureElement { high: 13.0, low: 10.0, bi_index: 2 };
        assert_eq!(check_fenxing(&prev, &curr, &next), None);
    }

    #[test]
    fn test_gap_detection() {
        let a = FeatureElement { high: 12.0, low: 10.0, bi_index: 0 };
        let b = FeatureElement { high: 9.0, low: 7.0, bi_index: 1 };
        // 上升线段：a.low(10) > b.high(9) → 有缺口
        assert!(has_gap(&a, &b, true));
        // 下降线段：a.high(12) < b.low(7)？ No
        assert!(!has_gap(&a, &b, false));
    }

    #[test]
    fn test_overlap_check() {
        // 3笔有重叠
        let bis = vec![
            make_bi(0, "up", 10.0, 15.0, 0, 2),
            make_bi(1, "down", 15.0, 12.0, 2, 4),
            make_bi(2, "up", 12.0, 16.0, 4, 6),
        ];
        assert!(check_overlap_of_first_3(&bis, 0));

        // 3笔无重叠（第二笔终点远低于第一笔起点）
        let bis2 = vec![
            make_bi(0, "up", 10.0, 15.0, 0, 2),
            make_bi(1, "down", 15.0, 3.0, 2, 4),
            make_bi(2, "up", 3.0, 5.0, 4, 6),
        ];
        assert!(!check_overlap_of_first_3(&bis2, 0));
    }

    #[test]
    fn test_contain_up_xd() {
        // 上升线段的特征序列方向=下降 → 包含处理用下降方向：high=min, low=min
        let mut seq = Vec::new();
        // A(10,7) 包含 B(9,8)：10>=9 且 7<=8 → 合并为 (9,7)
        feature_seq_push(&mut seq, FeatureElement { high: 10.0, low: 7.0, bi_index: 0 }, true);
        feature_seq_push(&mut seq, FeatureElement { high: 9.0, low: 8.0, bi_index: 1 }, true);
        assert_eq!(seq.len(), 1);
        assert_eq!(seq[0].high, 9.0);   // min(10,9)
        assert_eq!(seq[0].low, 7.0);    // min(7,8)

        // 添加不包含的：C(12,9) —— 9>=12? No，12>=9且9<=7? No → 不包含
        feature_seq_push(&mut seq, FeatureElement { high: 12.0, low: 9.0, bi_index: 2 }, true);
        assert_eq!(seq.len(), 2);
    }

    #[test]
    fn test_contain_down_xd() {
        // 下降线段的特征序列方向=上升 → 包含处理用上升方向：high=max, low=max
        let mut seq = Vec::new();
        // A(10,7) 包含 B(9,8)：10>=9 且 7<=8 → 合并为 (10,8)
        feature_seq_push(&mut seq, FeatureElement { high: 10.0, low: 7.0, bi_index: 0 }, false);
        feature_seq_push(&mut seq, FeatureElement { high: 9.0, low: 8.0, bi_index: 1 }, false);
        assert_eq!(seq.len(), 1);
        assert_eq!(seq[0].high, 10.0);  // max(10,9)
        assert_eq!(seq[0].low, 8.0);    // max(7,8)

        // 添加不包含的：C(12,9) —— 10>=12? No，12>=10且9<=8? No → 不包含
        feature_seq_push(&mut seq, FeatureElement { high: 12.0, low: 9.0, bi_index: 2 }, false);
        assert_eq!(seq.len(), 2);
    }

    #[test]
    fn test_xd_min_3_bi() {
        // 2笔不能构成线段
        let bis = vec![
            make_bi(0, "up", 10.0, 15.0, 0, 2),
            make_bi(1, "down", 15.0, 12.0, 2, 4),
        ];
        let xds = build_xd(&bis);
        assert!(xds.is_empty() || !xds[0].is_finished);
    }

    #[test]
    fn test_xd_000001() {
        // 平安银行000001 日线（2024-01-11起9笔数据）
        let bis = vec![
            make_bi(0, "up",   8.96,  9.62, 2, 5),   // BI[0]
            make_bi(1, "down", 9.62,  9.10, 5, 8),   // BI[1]
            make_bi(2, "up",   9.10, 10.80, 8, 14),   // BI[2]
            make_bi(3, "down",10.80, 10.18, 14, 17),  // BI[3]
            make_bi(4, "up",  10.18, 11.36, 17, 22),  // BI[4] 创新高
            make_bi(5, "down",11.36, 10.48, 22, 26),  // BI[5]
            make_bi(6, "up",  10.48, 11.74, 26, 32),  // BI[6] 再创新高
            make_bi(7, "down",11.74, 10.84, 32, 37),  // BI[7]
            make_bi(8, "up",  10.84, 13.43, 37, 43),  // BI[8]
        ];

        let xds = build_xd(&bis);

        // 至少要有1个线段
        assert!(!xds.is_empty(), "应该至少产生1个线段");

        // 线段1应该是上升线段
        assert_eq!(xds[0].direction, "up", "第一个线段方向应为上升");

        // 线段起点价格应为8.96（BI[0]的起点价）
        assert!((xds[0].start_price - 8.96).abs() < 0.01,
            "线段起点价格应为8.96，实际={}", xds[0].start_price);

        // 验证线段方向交替
        for i in 1..xds.len() {
            assert_ne!(xds[i].direction, xds[i-1].direction,
                "线段[{}]和[{}]方向不应相同", i, i-1);
        }
    }

    #[test]
    fn test_xd_random_directions_alternate() {
        // 随机数据测试：线段方向必须交替
        for seed in [42u64, 123, 456, 789, 1024, 2048, 3000, 4000] {
            let klines = gen_klines(seed, 500);
            let bis = crate::bi::build_bi(&klines, None);
            if bis.len() < 3 { continue; }

            // 检查笔方向交替
            let mut bi_alt = true;
            for i in 1..bis.len() {
                if bis[i].direction.as_str() == bis[i-1].direction.as_str() {
                    bi_alt = false;
                    break;
                }
            }
            if !bi_alt { continue; }

            let xds = build_xd(&bis);
            for i in 1..xds.len() {
                assert_ne!(xds[i].direction, xds[i-1].direction,
                    "seed={}: 线段[{}]和[{}]方向相同", seed, i, i-1);
            }
        }
    }

    #[test]
    fn test_xd_strict_chanlun_rules() {
        // 严格缠论规则验证
        for seed in [42u64, 123, 456, 789, 1024, 2048, 3000, 4000] {
            let klines = gen_klines(seed, 500);
            let bis = crate::bi::build_bi(&klines, None);
            if bis.len() < 3 { continue; }

            let mut bi_alternating = true;
            for i in 1..bis.len() {
                if bis[i].direction.as_str() == bis[i-1].direction.as_str() {
                    bi_alternating = false;
                    break;
                }
            }
            if !bi_alternating { continue; }

            let xds = build_xd(&bis);

            // 完成线段方向与价格一致
            for (i, xd) in xds.iter().enumerate() {
                if xd.is_finished {
                    if xd.direction.as_str() == "up" {
                        assert!(xd.start_price < xd.end_price,
                            "seed={}: 上升线段[{}] start={:.2} >= end={:.2}", seed, i, xd.start_price, xd.end_price);
                    } else {
                        assert!(xd.start_price > xd.end_price,
                            "seed={}: 下降线段[{}] start={:.2} <= end={:.2}", seed, i, xd.start_price, xd.end_price);
                    }
                }
            }

            // 线段方向交替
            for i in 1..xds.len() {
                assert_ne!(xds[i].direction, xds[i-1].direction,
                    "seed={}: 线段[{}]和[{}]方向相同", seed, i, i-1);
            }
        }
    }
}
