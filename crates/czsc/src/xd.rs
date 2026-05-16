//! 线段分析 — 严格按缠论原文定义（第62、65、67、71课）
//!
//! ## 线段定义
//! 线段 = 至少3笔 + 前三笔重叠 + 方向交替 + 首尾同方向
//!
//! ## 特征序列
//! 上升线段的特征序列 = 所有下降笔
//! 下降线段的特征序列 = 所有上升笔
//!
//! ## 特征序列包含处理（缠论第71课）
//! "线段是向上的，特征序列也是向上的"
//! 包含关系：一根K线完全包含另一根（A.high >= B.high 且 A.low <= B.low，或反过来）
//! 包含方向 = 线段方向：
//! - 上升线段特征序列按上升方向处理 → high=max, low=max, bi_index=高点所在笔
//! - 下降线段特征序列按下降方向处理 → high=min, low=min, bi_index=低点所在笔
//!
//! ## 线段终结
//! - 无缺口：直接确认终结
//! - 有缺口：需要二次确认（反向线段前三笔重叠 + 无反向创新高/低）
//!
//! ## 线段终点确定
//! 分型中间元素的 bi_index 对应特征笔（反向笔）
//! 线段终点 = bi_index - 1（该特征笔前面的同向笔）
//! 终点价格 = 分型极值（顶分型=curr.high, 底分型=curr.low）

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
/// 包含关系：一根K线完全包含另一根
/// A包含B: A.high >= B.high 且 A.low <= B.low
/// B包含A: B.high >= A.high 且 B.low <= A.low
///
/// 包含处理方向 = 线段方向（缠论第71课）：
/// "线段是向上的，特征序列也是向上的"
/// - 上升线段：high=max, low=max, bi_index=高点所在笔
/// - 下降线段：high=min, low=min, bi_index=低点所在笔
fn feature_seq_push(feature_seq: &mut Vec<FeatureElement>, elem: FeatureElement, is_xd_up: bool) {
    if feature_seq.is_empty() {
        feature_seq.push(elem);
        return;
    }

    let last = feature_seq.last().unwrap();
    let a_contains_b = last.high >= elem.high && last.low <= elem.low;
    let b_contains_a = elem.high >= last.high && elem.low <= last.low;

    if a_contains_b || b_contains_a {
        let last = feature_seq.last_mut().unwrap();
        if is_xd_up {
            // 上升方向: high=max, low=max, bi_index=高点所在笔
            let keep_last_idx = last.high >= elem.high;
            last.high = last.high.max(elem.high);
            last.low = last.low.max(elem.low);
            if !keep_last_idx {
                last.bi_index = elem.bi_index;
            }
        } else {
            // 下降方向: high=min, low=min, bi_index=低点所在笔
            let keep_last_idx = last.low <= elem.low;
            last.high = last.high.min(elem.high);
            last.low = last.low.min(elem.low);
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

fn build_xd_incremental(bis: &[Bi], min_len: usize) -> Vec<XianDuan> {
    if bis.len() < min_len {
        return Vec::new();
    }

    let mut xds: Vec<XianDuan> = Vec::new();
    let mut xd_start: usize = 0;

    while xd_start < bis.len() {
        // 线段方向：由线段起点的特征分型确定
        // 上升线段起点=底分型，下降线段起点=顶分型
        // xd_start 可能指向同向笔或反向笔，方向由线段趋势决定
        // 第一段取 bis[xd_start].direction，后续段取前一段的反方向
        let is_xd_up = if xds.is_empty() {
            bis[xd_start].direction.as_str() == "up"
        } else {
            // 线段间首尾相连，新线段方向 = 前一段相反方向
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
            let xd_dir_str = if is_xd_up { "up" } else { "down" };
            let bi_is_same_dir = bi.direction.as_str() == xd_dir_str;

            if !bi_is_same_dir {
                let elem = FeatureElement::from_bi(bi, i);
                feature_seq_push(&mut feature_seq, elem, is_xd_up);
            }

            // 检测分型（只在最后3个特征元素上检测）
            if feature_seq.len() >= 3 {
                let prev = &feature_seq[feature_seq.len() - 3];
                let curr = &feature_seq[feature_seq.len() - 2];
                let next = &feature_seq[feature_seq.len() - 1];

                let fenxing = check_fenxing(prev, curr, next);

                let is_break = match fenxing {
                    Some(FenxingType::Top) => is_xd_up,
                    Some(FenxingType::Bottom) => !is_xd_up,
                    None => false,
                };

                if is_break {
                    let break_bi_idx = curr.bi_index; // 分型极值所在的特征笔（反向笔）
                    let end_bi_idx = break_bi_idx - 1; // 线段终点 = 特征笔前面的同向笔
                    let gap = has_gap(prev, curr, is_xd_up);

                    // 至少 min_len 笔
                    if end_bi_idx - xd_start + 1 >= min_len {
                        if !gap {
                            push_xd(&mut xds, bis, xd_start, end_bi_idx, curr.high, curr.low, is_xd_up, true);
                            // 新线段从终点笔开始（线段首尾相连）
                            xd_start = end_bi_idx;
                            found_break = true;
                            break;
                        } else {
                            let reverse_is_xd_up = !is_xd_up;
                            if check_overlap_of_first_3(bis, break_bi_idx) {
                                let has_reverse_innovation = check_reverse_innovation(
                                    bis,
                                    break_bi_idx,
                                    reverse_is_xd_up,
                                );
                                if !has_reverse_innovation {
                                    push_xd(&mut xds, bis, xd_start, end_bi_idx, curr.high, curr.low, is_xd_up, true);
                                    xd_start = end_bi_idx;
                                    found_break = true;
                                    break;
                                }
                            }
                        }
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

        let is_xd_up = if xds.is_empty() {
            start_bi.direction.as_str() == "up"
        } else {
            xds.last().unwrap().direction.as_str() != "up"
        };

        if check_overlap_of_first_3(bis, xd_start) || bis.len() - xd_start < 3 {
            let mut extreme_price = if is_xd_up {
                start_bi.start_price.max(start_bi.end_price)
            } else {
                start_bi.start_price.min(start_bi.end_price)
            };
            for i in xd_start..bis.len() {
                if is_xd_up {
                    extreme_price = extreme_price.max(bis[i].start_price).max(bis[i].end_price);
                } else {
                    extreme_price = extreme_price.min(bis[i].start_price).min(bis[i].end_price);
                }
            }

            let direction = if is_xd_up { "up" } else { "down" }.to_string();
            xds.push(XianDuan {
                direction,
                start_index: start_bi.start_index,
                end_index: end_bi.end_index,
                start_dt: start_bi.start_dt.clone(),
                end_dt: end_bi.end_dt.clone(),
                start_price: start_bi.start_price,
                end_price: extreme_price,
                is_finished: false,
            });
        }
    }

    xds
}

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

/// 添加一个完成的线段到结果中
fn push_xd(
    xds: &mut Vec<XianDuan>,
    bis: &[Bi],
    start_bi_idx: usize,
    end_bi_idx: usize,
    fenxing_high: f64,
    fenxing_low: f64,
    is_xd_up: bool,
    is_finished: bool,
) {
    let start_bi = &bis[start_bi_idx];
    let end_bi = &bis[end_bi_idx];

    // 计算线段内的极值价格
    let mut max_price = f64::MIN;
    let mut min_price = f64::MAX;
    for i in start_bi_idx..=end_bi_idx {
        max_price = max_price.max(bis[i].start_price).max(bis[i].end_price);
        min_price = min_price.min(bis[i].start_price).min(bis[i].end_price);
    }

    let (start_price, end_price) = if is_xd_up {
        (min_price, fenxing_high) // 上升：低→高
    } else {
        (max_price, fenxing_low)  // 下降：高→低
    };

    let direction = if is_xd_up { "up" } else { "down" }.to_string();

    xds.push(XianDuan {
        direction,
        start_index: start_bi.start_index,
        end_index: end_bi.end_index,
        start_dt: start_bi.start_dt.clone(),
        end_dt: end_bi.end_dt.clone(),
        start_price,
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
            start_price,
            end_price,
            start_dt,
            end_dt,
            start_index: start_idx,
            end_index: end_idx,
            is_finished: true,
        }
    }

    fn gen_klines(seed: u64, n: usize) -> Vec<yifang_data::KLine> {
        use yifang_data::{KLine, TimeFrame};
        let mut state = seed;
        let mut price = 15.0;
        let mut klines = Vec::new();
        for i in 0..n {
            state = state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            let r = ((state >> 33) as f64) / (1u64 << 31) as f64;
            let change = (r - 0.5) * 2.0;
            price = (price + change).max(1.0).min(50.0);
            let high = price + r * 0.5;
            let r2 = ((state.wrapping_add(1) >> 33) as f64) / (1u64 << 31) as f64;
            let low = price - r2 * 0.5;
            let open = low + r2 * (high - low);
            let r3 = ((state.wrapping_add(2) >> 33) as f64) / (1u64 << 31) as f64;
            let close = low + r3 * (high - low);
            klines.push(KLine {
                symbol: "TEST".to_string(),
                timeframe: TimeFrame::D,
                dt: format!("2024-{:02}-{:02}", i / 28 + 1, i % 28 + 1),
                id: i as u64,
                open, close, high, low,
                vol: 1000.0,
                amount: 0.0,
            });
        }
        klines
    }

    #[test]
    fn test_fenxing_detection() {
        let prev = FeatureElement { high: 10.0, low: 8.0, bi_index: 1 };
        let curr = FeatureElement { high: 12.0, low: 9.0, bi_index: 3 };
        let next = FeatureElement { high: 11.0, low: 7.0, bi_index: 5 };
        assert_eq!(check_fenxing(&prev, &curr, &next), Some(FenxingType::Top));

        let p = FeatureElement { high: 11.0, low: 7.0, bi_index: 5 };
        let c = FeatureElement { high: 12.0, low: 6.0, bi_index: 3 };
        let n = FeatureElement { high: 13.0, low: 8.0, bi_index: 1 };
        assert_eq!(check_fenxing(&p, &c, &n), Some(FenxingType::Bottom));
    }

    #[test]
    fn test_gap_detection() {
        let prev = FeatureElement { high: 10.0, low: 8.0, bi_index: 1 };
        let curr = FeatureElement { high: 7.5, low: 5.0, bi_index: 3 };
        assert!(has_gap(&prev, &curr, true));
        assert!(!has_gap(&prev, &curr, false));
    }

    #[test]
    fn test_overlap_check() {
        let bis = vec![
            make_bi(0, "up", 10.0, 12.0, 0, 2),
            make_bi(1, "down", 12.0, 9.0, 2, 4),
            make_bi(2, "up", 9.0, 11.0, 4, 6),
        ];
        assert!(check_overlap_of_first_3(&bis, 0));
    }

    #[test]
    fn test_contain_up_xd() {
        let mut seq = Vec::new();
        // A(10, 5) + B(8, 6): A包含B（10>=8 且 5<=6）
        feature_seq_push(&mut seq, FeatureElement { high: 10.0, low: 5.0, bi_index: 1 }, true);
        feature_seq_push(&mut seq, FeatureElement { high: 8.0, low: 6.0, bi_index: 3 }, true);
        assert_eq!(seq.len(), 1);
        assert_eq!(seq[0].high, 10.0); // max(10,8)=10
        assert_eq!(seq[0].low, 6.0);   // max(5,6)=6
        assert_eq!(seq[0].bi_index, 1); // 高点10在BI[1]
    }

    #[test]
    fn test_contain_down_xd() {
        let mut seq = Vec::new();
        // A(12, 6) + B(10, 8): A包含B（12>=10 且 6<=8）
        feature_seq_push(&mut seq, FeatureElement { high: 12.0, low: 6.0, bi_index: 1 }, false);
        feature_seq_push(&mut seq, FeatureElement { high: 10.0, low: 8.0, bi_index: 3 }, false);
        assert_eq!(seq.len(), 1);
        assert_eq!(seq[0].high, 10.0); // min(12,10)=10
        assert_eq!(seq[0].low, 6.0);   // min(6,8)=6
        assert_eq!(seq[0].bi_index, 1); // 低点6在BI[1]
    }

    #[test]
    fn test_xd_min_3_bi() {
        let bis = vec![
            make_bi(0, "up", 10.0, 15.0, 0, 2),
            make_bi(1, "down", 15.0, 12.0, 2, 4),
        ];
        let xds = build_xd(&bis);
        assert!(xds.is_empty() || !xds[0].is_finished);
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

        // 验证前两个线段
        assert!(xds.len() >= 2, "应有至少2个线段");
        assert_eq!(xds[0].direction.as_str(), "up");
        assert!(xds[0].is_finished);
        assert_eq!(xds[1].direction.as_str(), "down");
        assert!(xds[1].is_finished);

        // 线段方向交替
        for i in 1..xds.len() {
            assert_ne!(xds[i].direction, xds[i-1].direction);
        }
    }

    #[test]
    fn test_xd_random_directions_alternate() {
        for seed in [42u64, 123, 456, 789, 1024] {
            let klines = gen_klines(seed, 500);
            let bis = crate::bi::build_bi(&klines, None);
            if bis.len() < 3 { continue; }

            let mut bi_ok = true;
            for i in 1..bis.len() {
                if bis[i].direction.as_str() == bis[i-1].direction.as_str() {
                    bi_ok = false;
                    break;
                }
            }
            if !bi_ok { continue; }

            let xds = build_xd(&bis);
            for i in 1..xds.len() {
                assert_ne!(xds[i].direction, xds[i-1].direction,
                    "seed={}: 线段[{}]和[{}]方向相同", seed, i, i-1);
            }
        }
    }

    #[test]
    fn test_xd_strict_chanlun_rules() {
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
            }
            for i in 1..xds.len() {
                assert_ne!(xds[i].direction, xds[i-1].direction,
                    "seed={}: 线段[{}]和[{}]方向相同", seed, i, i-1);
            }
        }
    }
}
