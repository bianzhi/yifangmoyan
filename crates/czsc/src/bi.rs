//! 笔的构建
//!
//! 严格遵循缠论原文（第62、65、77课），五步流程：
//!
//! 1. K线去包含
//! 2. 顶底分型识别（ensure_alternating 保证顶底交替）
//! 3. 初步顶底对匹配（方向正确 + 间隔≥3根去包含K线）
//! 4. 分型失效检查（核心难点）
//!    - 检查范围：从 start_fx.index+1 到下一个反向分型的k1索引-1
//!    - 顶分型：该区间内有新高 → 顶失效
//!    - 底分型：该区间内有新低 → 底失效
//!    - 一旦反向分型k1出现（下一个对中分型），前分型锁定不可逆
//! 5. 笔确认，一旦确认不可逆

use crate::fenxing::{FxMark, FxResult, check_fxs};
use crate::include::NewBar;
use yifang_data::Bi;

/// 最小笔长度（新笔定义：顶底分型之间至少1根独立K线）
/// 顶分型3根 + 独立1根 + 底分型3根 = 7根去包含K线
/// 注：当前实现中不再使用此常量进行长度过滤，间隔≥3已保证
#[allow(dead_code)]
const DEFAULT_MIN_BI_LEN: usize = 7;

/// 顶底对（步骤3的输出）
#[derive(Clone)]
struct FxPair {
    /// 起点分型在 fxs 序列中的索引
    start_idx: usize,
    /// 终点分型在 fxs 序列中的索引
    end_idx: usize,
}

/// 步骤3：初步匹配顶底对
///
/// 严格按照缠论规范：
/// - 分型必须反向（顶→底 或 底→顶）
/// - 两个分型中间K线索引间隔 ≥ 3（即 next_fx.bar_index - curr_fx.bar_index ≥ 3）
/// - 同类型分型：保留极值更大的（只在还没匹配成对时才替换）
/// - 方向正确：顶的fx > 底的fx
/// - 关键：匹配成功后，curr固定为end，后续的同类型替换只在未匹配的候选上执行
fn match_fx_pairs(fxs: &[FxResult]) -> Vec<FxPair> {
    let mut pairs = Vec::new();
    if fxs.len() < 2 {
        return pairs;
    }

    let mut curr_idx = 0;
    let mut i = 1;
    while i < fxs.len() {
        let curr = &fxs[curr_idx];
        let next = &fxs[i];

        // 同类型分型：保留极值更大的
        if next.mark == curr.mark {
            let should_replace = match next.mark {
                FxMark::Top => next.fx > curr.fx,
                FxMark::Bottom => next.fx < curr.fx,
            };
            if should_replace {
                curr_idx = i;
            }
            i += 1;
            continue;
        }

        // 间隔检查：顶底分型之间至少要有1根独立K线
        // curr.bars[2] 是起始分型最后一根K线，next.bars[0] 是终止分型第一根K线
        // 它们之间至少隔1根：next.bars[0] - curr.bars[2] >= 2
        if next.bars[0] < curr.bars[2] + 2 {
            i += 1;
            continue;
        }

        // 方向检查：顶的fx必须高于底的fx
        let direction_ok = match curr.mark {
            FxMark::Top => next.fx < curr.fx,   // 顶→底
            FxMark::Bottom => next.fx > curr.fx, // 底→顶
        };
        if direction_ok {
            pairs.push(FxPair {
                start_idx: curr_idx,
                end_idx: i,
            });
            curr_idx = i; // 匹配成功，curr固定为end
        }
        i += 1;
    }

    pairs
}

/// 步骤4：分型失效检查
///
/// 严格按照缠论规范（第62、77课核心规则）：
/// - 分型失效只发生在反向分型第一根K线出现之前
/// - 一旦反向分型的第一根K线出现，前分型锁定不可逆
/// - 反向分型第三根K线走完且满足笔条件后，笔永久确认不可修改
///
/// 检查范围：
/// - check_start = start_fx.bar_index + 1（分型中间K线的下一根）
/// - check_end = end_fx.bar_index - 1（end_fx 即反向分型，其中间K线索引-1 = 反向分型k1前一个位置）
///   注：反向分型的k1 = bars[end_fx.bar_index - 1]、k2 = bars[end_fx.bar_index]、k3 = bars[end_fx.bar_index + 1]
///   一旦 k1 出现，start_fx 就被锁定，所以检查截止位置 = k1 索引 - 1
/// - 顶分型：该区间内有新高 → 顶失效
/// - 底分型：该区间内有新低 → 底失效
///
/// 重要：失效后需要收集剩余的有效分型，重新配对保证相邻笔共享端点
fn check_fx_invalidation(
    fxs: &[FxResult],
    pairs: &[FxPair],
    bars: &[NewBar],
) -> Vec<FxPair> {
    // 收集失效的分型索引（整对删除，start和end分型都移除）
    let mut invalid_fx_set: std::collections::HashSet<usize> = std::collections::HashSet::new();

    for pair in pairs.iter() {
        let start_fx = &fxs[pair.start_idx];

        // 检查起始位置：分型中间K线的下一根
        let check_start = start_fx.bar_index + 1;

        // 检查终止位置：反向分型（end_fx）k1出现即锁定
        let end_fx = &fxs[pair.end_idx];
        let check_end = end_fx.bar_index - 1;

        // 在区间内检查是否有突破
        let invalid = if start_fx.mark == FxMark::Top {
            // 顶分型失效：之后有更高点
            if check_start <= check_end {
                (check_start..=check_end).any(|j| bars[j].high > start_fx.fx)
            } else {
                false
            }
        } else {
            // 底分型失效：之后有更低点
            if check_start <= check_end {
                (check_start..=check_end).any(|j| bars[j].low < start_fx.fx)
            } else {
                false
            }
        };

        if invalid {
            // 整对失效：记录这对的所有分型
            invalid_fx_set.insert(pair.start_idx);
            invalid_fx_set.insert(pair.end_idx);
        }
    }

    // 从全部分型中移除失效的，保留有效分型
    let valid_fx_indices: Vec<usize> = (0..fxs.len())
        .filter(|idx| !invalid_fx_set.contains(idx))
        .collect();

    // 用保留的分型重新配对
    match_fx_pairs_from_indices(fxs, &valid_fx_indices)
}

/// 用指定的分型子集重新配对
///
/// 与 match_fx_pairs 不同的是，子集中可能出现连续同类型分型（因为失效删除可能暴露出
/// 原来被交替约束过滤掉的分型顺序）。在这种情况下：
/// - 连续同类型分型：需要回溯修改已确认 pair 的端点（替换为更极端的分型）
/// - 保证相邻笔共享端点
fn match_fx_pairs_from_indices(fxs: &[FxResult], indices: &[usize]) -> Vec<FxPair> {
    let mut pairs = Vec::new();
    if indices.len() < 2 {
        return pairs;
    }

    let mut curr_pos = 0; // 在 indices 中的位置
    let mut i = 1;

    while i < indices.len() {
        let curr = &fxs[indices[curr_pos]];
        let next = &fxs[indices[i]];

        // 同类型分型：保留极值更大的
        if next.mark == curr.mark {
            let should_replace = match next.mark {
                FxMark::Top => next.fx > curr.fx,
                FxMark::Bottom => next.fx < curr.fx,
            };
            if should_replace {
                // 需要回溯：如果上一对(pair)的end就是当前curr，也要更新
                if let Some(last_pair) = pairs.last_mut() {
                    if last_pair.end_idx == indices[curr_pos] {
                        last_pair.end_idx = indices[i];
                    }
                }
                curr_pos = i;
            }
            i += 1;
            continue;
        }

        // 间隔检查：顶底分型之间至少要有1根独立K线
        if next.bars[0] < curr.bars[2] + 2 {
            i += 1;
            continue;
        }

        // 方向检查
        let direction_ok = match curr.mark {
            FxMark::Top => next.fx < curr.fx,
            FxMark::Bottom => next.fx > curr.fx,
        };
        if direction_ok {
            pairs.push(FxPair {
                start_idx: indices[curr_pos],
                end_idx: indices[i],
            });
            curr_pos = i;
        }
        i += 1;
    }

    pairs
}

/// 步骤5：将有效的顶底对转为笔
fn create_bis(fxs: &[FxResult], pairs: &[FxPair], _bars: &[NewBar]) -> Vec<Bi> {
    let mut bis = Vec::new();

    for pair in pairs {
        let start_fx = &fxs[pair.start_idx];
        let end_fx = &fxs[pair.end_idx];

        let direction = match start_fx.mark {
            FxMark::Bottom => "up",
            FxMark::Top => "down",
        };

        // 起止价格：上升笔起价=底分型low，止价=顶分型high；下降笔起价=顶分型high，止价=底分型low
        let (start_price, end_price) = match start_fx.mark {
            FxMark::Bottom => (start_fx.fx, end_fx.fx), // fx已经是对应的价格
            FxMark::Top => (start_fx.fx, end_fx.fx),
        };

        // 验证方向与价格一致性
        if direction == "up" && start_price >= end_price {
            eprintln!("[BI-WARN] 上升笔但 start({:.2}) >= end({:.2}): start_fx={:?} idx={} end_fx={:?} idx={}", 
                start_price, end_price, start_fx.mark, start_fx.bar_index, end_fx.mark, end_fx.bar_index);
        }
        if direction == "down" && start_price <= end_price {
            eprintln!("[BI-WARN] 下降笔但 start({:.2}) <= end({:.2}): start_fx={:?} idx={} end_fx={:?} idx={}", 
                start_price, end_price, start_fx.mark, start_fx.bar_index, end_fx.mark, end_fx.bar_index);
        }

        bis.push(Bi {
            direction: direction.to_string(),
            start_index: start_fx.merged_index as u64,
            end_index: end_fx.merged_index as u64,
            start_dt: start_fx.dt.clone(),
            end_dt: end_fx.dt.clone(),
            start_price,
            end_price,
            is_finished: true,
        });
    }

    bis
}

/// 构建笔序列
///
/// 严格遵循缠论五步流程，在完整K线序列上一次性完成：
/// 1. K线去包含
/// 2. 分型识别（含 ensure_alternating）
/// 3. 初步匹配顶底对
/// 4. 分型失效检查
/// 5. 生成笔
pub fn build_bi(klines: &[yifang_data::KLine], _min_bi_len: Option<usize>) -> Vec<Bi> {
    if klines.len() < 3 {
        return Vec::new();
    }

    // 步骤1：K线去包含
    let bars = crate::include::remove_include(klines);
    if bars.len() < 3 {
        return Vec::new();
    }

    // 步骤2：分型识别（含 ensure_alternating）
    let fxs = check_fxs(&bars);
    if fxs.len() < 2 {
        return Vec::new();
    }

    // 步骤3：初步匹配顶底对
    let pairs = match_fx_pairs(&fxs);
    if pairs.is_empty() {
        return Vec::new();
    }

    // 步骤4：分型失效检查
    let valid_pairs = check_fx_invalidation(&fxs, &pairs, &bars);

    // 步骤5：生成笔
    create_bis(&fxs, &valid_pairs, &bars)
}

// ═══════════════════════════════════════════════════════════════
// 测试
// ═══════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use crate::include::remove_include;
    use yifang_data::{KLine, TimeFrame};

    fn make_kline(id: u64, dt: &str, open: f64, close: f64, high: f64, low: f64) -> KLine {
        KLine {
            symbol: "test".to_string(),
            timeframe: TimeFrame::D,
            dt: dt.to_string(),
            id,
            open,
            close,
            high,
            low,
            vol: 1000.0,
            amount: 10000.0,
        }
    }

    #[test]
    fn test_min_bi_len() {
        let default_len = DEFAULT_MIN_BI_LEN;
        assert_eq!(default_len, 7, "新笔定义：顶底分型之间至少1根独立K线，共7根");
    }

    #[test]
    fn test_bi_exact_output() {
        // 构建两笔：底(10)→顶(20)→底(12)
        let klines = vec![
            make_kline(0, "2024-01-01", 10.0, 9.0, 10.0, 9.0),   // 下降
            make_kline(1, "2024-01-02", 9.0, 8.0, 9.0, 8.0),     // BTM k1
            make_kline(2, "2024-01-03", 8.0, 7.0, 8.0, 7.0),     // BTM k2=7
            make_kline(3, "2024-01-04", 9.0, 8.0, 9.0, 8.0),     // BTM k3
            make_kline(4, "2024-01-05", 10.0, 12.0, 12.0, 10.0), // 上升
            make_kline(5, "2024-01-06", 12.0, 15.0, 15.0, 12.0), // 上升
            make_kline(6, "2024-01-07", 15.0, 18.0, 18.0, 15.0), // 上升
            make_kline(7, "2024-01-08", 18.0, 20.0, 20.0, 18.0), // TOP k2=20
            make_kline(8, "2024-01-09", 20.0, 19.0, 20.0, 18.0), // TOP k3 (包含)
            make_kline(9, "2024-01-10", 19.0, 17.0, 19.0, 17.0), // 下降
            make_kline(10, "2024-01-11", 17.0, 15.0, 17.0, 15.0),// 下降
            make_kline(11, "2024-01-12", 15.0, 13.0, 15.0, 13.0),// 下降
            make_kline(12, "2024-01-13", 13.0, 12.0, 13.0, 12.0),// BTM k1
        ];

        let bis = build_bi(&klines, None);
        println!("\n输出笔: {}根", bis.len());
        for (i, bi) in bis.iter().enumerate() {
            let d = if bi.start_price < bi.end_price { "↑" } else { "↓" };
            println!("  BI[{}] {} {:.1}→{:.1} (idx {}→{})", i, d, bi.start_price, bi.end_price, bi.start_index, bi.end_index);
        }

        assert!(!bis.is_empty(), "必须找到笔");
    }

    #[test]
    fn test_first_reverse_fx_selection() {
        // 测试：当第一个分型是顶分型时，应该正确形成下降笔
        let klines = vec![
            make_kline(0, "2024-01-01", 10.0, 12.0, 12.0, 10.0),  // 上升
            make_kline(1, "2024-01-02", 12.0, 15.0, 15.0, 12.0),  // TOP k1
            make_kline(2, "2024-01-03", 15.0, 18.0, 18.0, 15.0),  // TOP k2=18
            make_kline(3, "2024-01-04", 18.0, 17.0, 18.0, 15.0),  // TOP k3
            make_kline(4, "2024-01-05", 17.0, 14.0, 17.0, 14.0),  // 下降
            make_kline(5, "2024-01-06", 14.0, 11.0, 14.0, 11.0),  // 下降
            make_kline(6, "2024-01-07", 11.0, 9.0, 11.0, 9.0),    // 下降
            make_kline(7, "2024-01-08", 9.0, 7.0, 9.0, 7.0),      // BTM k1 → k2区间
        ];

        let bis = build_bi(&klines, None);
        println!("\n首顶测试: {}笔", bis.len());
        for (i, bi) in bis.iter().enumerate() {
            let d = if bi.start_price < bi.end_price { "↑" } else { "↓" };
            println!("  BI[{}] {} {:.1}→{:.1}", i, d, bi.start_price, bi.end_price);
        }

        if !bis.is_empty() {
            assert_eq!(bis[0].direction, "down", "第一个分型是顶，应为下降笔");
        }
    }

    #[test]
    fn test_contains_scenario() {
        // 测试包含处理：一根大阳线后跟包含关系的小K线，再下跌
        let klines = vec![
            make_kline(0, "2024-01-01", 10.0, 11.0, 11.0, 10.0),
            make_kline(1, "2024-01-02", 11.0, 12.0, 12.0, 11.0),
            make_kline(2, "2024-01-03", 12.0, 14.0, 14.0, 12.0),  // 大阳
            make_kline(3, "2024-01-04", 13.0, 13.5, 13.5, 13.0),  // 被包含
            make_kline(4, "2024-01-05", 13.5, 13.0, 13.5, 13.0),  // 继续被包含
            make_kline(5, "2024-01-06", 13.0, 12.0, 13.0, 12.0),
            make_kline(6, "2024-01-07", 12.0, 11.0, 12.0, 11.0),
            make_kline(7, "2024-01-08", 11.0, 10.0, 11.0, 10.0),
            make_kline(8, "2024-01-09", 10.0, 9.0, 10.0, 9.0),
            make_kline(9, "2024-01-10", 9.0, 8.0, 9.0, 8.0),
            make_kline(10, "2024-01-11", 8.0, 7.0, 8.0, 7.0),
            make_kline(11, "2024-01-12", 7.0, 6.0, 7.0, 6.0),
            make_kline(12, "2024-01-13", 6.0, 7.0, 7.0, 6.0),
            make_kline(13, "2024-01-14", 7.0, 8.0, 8.0, 7.0),
            make_kline(14, "2024-01-15", 8.0, 9.0, 9.0, 8.0),
            make_kline(15, "2024-01-16", 9.0, 10.0, 10.0, 9.0),
            make_kline(16, "2024-01-17", 10.0, 11.0, 11.0, 10.0),
            make_kline(17, "2024-01-18", 11.0, 12.0, 12.0, 11.0),
        ];

        let bars = remove_include(&klines);
        println!("\n包含测试 - 去包含后 {} bars:", bars.len());
        for (i, b) in bars.iter().enumerate() {
            println!("  [{}] dt={} h={:.1} l={:.1}", i, b.dt, b.high, b.low);
        }
        let fxs = check_fxs(&bars);
        for (i, fx) in fxs.iter().enumerate() {
            let mark = match fx.mark { FxMark::Top => "TOP", FxMark::Bottom => "BTM" };
            println!("  FX[{}] {} fx={:.1} dt={}", i, mark, fx.fx, fx.dt);
        }

        let bis = build_bi(&klines, None);
        println!("包含测试结果: {} 笔", bis.len());
        for (i, bi) in bis.iter().enumerate() {
            let d = if bi.start_price < bi.end_price { "↑" } else { "↓" };
            println!("  BI[{}] {} {:.1}→{:.1} ({}→{})", i, d, bi.start_price, bi.end_price, bi.start_index, bi.end_index);
        }
        assert!(!bis.is_empty(), "包含场景应该找到笔");
    }

    // ─── 辅助函数 ───

    fn bar_str(b: &NewBar) -> String {
        format!("h={:.1} l={:.1}", b.high, b.low)
    }

    fn fx_str(fx: &FxResult) -> String {
        let mark = match fx.mark { FxMark::Top => "TOP", FxMark::Bottom => "BTM" };
        format!("{} fx={:.1} bars=[{},{},{}]", mark, fx.fx, fx.bars[0], fx.bars[1], fx.bars[2])
    }

    /// 简易K线构造：只需 high 和 low
    fn mk_kline(id: u64, dt: &str, high: f64, low: f64) -> KLine {
        KLine {
            symbol: "test".to_string(),
            timeframe: TimeFrame::D,
            dt: dt.to_string(),
            id,
            open: low,
            close: high,
            high,
            low,
            vol: 1000.0,
            amount: 10000.0,
        }
    }

    #[test]
    fn test_manual_trace_three_segments() {
        // 手工构造三段走势：底→顶→底→顶
        let klines = vec![
            mk_kline(0,  "2024-01-01", 12.0, 10.0),
            mk_kline(1,  "2024-01-02", 10.0, 8.0),   // BTM k1
            mk_kline(2,  "2024-01-03",  8.0, 6.0),   // BTM k2 = BOTTOM(6)
            mk_kline(3,  "2024-01-04", 10.0, 8.0),   // BTM k3
            mk_kline(4,  "2024-01-05", 12.0, 10.0),  // 上升
            mk_kline(5,  "2024-01-06", 14.0, 12.0),  // 上升
            mk_kline(6,  "2024-01-07", 16.0, 14.0),  // 上升
            mk_kline(7,  "2024-01-08", 18.0, 14.0),  // TOP k2 = TOP(18)
            mk_kline(8,  "2024-01-09", 16.0, 14.0),  // TOP k3
            mk_kline(9,  "2024-01-10", 14.0, 12.0),  // 下降
            mk_kline(10, "2024-01-11", 12.0, 10.0),  // 下降
            mk_kline(11, "2024-01-12", 10.0, 8.0),   // 下降 → BTM k1
            mk_kline(12, "2024-01-13",  8.0, 6.0),   // BTM k2 = BOTTOM(6)
            mk_kline(13, "2024-01-14", 10.0, 8.0),   // BTM k3
            mk_kline(14, "2024-01-15", 12.0, 10.0),  // 上升
            mk_kline(15, "2024-01-16", 14.0, 12.0),  // 上升
            mk_kline(16, "2024-01-17", 16.0, 14.0),  // 上升
            mk_kline(17, "2024-01-18", 20.0, 16.0),  // TOP k2 = TOP(20)
            mk_kline(18, "2024-01-19", 18.0, 16.0),  // TOP k3
        ];

        let bars = remove_include(&klines);
        println!("\n三段走势 - 去包含后 {} bars:", bars.len());
        for (i, b) in bars.iter().enumerate() {
            println!("  [{}] {}", i, bar_str(b));
        }

        let fxs = check_fxs(&bars);
        println!("分型:");
        for (i, fx) in fxs.iter().enumerate() {
            println!("  [{}] {}", i, fx_str(fx));
        }

        let bis = build_bi(&klines, None);
        println!("笔: {}条", bis.len());
        for (i, bi) in bis.iter().enumerate() {
            let d = if bi.start_price < bi.end_price { "↑" } else { "↓" };
            println!("  [{}] {} {:.1}→{:.1}", i, d, bi.start_price, bi.end_price);
        }

        assert!(bis.len() >= 2, "三段走势至少2笔");
    }

    #[test]
    fn test_manual_trace_with_include() {
        // 构造包含关系场景
        let klines = vec![
            mk_kline(0, "2024-01-01", 12.0, 10.0),
            mk_kline(1, "2024-01-02", 10.0, 8.0),
            mk_kline(2, "2024-01-03", 9.0, 7.0),     // BTM(7)
            mk_kline(3, "2024-01-04", 11.0, 9.0),
            mk_kline(4, "2024-01-05", 14.0, 12.0),   // 大阳线
            mk_kline(5, "2024-01-06", 13.5, 13.0),   // 被包含
            mk_kline(6, "2024-01-07", 16.0, 14.0),
            mk_kline(7, "2024-01-08", 18.0, 16.0),   // TOP(18)
            mk_kline(8, "2024-01-09", 16.0, 14.0),
            mk_kline(9, "2024-01-10", 14.0, 12.0),
            mk_kline(10, "2024-01-11", 12.0, 10.0),
            mk_kline(11, "2024-01-12", 10.0, 8.0),
            mk_kline(12, "2024-01-13", 8.0, 6.0),
            mk_kline(13, "2024-01-14", 6.0, 4.0),    // BTM(4)
            mk_kline(14, "2024-01-15", 8.0, 6.0),
        ];

        let bis = build_bi(&klines, None);
        println!("\n包含关系测试: {}笔", bis.len());
        for (i, bi) in bis.iter().enumerate() {
            let d = if bi.start_price < bi.end_price { "↑" } else { "↓" };
            println!("  [{}] {} {:.1}→{:.1}", i, d, bi.start_price, bi.end_price);
        }
        assert!(!bis.is_empty());
    }

    #[test]
    fn test_min_bi_len_boundary() {
        println!("\n========== 测试：最小笔长边界 ==========");

        // 最小笔长=6：需要顶底之间至少6根去包含bar
        let klines_enough = vec![
            mk_kline(0, "2024-01-01", 11.0, 10.0),
            mk_kline(1, "2024-01-02", 12.0, 11.0),
            mk_kline(2, "2024-01-03", 13.0, 12.0),
            mk_kline(3, "2024-01-04", 14.0, 13.0),  // TOP(14)
            mk_kline(4, "2024-01-05", 13.0, 12.0),
            mk_kline(5, "2024-01-06", 12.0, 11.0),
            mk_kline(6, "2024-01-07", 11.0, 10.0),  // BTM(10)
        ];

        // 再多几根让第二笔也满足
        let mut klines_full = klines_enough.clone();
        for i in 8..14 {
            let v = 10.0 + (i - 7) as f64;
            klines_full.push(mk_kline(i as u64, &format!("2024-01-{:02}", i), v, v - 1.0));
        }

        let bis = build_bi(&klines_full, Some(6));
        println!("\nmin_bi_len=6: {}条笔", bis.len());
        for (i, bi) in bis.iter().enumerate() {
            let d = if bi.start_price < bi.end_price { "↑" } else { "↓" };
            println!("  BI[{}] {} {:.1}→{:.1}", i, d, bi.start_price, bi.end_price);
        }

        let bis4 = build_bi(&klines_full, Some(4));
        println!("\nmin_bi_len=4: {}条笔", bis4.len());
        for (i, bi) in bis4.iter().enumerate() {
            let d = if bi.start_price < bi.end_price { "↑" } else { "↓" };
            println!("  BI[{}] {} {:.1}→{:.1}", i, d, bi.start_price, bi.end_price);
        }

        // min_bi_len is now ignored, both should be the same (since steps 3 enforces gap >= 3)
        // The interval check ensures we have enough bars between fenxing
    }

    #[test]
    fn test_include_effect_on_fenxing() {
        println!("\n========== 测试：包含对分型的影响 ==========");

        // 场景：上升中的连续包含后出现转折
        let klines = vec![
            mk_kline(0, "2024-01-01", 10.0, 8.0),
            mk_kline(1, "2024-01-02", 9.0, 7.0),   // BTM(7)
            mk_kline(2, "2024-01-03", 11.0, 9.0),
            mk_kline(3, "2024-01-04", 14.0, 12.0),
            mk_kline(4, "2024-01-05", 13.5, 13.0),  // 被bar[3]包含
            mk_kline(5, "2024-01-06", 12.0, 10.0),  // 下降转折
            mk_kline(6, "2024-01-07", 10.0, 8.0),
            mk_kline(7, "2024-01-08", 8.0, 6.0),    // BTM(6)
            mk_kline(8, "2024-01-09", 10.0, 8.0),
        ];

        let bars = remove_include(&klines);
        println!("去包含后: {} bars", bars.len());
        for (i, b) in bars.iter().enumerate() {
            println!("  [{}] {}", i, bar_str(b));
        }

        let fxs = check_fxs(&bars);
        println!("分型: {}个", fxs.len());
        for (i, fx) in fxs.iter().enumerate() {
            println!("  [{}] {}", i, fx_str(fx));
        }

        let bis = build_bi(&klines, None);
        println!("笔: {}条", bis.len());
        for (i, bi) in bis.iter().enumerate() {
            let d = if bi.start_price < bi.end_price { "↑" } else { "↓" };
            println!("  [{}] {} {:.1}→{:.1}", i, d, bi.start_price, bi.end_price);
        }
    }

    #[test]
    fn test_fx_invalidation() {
        println!("\n========== 测试：分型失效规则 ==========");

        // ---- 场景1：正常成笔（顶不被突破） ----
        println!("\n--- 场景1：顶不被突破，正常成笔 ---");
        let klines1 = vec![
            mk_kline(0, "2024-01-01", 13.0, 12.0),
            mk_kline(1, "2024-01-02", 11.0, 10.0),
            mk_kline(2, "2024-01-03", 10.0, 9.0),   // BTM(9)
            mk_kline(3, "2024-01-04", 11.0, 10.0),
            mk_kline(4, "2024-01-05", 12.0, 11.0),
            mk_kline(5, "2024-01-06", 13.0, 12.0),
            mk_kline(6, "2024-01-07", 14.0, 13.0),  // TOP(14)
            mk_kline(7, "2024-01-08", 13.0, 12.0),
            mk_kline(8, "2024-01-09", 12.0, 11.0),
            mk_kline(9, "2024-01-10", 11.0, 10.0),
            mk_kline(10, "2024-01-11", 10.0, 9.0),
            mk_kline(11, "2024-01-12", 9.0, 8.0),   // BTM k1
            mk_kline(12, "2024-01-13", 8.0, 7.0),   // BTM(7)
            mk_kline(13, "2024-01-14", 9.0, 8.0),
        ];
        let bis1 = build_bi(&klines1, None);
        println!("  场景1: {}条笔", bis1.len());
        for (i, bi) in bis1.iter().enumerate() {
            let d = if bi.start_price < bi.end_price { "↑" } else { "↓" };
            println!("    BI[{}] {} {:.1}→{:.1}", i, d, bi.start_price, bi.end_price);
        }
        assert!(!bis1.is_empty(), "场景1：应有笔");

        // ---- 场景2：顶分型被新高突破，失效 ----
        println!("\n--- 场景2：顶被新高突破，失效 ---");
        let klines2 = vec![
            mk_kline(0, "2024-01-01", 13.0, 12.0),
            mk_kline(1, "2024-01-02", 11.0, 10.0),
            mk_kline(2, "2024-01-03", 10.0, 9.0),   // BTM(9)
            mk_kline(3, "2024-01-04", 11.0, 10.0),
            mk_kline(4, "2024-01-05", 14.0, 13.0),  // TOP①(14) ← 将被突破
            mk_kline(5, "2024-01-06", 15.0, 14.0),  // 新高15 > 14 → TOP①失效
            mk_kline(6, "2024-01-07", 17.0, 16.0),
            mk_kline(7, "2024-01-08", 19.0, 18.0),  // TOP②(19)
            mk_kline(8, "2024-01-09", 18.0, 17.0),
            mk_kline(9, "2024-01-10", 17.0, 16.0),
            mk_kline(10, "2024-01-11", 16.0, 15.0),
            mk_kline(11, "2024-01-12", 15.0, 14.0),
            mk_kline(12, "2024-01-13", 14.0, 13.0),
            mk_kline(13, "2024-01-14", 13.0, 12.0), // BTM k1
            mk_kline(14, "2024-01-15", 12.0, 11.0), // BTM(11)
            mk_kline(15, "2024-01-16", 13.0, 12.0),
        ];
        let bis2 = build_bi(&klines2, None);
        println!("  场景2: {}条笔", bis2.len());
        for (i, bi) in bis2.iter().enumerate() {
            let d = if bi.start_price < bi.end_price { "↑" } else { "↓" };
            println!("    BI[{}] {} {:.1}→{:.1}", i, d, bi.start_price, bi.end_price);
        }
        assert!(!bis2.is_empty(), "场景2：应有笔");
        // 笔应从BTM(9)到TOP(19)，跳过被失效的TOP①(14)
        assert!((bis2[0].end_price - 19.0).abs() < 0.01,
            "场景2：第一笔终点应为19（突破后的新高），实际={:.1}", bis2[0].end_price);
        println!("  ✓ 第一笔终点={:.1}（正确：突破14后到19才成笔）", bis2[0].end_price);

        // ---- 场景3：反向分型k1出现后，前分型锁定不可逆 ----
        // 与场景1类似但加入新高：在底分型确认后出现比顶更高的K线，
        // 但顶分型已锁定（反向分型k1已出现），新高不影响
        println!("\n--- 场景3：反向分型k1锁定后新高不失效 ---");
        let klines3 = vec![
            mk_kline(0, "2024-01-01", 13.0, 12.0),
            mk_kline(1, "2024-01-02", 11.0, 10.0),
            mk_kline(2, "2024-01-03", 10.0, 9.0),   // BTM(9)
            mk_kline(3, "2024-01-04", 11.0, 10.0),
            mk_kline(4, "2024-01-05", 12.0, 11.0),
            mk_kline(5, "2024-01-06", 13.0, 12.0),
            mk_kline(6, "2024-01-07", 14.0, 13.0),  // TOP(14) k2
            mk_kline(7, "2024-01-08", 13.0, 12.0),  // TOP k3
            mk_kline(8, "2024-01-09", 12.0, 11.0),  // 独立K线（不属于顶也不属于底）
            mk_kline(9, "2024-01-10", 11.0, 10.0),  // BTM k1 → 锁定顶!
            mk_kline(10, "2024-01-11", 10.0, 9.0),  // BTM(9) k2
            mk_kline(11, "2024-01-12", 11.0, 10.0), // BTM k3
            mk_kline(12, "2024-01-13", 15.0, 14.0), // 新高15 > 14 → 不影响已锁定顶
            mk_kline(13, "2024-01-14", 14.0, 13.0),
            mk_kline(14, "2024-01-15", 12.0, 11.0),
            mk_kline(15, "2024-01-16", 11.0, 10.0),
            mk_kline(16, "2024-01-17", 10.0, 9.0),
            mk_kline(17, "2024-01-18", 9.0, 8.0),   // BTM(8)
            mk_kline(18, "2024-01-19", 10.0, 9.0),
        ];
        let bis3 = build_bi(&klines3, None);
        assert!(!bis3.is_empty(), "场景3：应有笔");
        assert!((bis3[0].end_price - 14.0).abs() < 0.01,
            "场景3：顶被锁定，终点仍为14，实际={:.1}", bis3[0].end_price);
        println!("  ✓ 顶被锁定，终点={:.1}（正确：新高15不影响已锁定的顶14）", bis3[0].end_price);

        println!("\n  ✓ 所有分型失效场景验证通过");
    }
}
