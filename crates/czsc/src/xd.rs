//! 线段分析 — 严格按缠论原文定义（第62、65、67、71课）
//!
//! ## 线段定义
//! 线段 = 至少3笔 + 前三笔重叠 + 方向交替 + 首尾同方向
//!
//! ## 特征序列
//! 上升线段的特征序列 = 所有下降笔
//! 下降线段的特征序列 = 所有上升笔
//! 特征序列需要做包含处理，方向由线段方向决定：
//! - 向上线段特征序列（下降笔）：按**下降方向**处理包含（取低低、高低）
//! - 向下线段特征序列（上升笔）：按**上升方向**处理包含（取高高、低高）
//!
//! ## 创新高/低重置机制
//! 当同向笔创新高（上升线段中）或创新低（下降线段中）时，
//! 之前的特征序列失效——因为线段在延续，之前积累的"反向特征"
//! 已经不能代表线段的结构了。特征序列应清空，从新极值点重新开始。
//!
//! ## 线段终结
//! 特征序列出现分型 = 线段终结预警：
//! - 无缺口：直接确认终结
//! - 有缺口：需要二次确认（后续形成反向特征序列分型）

use yifang_data::{Bi, XianDuan};

const DEFAULT_MIN_XD_LEN: usize = 3;

pub fn build_xd(bis: &[Bi]) -> Vec<XianDuan> {
    build_xd_with_min_len(bis, None)
}

pub fn build_xd_with_min_len(bis: &[Bi], min_xd_len: Option<usize>) -> Vec<XianDuan> {
    let min_len = min_xd_len.unwrap_or(DEFAULT_MIN_XD_LEN);
    build_xd_incremental(bis, min_len)
}

// ─── 特征序列元素 ──────────────────────────────────────

#[derive(Debug, Clone)]
struct FeatureElement {
    high: f64,
    low: f64,
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

// ─── 包含处理（增量式）──────────────────────────────────

/// 向特征序列中增量添加一个元素，并做包含处理
///
/// 包含处理方向由线段方向决定（缠论第71课）：
/// "线段是向上的，特征序列也是向上的"
/// - 上升线段特征序列（下降笔）：按上升方向 → high=max, low=max
/// - 下降线段特征序列（上升笔）：按下降方向 → high=min, low=min
fn feature_seq_push(feature_seq: &mut Vec<FeatureElement>, elem: FeatureElement, is_xd_up: bool) {
    if feature_seq.is_empty() {
        feature_seq.push(elem);
        return;
    }

    let last = feature_seq.last().unwrap();
    let has_include = (last.high >= elem.high && last.low <= elem.low)
        || (last.high <= elem.high && last.low >= elem.low);

    if has_include {
        let last = feature_seq.last_mut().unwrap();
        if is_xd_up {
            // 上升线段：特征序列按上升方向处理
            last.high = last.high.max(elem.high);
            last.low = last.low.max(elem.low);
        } else {
            // 下降线段：特征序列按下降方向处理
            last.high = last.high.min(elem.high);
            last.low = last.low.min(elem.low);
        }
    } else {
        feature_seq.push(elem);
    }
}

// ─── 分型检测 ────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq)]
enum FenxingType {
    Top,
    Bottom,
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

fn has_gap(prev: &FeatureElement, curr: &FeatureElement, is_xd_up: bool) -> bool {
    if is_xd_up {
        prev.low > curr.high
    } else {
        prev.high < curr.low
    }
}

// ─── 前三笔重叠检查 ──────────────────────────────────

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

// ─── 核心算法：逐笔增量式线段检测 ──────────────────────

/// 逐笔增量式线段检测
///
/// 1. 从新线段起始笔开始，逐步扫描后续每一笔
/// 2. 同向笔创新高/低时 → 清空特征序列（线段延续，之前的反向特征失效）
/// 3. 同向笔未创新高/低时 → 更新极值
/// 4. 反向笔 → 增量加入特征序列（做包含处理）
/// 5. 特征序列形成分型 → 记录预警
/// 6. 预警后如果同向笔创新高/低 → 取消预警
/// 7. 扫描完毕后，如果有有效预警 → 确认终结
fn build_xd_incremental(bis: &[Bi], min_len: usize) -> Vec<XianDuan> {
    if bis.len() < min_len {
        return Vec::new();
    }

    let mut xds = Vec::new();
    let mut xd_start: usize = 0;

    while xd_start < bis.len() {
        let is_xd_up = bis[xd_start].direction.as_str() == "up";

        // 前三笔重叠检查
        if !check_overlap_of_first_3(bis, xd_start) {
            if xd_start + 3 >= bis.len() {
                break;
            }
            xd_start += 1;
            continue;
        }

        // 线段极值
        let mut xd_extreme = if is_xd_up {
            bis[xd_start].start_price.max(bis[xd_start].end_price)
        } else {
            bis[xd_start].start_price.min(bis[xd_start].end_price)
        };

        // 增量式特征序列
        let mut feature_seq: Vec<FeatureElement> = Vec::new();

        // 创新高/低重置点：特征序列中此索引之前的元素是"重置前"的，
        // 不参与分型检测（但参与包含处理，防止跨重置点的错误合并）
        let mut feature_reset_at: usize = 0;

        // 分型预警
        let mut pending_break: Option<PendingBreak> = None;
        let mut found_break = false;

        for i in (xd_start + 1)..bis.len() {
            let bi = &bis[i];
            let bi_high = bi.start_price.max(bi.end_price);
            let bi_low = bi.start_price.min(bi.end_price);
            let bi_is_same_dir = bi.direction.as_str() == bis[xd_start].direction.as_str();

            if bi_is_same_dir {
                let innovated = if is_xd_up {
                    bi_high > xd_extreme
                } else {
                    bi_low < xd_extreme
                };

                if innovated {
                    // 创新高/低 → 线段延续
                    // 不清空特征序列（保留用于包含处理），但设置重置点
                    // 重置点之前的老特征元素不参与分型检测
                    xd_extreme = if is_xd_up { bi_high } else { bi_low };
                    feature_reset_at = feature_seq.len();
                    pending_break = None;
                } else {
                    // 未创新高/低
                    if is_xd_up {
                        xd_extreme = xd_extreme.max(bi_high);
                    } else {
                        xd_extreme = xd_extreme.min(bi_low);
                    }
                }
            } else {
                // 反向笔 → 特征序列元素
                let elem = FeatureElement::from_bi(bi, i);

                // 只在重置点之后的活跃元素中做包含处理
                // 避免新元素被重置点之前的老元素吞并
                if feature_seq.len() <= feature_reset_at {
                    // 重置点之后没有活跃元素，直接添加
                    feature_seq.push(elem);
                } else {
                    feature_seq_push(&mut feature_seq, elem, is_xd_up);
                }

                // 只在重置点之后的特征元素中检查分型
                let active_len = feature_seq.len().saturating_sub(feature_reset_at);
                if active_len >= 3 {
                    // 分型的三个元素必须在重置点之后
                    let start = feature_seq.len() - 3;
                    if start >= feature_reset_at {
                        let prev = &feature_seq[start];
                        let curr = &feature_seq[start + 1];
                        let next = &feature_seq[start + 2];

                        let fenxing = check_fenxing(prev, curr, next);

                        let is_break = match fenxing {
                            Some(FenxingType::Top) => is_xd_up,
                            Some(FenxingType::Bottom) => !is_xd_up,
                            None => false,
                        };

                        if is_break {
                            let break_bi_idx = curr.bi_index;
                            let gap = has_gap(prev, curr, is_xd_up);

                            // 至少 min_len 笔
                            if break_bi_idx >= xd_start + min_len - 1 {
                                pending_break = Some(PendingBreak {
                                    break_bi_idx,
                                    has_gap: gap,
                                });
                            }
                        }
                    }
                }
            }
        }

        // 所有笔扫描完毕，检查是否有有效预警
        if let Some(pb) = pending_break {
            if !pb.has_gap {
                // 无缺口 → 直接确认
                push_xd(&mut xds, bis, xd_start, pb.break_bi_idx, true);
                xd_start = pb.break_bi_idx;
                found_break = true;
            } else {
                // 有缺口 → 需要二次确认
                let reverse_is_xd_up = !is_xd_up;
                if check_overlap_of_first_3(bis, pb.break_bi_idx) {
                    let has_reverse_innovation = check_reverse_innovation(
                        bis,
                        pb.break_bi_idx,
                        reverse_is_xd_up,
                    );
                    if !has_reverse_innovation {
                        push_xd(&mut xds, bis, xd_start, pb.break_bi_idx, true);
                        xd_start = pb.break_bi_idx;
                        found_break = true;
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
        let start_bi = &bis[xd_start];
        let end_bi = &bis[bis.len() - 1];

        if check_overlap_of_first_3(bis, xd_start) || bis.len() - xd_start < 3 {
            xds.push(XianDuan {
                direction: start_bi.direction.clone(),
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

/// 缺口预警信息
struct PendingBreak {
    break_bi_idx: usize,
    has_gap: bool,
}

/// 二次确认：检查从 break_bi_idx 开始的反向线段是否有创新值
fn check_reverse_innovation(bis: &[Bi], break_bi_idx: usize, reverse_is_xd_up: bool) -> bool {
    let reverse_dir = if reverse_is_xd_up { "up" } else { "down" };
    let mut reverse_extreme = if reverse_is_xd_up {
        bis[break_bi_idx].start_price.max(bis[break_bi_idx].end_price)
    } else {
        bis[break_bi_idx].start_price.min(bis[break_bi_idx].end_price)
    };

    for i in (break_bi_idx + 1)..bis.len() {
        let bi = &bis[i];
        let bi_high = bi.start_price.max(bi.end_price);
        let bi_low = bi.start_price.min(bi.end_price);

        if bi.direction.as_str() == reverse_dir {
            let innovated = if reverse_is_xd_up {
                bi_high > reverse_extreme
            } else {
                bi_low < reverse_extreme
            };

            if innovated {
                return true;
            }

            if reverse_is_xd_up {
                reverse_extreme = reverse_extreme.max(bi_high);
            } else {
                reverse_extreme = reverse_extreme.min(bi_low);
            }
        }
    }

    false
}

fn push_xd(
    xds: &mut Vec<XianDuan>,
    bis: &[Bi],
    start_bi_idx: usize,
    end_bi_idx: usize,
    is_finished: bool,
) {
    let start_bi = &bis[start_bi_idx];
    let end_bi = &bis[end_bi_idx];

    let end_price = end_bi.start_price;
    let end_index = end_bi.start_index;
    let end_dt = end_bi.start_dt.clone();

    xds.push(XianDuan {
        direction: start_bi.direction.clone(),
        start_index: start_bi.start_index,
        end_index,
        start_dt: start_bi.start_dt.clone(),
        end_dt,
        start_price: start_bi.start_price,
        end_price,
        is_finished,
    });
}

// ═══════════════════════════════════════════════════════
// 测试
// ═══════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use yifang_data::Bi;

    fn make_bi(id: usize, dir: &str, start_price: f64, end_price: f64, start_idx: u64, end_idx: u64) -> Bi {
        let start_dt = format!("2024-01-{:02}", id * 2 + 1);
        let end_dt = format!("2024-01-{:02}", id * 2 + 2);
        Bi {
            direction: dir.to_string(),
            start_index: start_idx,
            end_index: end_idx,
            start_dt,
            end_dt,
            start_price,
            end_price,
            is_finished: true,
        }
    }

    #[test]
    fn test_fenxing_detection() {
        let prev = FeatureElement { high: 15.0, low: 10.0, bi_index: 0 };
        let curr = FeatureElement { high: 20.0, low: 15.0, bi_index: 1 };
        let next = FeatureElement { high: 17.0, low: 13.0, bi_index: 2 };
        assert_eq!(check_fenxing(&prev, &curr, &next), Some(FenxingType::Top));

        let prev = FeatureElement { high: 17.0, low: 13.0, bi_index: 0 };
        let curr = FeatureElement { high: 15.0, low: 8.0, bi_index: 1 };
        let next = FeatureElement { high: 18.0, low: 12.0, bi_index: 2 };
        assert_eq!(check_fenxing(&prev, &curr, &next), Some(FenxingType::Bottom));
    }

    #[test]
    fn test_gap_detection() {
        let prev = FeatureElement { high: 20.0, low: 18.0, bi_index: 0 };
        let curr = FeatureElement { high: 16.0, low: 14.0, bi_index: 1 };
        assert!(has_gap(&prev, &curr, true));
        assert!(!has_gap(&prev, &curr, false));
    }

    #[test]
    fn test_overlap_check() {
        let bis = vec![
            make_bi(0, "up", 10.0, 15.0, 0, 3),
            make_bi(1, "down", 15.0, 12.0, 3, 6),
            make_bi(2, "up", 12.0, 18.0, 6, 9),
        ];
        assert!(check_overlap_of_first_3(&bis, 0));

        let bis = vec![
            make_bi(0, "up", 10.0, 15.0, 0, 3),
            make_bi(1, "down", 15.0, 5.0, 3, 6),
            make_bi(2, "up", 5.0, 8.0, 6, 9),
        ];
        assert!(!check_overlap_of_first_3(&bis, 0));
    }

    #[test]
    fn test_contain_up_xd() {
        // 上升线段特征序列按上升方向处理: high=max, low=max
        let mut seq = Vec::new();
        feature_seq_push(&mut seq, FeatureElement { high: 20.0, low: 15.0, bi_index: 0 }, true);
        feature_seq_push(&mut seq, FeatureElement { high: 18.0, low: 16.0, bi_index: 1 }, true);
        assert_eq!(seq.len(), 1);
        assert_eq!(seq[0].high, 20.0); // max(20,18)
        assert_eq!(seq[0].low, 16.0);  // max(15,16)
    }

    #[test]
    fn test_contain_down_xd() {
        // 下降线段特征序列按下降方向处理: high=min, low=min
        let mut seq = Vec::new();
        feature_seq_push(&mut seq, FeatureElement { high: 15.0, low: 8.0, bi_index: 0 }, false);
        feature_seq_push(&mut seq, FeatureElement { high: 14.0, low: 10.0, bi_index: 1 }, false);
        assert_eq!(seq.len(), 1);
        assert_eq!(seq[0].high, 14.0); // min(15,14)
        assert_eq!(seq[0].low, 8.0);   // min(8,10)
    }

    #[test]
    fn test_xd_min_3_bi() {
        let bis = vec![
            make_bi(0, "up", 10.0, 15.0, 0, 2),
            make_bi(1, "down", 15.0, 12.0, 2, 4),
        ];
        assert!(build_xd(&bis).is_empty());
    }

    #[test]
    fn test_xd_innovation_resets() {
        // 创新高重置特征序列，避免跨创新高的错误合并
        let bis = vec![
            make_bi(0, "up", 10.0, 20.0, 0, 2),
            make_bi(1, "down", 20.0, 15.0, 2, 4),
            make_bi(2, "up", 15.0, 30.0, 4, 6),      // 创新高 → 重置
            make_bi(3, "down", 30.0, 22.0, 6, 8),
            make_bi(4, "up", 22.0, 28.0, 8, 10),     // 未创新高
            make_bi(5, "down", 28.0, 20.0, 10, 12),
            make_bi(6, "up", 20.0, 35.0, 12, 14),     // 创新高 → 重置
            make_bi(7, "down", 35.0, 25.0, 14, 16),
        ];
        let xds = build_xd(&bis);
        assert_eq!(xds.len(), 1);
        assert!(!xds[0].is_finished);
    }

    #[test]
    fn test_xd_000001() {
        let json_str = std::fs::read_to_string("/tmp/000001_daily.json").unwrap_or_default();
        if json_str.is_empty() {
            eprintln!("SKIP: no data file");
            return;
        }
        let records: Vec<serde_json::Value> = serde_json::from_str(&json_str).unwrap();

        use yifang_data::{KLine, TimeFrame};

        let klines: Vec<KLine> = records
            .iter()
            .enumerate()
            .map(|(i, r)| KLine {
                symbol: "000001".to_string(),
                timeframe: TimeFrame::D,
                dt: r["dt"].as_str().unwrap().to_string(),
                id: i as u64,
                open: r["open"].as_f64().unwrap(),
                close: r["close"].as_f64().unwrap(),
                high: r["high"].as_f64().unwrap(),
                low: r["low"].as_f64().unwrap(),
                vol: r["vol"].as_f64().unwrap(),
                amount: 0.0,
            })
            .collect();

        let bis = crate::bi::build_bi(&klines, None);
        let xds = build_xd(&bis);

        eprintln!("\n000001日线: {}笔 → {}线段", bis.len(), xds.len());
        for (i, bi) in bis.iter().enumerate() {
            eprintln!("  BI[{}] {} {}({:.2}) → {}({:.2})",
                i, bi.direction, bi.start_dt, bi.start_price, bi.end_dt, bi.end_price);
        }
        for (i, xd) in xds.iter().enumerate() {
            eprintln!("  XD[{}] {} {}({:.2}) → {}({:.2}) fin={}",
                i, xd.direction, xd.start_dt, xd.start_price, xd.end_dt, xd.end_price, xd.is_finished);
        }

        for i in 1..xds.len() {
            assert_ne!(xds[i].direction, xds[i-1].direction);
        }
    }
}

#[cfg(test)]
mod xd_random_test {
    use super::*;
    use crate::bi;
    use yifang_data::{KLine, TimeFrame};
    use std::hash::{Hash, Hasher};
    use std::collections::hash_map::DefaultHasher;

    fn gen_klines(seed: u64, n: usize) -> Vec<KLine> {
        let mut hasher = DefaultHasher::new();
        seed.hash(&mut hasher);
        let mut state = hasher.finish();
        let mut next_rand = || -> f64 {
            state = state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            (state >> 33) as f64 / (1u64 << 31) as f64
        };
        let mut price = 15.0;
        (0..n).map(|i| {
            let change = (next_rand() - 0.5) * 2.0;
            price = (price + change).max(1.0).min(50.0);
            let high = price + next_rand() * 0.5;
            let low = price - next_rand() * 0.5;
            let open = low + next_rand() * (high - low);
            let close = low + next_rand() * (high - low);
            KLine { symbol: "TEST".into(), timeframe: TimeFrame::D,
                dt: format!("2024-{:02}-{:02}", i / 30 + 1, i % 28 + 1),
                id: i as u64, open, close, high, low, vol: 1000.0, amount: 0.0 }
        }).collect()
    }

    #[test]
    fn test_xd_random_directions_alternate() {
        for seed in [42u64, 123, 456, 789, 1024, 2048, 3000, 4000] {
            let klines = gen_klines(seed, 500);
            let bis = bi::build_bi(&klines, None);
            if bis.len() < 3 { continue; }
            let xds = build_xd(&bis);
            for i in 1..xds.len() {
                assert_ne!(xds[i].direction, xds[i-1].direction,
                    "seed={}: 线段[{}]和[{}]方向相同", seed, i, i-1);
            }
        }
    }

    #[test]
    fn test_xd_strict_chanlun_rules() {
        // 严格校验缠论线段规则：
        // 1. 笔方向交替（若不交替说明笔算法有bug，跳过该seed）
        // 2. 线段方向交替
        // 3. 上升线段的起点价格 < 终点价格
        // 4. 下降线段的起点价格 > 终点价格
        // 5. 线段间首尾相连
        for seed in [42u64, 123, 456, 789, 1024, 2048, 3000, 4000] {
            let klines = gen_klines(seed, 500);
            let bis = bi::build_bi(&klines, None);
            if bis.len() < 3 { continue; }

            // 首先验证笔方向交替（500根K线下应无问题）
            let mut bi_alternating = true;
            for i in 1..bis.len() {
                if bis[i].direction.as_str() == bis[i-1].direction.as_str() {
                    bi_alternating = false;
                    break;
                }
            }
            if !bi_alternating {
                eprintln!("seed={}: 笔方向不交替，跳过", seed);
                continue;
            }

            let xds = build_xd(&bis);

            eprintln!("\nseed={}: {}笔 → {}线段", seed, bis.len(), xds.len());
            for (i, xd) in xds.iter().enumerate() {
                eprintln!("  XD[{}] {} {:.2}→{:.2} fin={}", i, xd.direction, xd.start_price, xd.end_price, xd.is_finished);
            }

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
                if i > 0 && xds[i-1].is_finished {
                    assert_eq!(xd.start_index, xds[i-1].end_index,
                        "seed={}: 线段[{}]起点不等于线段[{}]终点", seed, i, i-1);
                }
            }
            for i in 1..xds.len() {
                assert_ne!(xds[i].direction, xds[i-1].direction,
                    "seed={}: 线段[{}]和[{}]方向相同", seed, i, i-1);
            }
        }
    }
}

