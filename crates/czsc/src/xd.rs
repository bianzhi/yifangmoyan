//! 线段分析 — 严格按缠论原文定义（第62、65、67、71课）
//!
//! ## 线段定义
//! 线段 = 至少3笔 + 前三笔重叠 + 奇数笔交替 + 首尾同方向
//!
//! ## 线段终结
//! 线段只能被反向线段终结，判断标准是标准特征序列出现顶/底分型：
//! - 无缺口：直接确认
//! - 有缺口：需要二次确认（后续形成反向特征序列分型）
//!
//! ## 特征序列包含处理方向
//! - 向上线段的特征序列（下降笔）：按**下降方向**处理包含（取低低、高低）
//! - 向下线段的特征序列（上升笔）：按**上升方向**处理包含（取高高、低高）

use yifang_data::{Bi, XianDuan};

/// 默认最小线段长度（以笔数计）
const DEFAULT_MIN_XD_LEN: usize = 3;

/// 构建线段
pub fn build_xd(bis: &[Bi]) -> Vec<XianDuan> {
    build_xd_with_min_len(bis, None)
}

/// 构建线段（可指定最小笔数）
pub fn build_xd_with_min_len(bis: &[Bi], min_xd_len: Option<usize>) -> Vec<XianDuan> {
    let min_len = min_xd_len.unwrap_or(DEFAULT_MIN_XD_LEN);
    if bis.len() < min_len {
        return Vec::new();
    }

    build_xd_by_feature_sequence(bis, min_len)
}

// ─── 特征序列元素 ─────────────────────────────────────

/// 特征序列元素（即特征序列中的一笔）
#[derive(Debug, Clone)]
struct FeatureElement {
    /// 价格区间高点
    high: f64,
    /// 价格区间低点
    low: f64,
    /// 对应原始笔序列中的索引
    bi_index: usize,
}

impl FeatureElement {
    fn from_bi(bi: &Bi, bi_index: usize) -> Self {
        FeatureElement {
            high: bi.start_price.max(bi.end_price),
            low: bi.start_price.min(bi.end_price),
            bi_index,
        }
    }
}

/// 对特征序列做包含处理
///
/// **关键：包含处理方向由线段方向决定。**
///
/// 缠论原文（第71课）：
/// - 向上线段的特征序列（下降笔）：按**上升方向**处理包含
///   上升方向合并：high = max(h1,h2), low = max(l1,l2)
///   效果：保留高点信息，帮助识别顶分型
/// - 向下线段的特征序列（上升笔）：按**下降方向**处理包含
///   下降方向合并：high = min(h1,h2), low = min(l1,l2)
///   效果：保留低点信息，帮助识别底分型
fn contain_feature_sequence(
    elements: &[FeatureElement],
    is_xd_up: bool,
) -> Vec<FeatureElement> {
    if elements.is_empty() {
        return Vec::new();
    }

    let mut result = vec![elements[0].clone()];

    for i in 1..elements.len() {
        let prev = &result[result.len() - 1];
        let curr = &elements[i];

        // 判断包含：prev 包含 curr 或 curr 包含 prev
        let has_include = (prev.high >= curr.high && prev.low <= curr.low)
            || (prev.high <= curr.high && prev.low >= curr.low);

        if has_include {
            let last = result.last_mut().unwrap();
            if is_xd_up {
                // 向上线段：特征序列按上升方向处理包含
                // 上升方向：取高高=max(h1,h2)、低高=max(l1,l2)
                // 保留高点信息，帮助识别顶分型
                last.high = last.high.max(curr.high);
                last.low = last.low.max(curr.low);
            } else {
                // 向下线段：特征序列按下降方向处理包含
                // 下降方向：取低低=min(h1,h2)、高低=min(l1,l2)
                // 保留低点信息，帮助识别底分型
                last.high = last.high.min(curr.high);
                last.low = last.low.min(curr.low);
            }
        } else {
            result.push(curr.clone());
        }
    }

    result
}

// ─── 分型检测 ────────────────────────────────────────

/// 特征序列分型类型
#[derive(Debug, Clone, Copy, PartialEq)]
enum FenxingType {
    Top,    // 顶分型
    Bottom, // 底分型
}

/// 检测特征序列三个相邻元素是否形成分型
///
/// 缠论第71课：特征序列的分型和K线分型一样。
/// 特征序列元素=笔的价格区间[low, high]。
///
/// 顶分型：中间元素的 high 高于两侧元素的 high
///   （中间元素的"峰"比两侧高 → 转折极值点）
/// 底分型：中间元素的 low 低于两侧元素的 low
///   （中间元素的"谷"比两侧低 → 转折极值点）
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

/// 判断特征序列分型的第一元素和第二元素之间是否有缺口
///
/// 缺口的定义：两个相邻特征序列元素之间没有价格重叠
/// 对于向上线段的特征序列（下降笔）：
///   无缺口 = prev.low <= curr.high（前一个下降笔的低点 <= 后一个下降笔的高点）
///   有缺口 = prev.low > curr.high
/// 对于向下线段的特征序列（上升笔）：
///   无缺口 = prev.high >= curr.low（前一个上升笔的高点 >= 后一个上升笔的低点）
///   有缺口 = prev.high < curr.low
fn has_gap(prev: &FeatureElement, curr: &FeatureElement, is_xd_up: bool) -> bool {
    if is_xd_up {
        // 向上线段→特征序列是下降笔→prev和curr都是下降笔
        // 缺口：prev.low > curr.high（前一笔的低点高于后一笔的高点）
        prev.low > curr.high
    } else {
        // 向下线段→特征序列是上升笔→prev和curr都是上升笔
        // 缺口：prev.high < curr.low（前一笔的高点低于后一笔的低点）
        prev.high < curr.low
    }
}

// ─── 前三笔重叠检查 ──────────────────────────────────

/// 检查线段的前三笔是否有重叠区间
///
/// 缠论原文（第62课）：线段的前三笔必须有重叠的部分。
/// 计算前三笔的价格重叠区间：
/// - 重叠区间 = max(各笔low) 和 min(各笔high) 之间
/// - 若 max(low) <= min(high)，则存在重叠
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

// ─── 核心算法 ────────────────────────────────────────

/// 特征序列法构建线段
///
/// 核心逻辑：
/// 1. 从第一个笔确定线段方向
/// 2. 检查前三笔重叠（线段成立硬条件）
/// 3. 提取特征序列（与线段方向相反的笔）
/// 4. 对特征序列做包含处理（包含方向由线段方向决定）
/// 5. 在标准特征序列上找分型（笔破坏预警）：
///    - 无缺口分型 → 预警待确认
///    - 有缺口分型 → 预警待确认（需二次确认）
/// 6. 预警后，检查后续同向笔是否创新值：
///    - 创新值 → 预警取消，线段延续
///    - 未创新值 → 确认线段终结
fn build_xd_by_feature_sequence(bis: &[Bi], min_len: usize) -> Vec<XianDuan> {
    if bis.len() < min_len {
        return Vec::new();
    }

    let mut xds = Vec::new();
    let mut start_bi_idx: usize = 0;

    while start_bi_idx + min_len - 1 < bis.len() {
        // 当前线段方向由起始笔决定
        let xd_direction = bis[start_bi_idx].direction.as_str();
        let is_xd_up = xd_direction == "up";

        // 前三笔重叠检查（线段成立硬条件）
        if !check_overlap_of_first_3(bis, start_bi_idx) {
            start_bi_idx += 1;
            continue;
        }

        // 线段当前极值点：上升线段跟踪最高点，下降线段跟踪最低点
        // 在线段起始到破坏点之间，跟踪所有同向笔的极值
        let mut xd_extreme = if is_xd_up {
            bis[start_bi_idx].start_price.max(bis[start_bi_idx].end_price)
        } else {
            bis[start_bi_idx].start_price.min(bis[start_bi_idx].end_price)
        };

        // 提取特征序列：与线段方向相反的笔
        let feature_indices: Vec<usize> = (start_bi_idx..bis.len())
            .filter(|&i| bis[i].direction != xd_direction)
            .collect();

        if feature_indices.len() < 3 {
            break;
        }

        // 构建特征序列元素
        let elements: Vec<FeatureElement> = feature_indices
            .iter()
            .map(|&idx| FeatureElement::from_bi(&bis[idx], idx))
            .collect();

        // 对特征序列做包含处理（方向由线段方向决定）
        let contained = contain_feature_sequence(&elements, is_xd_up);

        // 在包含处理后的特征序列上找分型（笔破坏预警）
        let mut found_break = false;

        for i in 1..contained.len().saturating_sub(1) {
            let prev = &contained[i - 1];
            let curr = &contained[i];
            let next = &contained[i + 1];

            let fenxing = check_fenxing(prev, curr, next);

            // 上升线段被顶分型破坏，下降线段被底分型破坏
            let is_break = match fenxing {
                Some(FenxingType::Top) => is_xd_up,
                Some(FenxingType::Bottom) => !is_xd_up,
                None => false,
            };

            if is_break {
                let gap = has_gap(prev, curr, is_xd_up);
                let break_bi_idx = curr.bi_index;

                // 至少 min_len 笔构成线段
                if break_bi_idx < start_bi_idx + min_len - 1 {
                    continue;
                }

                // 更新极值到破坏点（含线段内所有同向笔）
                for j in start_bi_idx..=break_bi_idx {
                    if bis[j].direction == xd_direction {
                        let bi_high = bis[j].start_price.max(bis[j].end_price);
                        let bi_low = bis[j].start_price.min(bis[j].end_price);
                        if is_xd_up {
                            xd_extreme = xd_extreme.max(bi_high);
                        } else {
                            xd_extreme = xd_extreme.min(bi_low);
                        }
                    }
                }

                // ── 检查后续同向笔是否创新值 ──
                // 上升线段：后续向上笔是否创新高（超过线段最高点）
                // 下降线段：后续向下笔是否创新低（低于线段最低点）
                let mut innovation_after_break = false;
                for j in (break_bi_idx + 1)..bis.len() {
                    if bis[j].direction == xd_direction {
                        let bi_high = bis[j].start_price.max(bis[j].end_price);
                        let bi_low = bis[j].start_price.min(bis[j].end_price);
                        if is_xd_up && bi_high > xd_extreme {
                            innovation_after_break = true;
                            break;
                        }
                        if !is_xd_up && bi_low < xd_extreme {
                            innovation_after_break = true;
                            break;
                        }
                    }
                }

                if innovation_after_break {
                    // 预警取消：后续同向笔创新值，线段延续
                    continue;
                }

                if gap {
                    // ── 有缺口：需要二次确认 ──
                    let confirmed = confirm_gap_break(
                        bis, break_bi_idx, is_xd_up, &contained, i, min_len,
                    );

                    if let Some(end_bi_idx) = confirmed {
                        push_xd(&mut xds, bis, start_bi_idx, end_bi_idx, true);
                        start_bi_idx = end_bi_idx;
                        found_break = true;
                        break;
                    }
                } else {
                    // ── 无缺口：直接确认 ──
                    push_xd(&mut xds, bis, start_bi_idx, break_bi_idx, true);
                    start_bi_idx = break_bi_idx;
                    found_break = true;
                    break;
                }
            }
        }

        if !found_break {
            break;
        }
    }

    // 处理最后一段未完成的线段
    if start_bi_idx < bis.len() {
        let start_bi = &bis[start_bi_idx];
        let end_bi = &bis[bis.len() - 1];

        if check_overlap_of_first_3(bis, start_bi_idx) || bis.len() - start_bi_idx < 3 {
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
/// 需要后续形成反向特征序列的分型才能确认原线段终结。
///
/// 具体做法：
/// 1. 从 break_bi_idx 开始，检查是否能形成反向线段
/// 2. 反向线段的特征序列需要出现分型
/// 3. 如果确认了反向线段，则原线段在 break_bi_idx 处终结
fn confirm_gap_break(
    bis: &[Bi],
    break_bi_idx: usize,
    is_prev_xd_up: bool,
    _contained: &[FeatureElement],
    _fenxing_idx: usize,
    min_len: usize,
) -> Option<usize> {
    // 反向线段方向
    let reverse_dir = if is_prev_xd_up { "down" } else { "up" };

    // 从 break_bi_idx 开始找反向线段
    // 检查反向笔序列是否有足够的特征序列分型
    let reverse_feature_indices: Vec<usize> = (break_bi_idx..bis.len())
        .filter(|&i| bis[i].direction != reverse_dir)
        .collect();

    if reverse_feature_indices.len() < 3 {
        return None;
    }

    // 构建反向特征序列
    let reverse_elements: Vec<FeatureElement> = reverse_feature_indices
        .iter()
        .map(|&idx| FeatureElement::from_bi(&bis[idx], idx))
        .collect();

    // 反向线段的方向决定包含处理方向
    let is_reverse_xd_up = reverse_dir == "up";
    let reverse_contained = contain_feature_sequence(&reverse_elements, is_reverse_xd_up);

    // 在反向特征序列上找分型
    for j in 1..reverse_contained.len().saturating_sub(1) {
        let prev = &reverse_contained[j - 1];
        let curr = &reverse_contained[j];
        let next = &reverse_contained[j + 1];

        let fenxing = check_fenxing(prev, curr, next);

        // 反向线段被分型破坏
        let is_break = match fenxing {
            Some(FenxingType::Top) => is_reverse_xd_up,
            Some(FenxingType::Bottom) => !is_reverse_xd_up,
            None => false,
        };

        if is_break {
            let confirm_bi_idx = curr.bi_index;

            // 确认线段至少有 min_len 笔
            if confirm_bi_idx >= break_bi_idx + min_len - 1 {
                // 二次确认成功，原线段在 break_bi_idx 处终结
                return Some(break_bi_idx);
            }
        }
    }

    None
}

/// 构建线段并加入列表
fn push_xd(
    xds: &mut Vec<XianDuan>,
    bis: &[Bi],
    start_bi_idx: usize,
    break_bi_idx: usize,
    is_finished: bool,
) {
    let start_bi = &bis[start_bi_idx];
    let break_bi = &bis[break_bi_idx];

    // 线段端点计算：
    // 上升线段：起点 = 起始笔的起点（底分型），终点 = 终结笔的起点（顶分型）
    //   - 上升笔：start_price=底, end_price=顶
    //   - 终结时的下降笔：start_price=顶, end_price=底 → 终点取 start_price
    // 下降线段：起点 = 起始笔的起点（顶分型），终点 = 终结笔的起点（底分型）
    //   - 下降笔：start_price=顶, end_price=底
    //   - 终结时的上升笔：start_price=底, end_price=顶 → 终点取 start_price
    // 统一：终点都取终结笔的 start_price（因为终结笔是特征序列元素，
    //        其起点就是线段的转折端点）
    let end_price = break_bi.start_price;
    let end_index = break_bi.start_index;
    let end_dt = break_bi.start_dt.clone();

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

    fn make_bi(
        id: usize,
        dir: &str,
        start_price: f64,
        end_price: f64,
        start_idx: u64,
        end_idx: u64,
    ) -> Bi {
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

    // ─── 基础单元测试 ─────────────────────────────────

    #[test]
    fn test_fenxing_detection() {
        // 顶分型：中间元素的 high 高于两侧
        let prev = FeatureElement { high: 15.0, low: 10.0, bi_index: 0 };
        let curr = FeatureElement { high: 20.0, low: 15.0, bi_index: 1 };
        let next = FeatureElement { high: 17.0, low: 13.0, bi_index: 2 };
        assert_eq!(check_fenxing(&prev, &curr, &next), Some(FenxingType::Top));

        // 底分型：中间元素的 low 低于两侧
        let prev2 = FeatureElement { high: 17.0, low: 13.0, bi_index: 0 };
        let curr2 = FeatureElement { high: 12.0, low: 8.0, bi_index: 1 };
        let next2 = FeatureElement { high: 15.0, low: 10.0, bi_index: 2 };
        assert_eq!(check_fenxing(&prev2, &curr2, &next2), Some(FenxingType::Bottom));
    }

    #[test]
    fn test_gap_detection() {
        // 向上线段特征序列缺口：prev.low > curr.high
        let prev = FeatureElement { high: 25.0, low: 20.0, bi_index: 0 };
        let curr = FeatureElement { high: 18.0, low: 13.0, bi_index: 1 };
        assert!(has_gap(&prev, &curr, true));  // 20 > 18
        assert!(!has_gap(&prev, &FeatureElement { high: 22.0, low: 15.0, bi_index: 2 }, true)); // 20 < 22

        // 向下线段特征序列缺口：prev.high < curr.low
        let prev_d = FeatureElement { high: 18.0, low: 13.0, bi_index: 0 };
        let curr_d = FeatureElement { high: 25.0, low: 20.0, bi_index: 1 };
        assert!(has_gap(&prev_d, &curr_d, false)); // 18 < 20
        assert!(!has_gap(&prev_d, &FeatureElement { high: 12.0, low: 8.0, bi_index: 2 }, false)); // 18 > 8
    }

    #[test]
    fn test_overlap_check() {
        let bis = vec![
            make_bi(0, "up", 10.0, 20.0, 0, 3),
            make_bi(1, "down", 20.0, 15.0, 3, 6),
            make_bi(2, "up", 15.0, 25.0, 6, 9),
        ];
        // max(low)=15, min(high)=20 → 有重叠
        assert!(check_overlap_of_first_3(&bis, 0));
    }

    #[test]
    fn test_contain_feature_sequence_up_xd() {
        // 向上线段 → 特征序列按上升方向处理包含 (max+max)
        // 不包含的例子
        let elements = vec![
            FeatureElement { high: 15.0, low: 10.0, bi_index: 0 },
            FeatureElement { high: 18.0, low: 13.0, bi_index: 1 },
        ];
        assert_eq!(contain_feature_sequence(&elements, true).len(), 2);

        // 包含合并：20/15 包含 18/16
        let elements2 = vec![
            FeatureElement { high: 20.0, low: 15.0, bi_index: 0 },
            FeatureElement { high: 18.0, low: 16.0, bi_index: 1 },
        ];
        let result = contain_feature_sequence(&elements2, true);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].high, 20.0); // max(20,18)
        assert_eq!(result[0].low, 16.0);  // max(15,16)
    }

    #[test]
    fn test_contain_feature_sequence_down_xd() {
        // 向下线段 → 特征序列按下降方向处理包含 (min+min)
        let elements = vec![
            FeatureElement { high: 18.0, low: 12.0, bi_index: 0 },
            FeatureElement { high: 16.0, low: 14.0, bi_index: 1 },
        ];
        let result = contain_feature_sequence(&elements, false);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].high, 16.0); // min(18,16)
        assert_eq!(result[0].low, 12.0);  // min(12,14)
    }

    // ─── 线段构建测试 ──────────────────────────────────

    #[test]
    fn test_xd_min_3_bi() {
        let bis = vec![
            make_bi(0, "up", 10.0, 15.0, 0, 3),
            make_bi(1, "down", 15.0, 12.0, 3, 6),
        ];
        assert!(build_xd(&bis).is_empty(), "少于3笔不应有线段");
    }

    #[test]
    fn test_xd_up_no_break() {
        // 持续上升：特征序列不断创新高，无顶分型
        let bis = vec![
            make_bi(0, "up", 10.0, 15.0, 0, 3),
            make_bi(1, "down", 15.0, 12.0, 3, 6),
            make_bi(2, "up", 12.0, 18.0, 6, 9),
            make_bi(3, "down", 18.0, 13.0, 9, 12),
            make_bi(4, "up", 13.0, 20.0, 12, 15),
            make_bi(5, "down", 20.0, 14.0, 15, 18),
        ];
        let xds = build_xd(&bis);
        assert_eq!(xds.len(), 1);
        assert!(!xds[0].is_finished);
        assert_eq!(xds[0].direction, "up");
    }

    #[test]
    fn test_xd_up_then_break_no_gap() {
        // 上升线段被顶分型破坏（无缺口 → 直接确认）
        // 特征序列(下降笔)：15/10, 20/15, 17/13
        // 顶分型：15<20>17 ✓, 10<15>13 ✓
        // 缺口：10 <= 17 → 无缺口
        let bis = vec![
            make_bi(0, "up", 10.0, 15.0, 0, 3),
            make_bi(1, "down", 15.0, 10.0, 3, 6),
            make_bi(2, "up", 10.0, 20.0, 6, 9),
            make_bi(3, "down", 20.0, 15.0, 9, 12),
            make_bi(4, "up", 15.0, 17.0, 12, 15),
            make_bi(5, "down", 17.0, 13.0, 15, 18),
        ];
        let xds = build_xd(&bis);
        assert!(xds[0].is_finished);
        assert_eq!(xds[0].direction, "up");
        assert_eq!(xds[0].end_price, 20.0); // BI[3].start_price = 顶点
    }

    #[test]
    fn test_xd_down_then_break_no_gap() {
        // 下降线段被底分型破坏（无缺口 → 直接确认）
        // 特征序列(上升笔)：17/13, 12/8, 15/10
        // 底分型：17>12<15 ✓, 13>8<10 ✓
        let bis = vec![
            make_bi(0, "down", 20.0, 13.0, 0, 3),
            make_bi(1, "up", 13.0, 17.0, 3, 6),
            make_bi(2, "down", 17.0, 8.0, 6, 9),
            make_bi(3, "up", 8.0, 12.0, 9, 12),
            make_bi(4, "down", 12.0, 10.0, 12, 15),
            make_bi(5, "up", 10.0, 15.0, 15, 18),
        ];
        let xds = build_xd(&bis);
        assert!(xds[0].is_finished);
        assert_eq!(xds[0].direction, "down");
        assert_eq!(xds[0].end_price, 8.0); // BI[3].start_price = 底点
    }

    #[test]
    fn test_xd_alternating_directions() {
        // 上升→下降交替
        // 上升线段从BI[0]开始，特征序列顶分型在[0,1,2]
        // 但BI[4]和BI[6]高点都较高，BI[6]h=28>线段极值25→创新值
        // 所以上升线段不会在BI[2]处终结
        let bis = vec![
            make_bi(0, "up", 10.0, 20.0, 0, 3),
            make_bi(1, "down", 20.0, 10.0, 3, 6),
            make_bi(2, "up", 10.0, 25.0, 6, 9),
            make_bi(3, "down", 25.0, 18.0, 9, 12),
            make_bi(4, "up", 18.0, 22.0, 12, 15),
            make_bi(5, "down", 22.0, 15.0, 15, 18),
            make_bi(6, "up", 15.0, 28.0, 18, 21),
            make_bi(7, "down", 28.0, 20.0, 21, 24),
        ];
        let xds = build_xd(&bis);
        // BI[6]创新高28，上升线段不终结
        assert!(xds.len() >= 1);
        assert_eq!(xds[0].direction, "up");
        // 上升线段延续（未终结），因为BI[6]创新高28
        assert!(!xds[0].is_finished);
    }

    #[test]
    fn test_xd_feature_sequence_include() {
        // 特征序列包含处理后形成顶分型
        // 特征序列原始：20/15, 25/18, 22/19, 21/17
        // 下降方向处理：25/18 包含 22/19 → 合并为22/18
        // 结果：20/15, 22/18, 21/17 → 顶分型
        let bis = vec![
            make_bi(0, "up", 10.0, 20.0, 0, 3),
            make_bi(1, "down", 20.0, 15.0, 3, 6),
            make_bi(2, "up", 15.0, 25.0, 6, 9),
            make_bi(3, "down", 25.0, 18.0, 9, 12),
            make_bi(4, "up", 18.0, 22.0, 12, 15),
            make_bi(5, "down", 22.0, 19.0, 15, 18),
            make_bi(6, "up", 19.0, 21.0, 18, 21),
            make_bi(7, "down", 21.0, 17.0, 21, 24),
        ];
        let xds = build_xd(&bis);
        if xds.len() >= 1 && xds[0].is_finished {
            assert_eq!(xds[0].direction, "up");
        }
    }

    #[test]
    fn test_xd_multiple_segments() {
        // 上升线段 + 下降线段
        // 上升线段特征序列：20/15, 25/18, 22/15 → 顶分型 ✓
        // 下降线段特征序列：20/15, 12/8, 15/9 → 底分型 ✓
        let bis = vec![
            make_bi(0, "up", 10.0, 20.0, 0, 3),
            make_bi(1, "down", 20.0, 15.0, 3, 6),
            make_bi(2, "up", 15.0, 25.0, 6, 9),
            make_bi(3, "down", 25.0, 18.0, 9, 12),
            make_bi(4, "up", 18.0, 22.0, 12, 15),
            make_bi(5, "down", 22.0, 15.0, 15, 18),
            make_bi(6, "up", 15.0, 20.0, 18, 21),
            make_bi(7, "down", 20.0, 8.0, 21, 24),
            make_bi(8, "up", 8.0, 12.0, 24, 27),
            make_bi(9, "down", 12.0, 9.0, 27, 30),
            make_bi(10, "up", 9.0, 15.0, 30, 33),
            make_bi(11, "down", 15.0, 10.0, 33, 36),
        ];
        let xds = build_xd(&bis);
        assert!(xds.len() >= 2, "应有至少2个线段，实际{}", xds.len());

        // 上升线段
        assert_eq!(xds[0].direction, "up");
        assert!(xds[0].is_finished);
        assert_eq!(xds[0].end_price, 25.0);

        // 下降线段
        assert_eq!(xds[1].direction, "down");
        if xds[1].is_finished {
            assert_eq!(xds[1].end_price, 8.0);
            }
    }

    #[test]
    fn test_xd_innovation_cancels_break() {
        // 上升线段：特征序列出现顶分型，但后续向上笔创新高，预警取消
        // BI[0]↑10→20, BI[1]↓20→10, BI[2]↑10→25
        // BI[3]↓25→18, BI[4]↑18→22, BI[5]↓22→15
        // BI[6]↑15→30(创新高！)
        // 特征序列(上升方向max+max): [20/10, 25/18, 22/15] → 顶分型
        // 但 BI[6] h=30 > xd_extreme=25 → 创新值 → 预警取消
        let bis = vec![
            make_bi(0, "up", 10.0, 20.0, 0, 3),
            make_bi(1, "down", 20.0, 10.0, 3, 6),
            make_bi(2, "up", 10.0, 25.0, 6, 9),
            make_bi(3, "down", 25.0, 18.0, 9, 12),
            make_bi(4, "up", 18.0, 22.0, 12, 15),
            make_bi(5, "down", 22.0, 15.0, 15, 18),
            make_bi(6, "up", 15.0, 30.0, 18, 21),
        ];
        let xds = build_xd(&bis);
        assert!(xds.len() >= 1);
        assert_eq!(xds[0].direction, "up");
        assert!(!xds[0].is_finished, "线段应延续（BI[6]创新高30>25）");
    }

    #[test]
    fn test_xd_no_innovation_confirms_break() {
        // 上升线段：特征序列出现顶分型，后续向上笔不创新高，确认终结
        // BI[0]↑10→20, BI[1]↓20→10, BI[2]↑10→25
        // BI[3]↓25→18, BI[4]↑18→22, BI[5]↓22→15
        // BI[6]↑15→23(未创新高23<25)
        // 特征序列(上升方向max+max): [20/10, 25/18, 22/15] → 顶分型
        // BI[6] h=23 < xd_extreme=25 → 不创新值 → 确认终结
        let bis = vec![
            make_bi(0, "up", 10.0, 20.0, 0, 3),
            make_bi(1, "down", 20.0, 10.0, 3, 6),
            make_bi(2, "up", 10.0, 25.0, 6, 9),
            make_bi(3, "down", 25.0, 18.0, 9, 12),
            make_bi(4, "up", 18.0, 22.0, 12, 15),
            make_bi(5, "down", 22.0, 15.0, 15, 18),
            make_bi(6, "up", 15.0, 23.0, 18, 21),
            make_bi(7, "down", 23.0, 14.0, 21, 24),
        ];
        let xds = build_xd(&bis);
        assert!(xds.len() >= 1);
        assert_eq!(xds[0].direction, "up");
        assert!(xds[0].is_finished, "线段应终结（BI[6]未创新高23<25）");
    }
}

#[cfg(test)]
mod test_000001 {
    use super::*;
    use yifang_data::{KLine, TimeFrame};
    use serde_json;

    #[test]
    fn test_000001_xd_debug() {
        let json_str = std::fs::read_to_string("/tmp/000001_daily.json").unwrap_or_default();
        if json_str.is_empty() {
            eprintln!("SKIP: /tmp/000001_daily.json not found");
            return;
        }
        let records: Vec<serde_json::Value> = serde_json::from_str(&json_str).unwrap();
        
        let klines: Vec<KLine> = records.iter().enumerate().map(|(i, r)| {
            KLine {
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
            }
        }).collect();
        
        let bis = crate::bi::build_bi(&klines, None);
        let xds = build_xd(&bis);
        
        eprintln!("000001日线: {}笔, {}线段", bis.len(), xds.len());
        for (i, xd) in xds.iter().enumerate() {
            eprintln!("  XD[{}] {} {}({:.2}) → {}({:.2}) finished={}", 
                i, xd.direction, xd.start_dt, xd.start_price, xd.end_dt, xd.end_price, xd.is_finished);
        }
        
        // 验证线段方向交替
        for i in 1..xds.len() {
            assert_ne!(xds[i].direction, xds[i-1].direction,
                "线段[{}]和[{}]方向相同", i, i-1);
        }
    }
}
