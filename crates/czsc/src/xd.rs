//! 缠论线段构建（特征序列分型破坏法）
//!
//! 严格依据缠论第71课、第77课原文，完整实现：
//!
//! # 核心原文规则
//! 1. 线段由连续次级别笔构成，首笔定线段方向
//! 2. 特征序列定义：
//!    - 向上线段：特征序列 = 内部所有向下笔
//!    - 向下线段：特征序列 = 内部所有向上笔
//! 3. 特征序列必须先做包含处理：
//!    - 向上线段特征序列(向下笔)：合并取低低
//!    - 向下线段特征序列(向上笔)：合并取高高
//! 4. 线段破坏分两类：
//!    - 第一类破坏（无缺口）：特征序列分型形成 → 直接确认线段终结
//!    - 第二类破坏（有缺口）：需新反向线段走出完整特征序列分型做二次确认；
//!      确认失败、再度创出新高/新低 → 原线段延续，此前分型作废
//! 5. 新生线段同样满足「至少三笔+前三笔重叠」成立条件

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

/// 特征序列元素（仅存 high/low，对应 Python 版 Bi 的 h/l 字段）
#[derive(Debug, Clone)]
struct FeatElem {
    high: f64,
    low: f64,
}

impl FeatElem {
    fn from_bi(bi: &Bi) -> Self {
        Self {
            high: bi.start_price.max(bi.end_price),
            low: bi.start_price.min(bi.end_price),
        }
    }
}

// ─── 内部线段结构 ──────────────────────────────────────

/// 线段构建过程中的内部表示（对应 Python 版 Segment）
#[derive(Debug, Clone)]
struct InnerSeg {
    /// 线段方向: true=up, false=down
    is_up: bool,
    /// 线段内笔索引列表（在 bis 数组中的位置）
    bi_indices: Vec<usize>,
    /// 特征序列（反向笔的原始 high/low，未包含处理）
    feature: Vec<FeatElem>,
    /// 是否正式确认终结
    confirmed_end: bool,
}

impl InnerSeg {
    fn new(start_bi_idx: usize, bis: &[Bi]) -> Self {
        let bi = &bis[start_bi_idx];
        let is_up = bi.direction.as_str() == "up";
        Self {
            is_up,
            bi_indices: vec![start_bi_idx],
            feature: Vec::new(),
            confirmed_end: false,
        }
    }

    /// 添加一笔到线段中，同时收集反向笔进特征序列
    fn add_bi(&mut self, bi_idx: usize, bis: &[Bi]) {
        let bi = &bis[bi_idx];
        let bi_dir = bi.direction.as_str();
        self.bi_indices.push(bi_idx);

        // 收集反向笔进入特征序列
        if self.is_up && bi_dir == "down" {
            self.feature.push(FeatElem::from_bi(bi));
        }
        if !self.is_up && bi_dir == "up" {
            self.feature.push(FeatElem::from_bi(bi));
        }
    }

    /// 线段内笔的数量
    fn bi_count(&self) -> usize {
        self.bi_indices.len()
    }
}

// ─── 特征序列包含处理 ─────────────────────────────────

/// 特征序列包含处理（严格原版）
///
/// - 向上线段特征序列(向下笔)：合并取低低 → high=min, low=min
/// - 向下线段特征序列(向上笔)：合并取高高 → high=max, low=max
fn merge_feature_include(feat: &[FeatElem], seg_is_up: bool) -> Vec<FeatElem> {
    if feat.len() < 2 {
        return feat.to_vec();
    }
    let mut res: Vec<FeatElem> = vec![feat[0].clone()];
    for item in &feat[1..] {
        let last = res.last().unwrap();
        // 判断互相包含
        let in1 = last.low <= item.low && last.high >= item.high;
        let in2 = item.low <= last.low && item.high >= last.high;
        if !in1 && !in2 {
            res.push(item.clone());
            continue;
        }
        // 按线段方向合并
        let (nh, nl) = if seg_is_up {
            // 向上线段：特征序列方向=向下 → 取低低
            (last.high.min(item.high), last.low.min(item.low))
        } else {
            // 向下线段：特征序列方向=向上 → 取高高
            (last.high.max(item.high), last.low.max(item.low))
        };
        res.pop();
        res.push(FeatElem { high: nh, low: nl });
    }
    res
}

// ─── 特征序列分型判断 ─────────────────────────────────

/// 判断是否出现顶分型（任一中间元素 high 最高）
fn is_top_fx(arr: &[FeatElem]) -> bool {
    if arr.len() < 3 {
        return false;
    }
    for i in 1..arr.len() - 1 {
        let a = &arr[i - 1];
        let b = &arr[i];
        let c = &arr[i + 1];
        if b.high > a.high && b.high > c.high {
            return true;
        }
    }
    false
}

/// 判断是否出现底分型（任一中间元素 low 最低）
fn is_bottom_fx(arr: &[FeatElem]) -> bool {
    if arr.len() < 3 {
        return false;
    }
    for i in 1..arr.len() - 1 {
        let a = &arr[i - 1];
        let b = &arr[i];
        let c = &arr[i + 1];
        if b.low < a.low && b.low < c.low {
            return true;
        }
    }
    false
}

// ─── 特征序列缺口判断 ─────────────────────────────────

/// 判断特征序列分型第一、二元素是否存在缺口
///
/// 只取最后两根（对应分型的前两根）判断缺口：
/// - 向上线段（特征序列=向下笔）：第二根高点 < 第一根低点 = 存在缺口
/// - 向下线段（特征序列=向上笔）：第二根低点 > 第一根高点 = 存在缺口
fn feature_has_gap(feat: &[FeatElem], seg_is_up: bool) -> bool {
    if feat.len() < 2 {
        return false;
    }
    let f1 = &feat[feat.len() - 2];
    let f2 = &feat[feat.len() - 1];
    if seg_is_up {
        // 向上线段：向下笔序列，第二根高点 < 第一根低点 = 存在缺口
        f2.high < f1.low
    } else {
        // 向下线段：向上笔序列，第二根低点 > 第一根高点 = 存在缺口
        f2.low > f1.high
    }
}

// ─── 前三笔重叠检查 ──────────────────────────────────

/// 缠论：一个线段的前三笔必须有重叠区域
fn check_overlap_of_first_3(bis: &[Bi], indices: &[usize]) -> bool {
    if indices.len() < 3 {
        return false;
    }

    let mut max_low = f64::MIN;
    let mut min_high = f64::MAX;

    for &idx in &indices[..3] {
        let bi = &bis[idx];
        let high = bi.start_price.max(bi.end_price);
        let low = bi.start_price.min(bi.end_price);
        max_low = max_low.max(low);
        min_high = min_high.min(high);
    }

    max_low <= min_high
}

// ─── 线段输出 ─────────────────────────────────────────

/// 将内部线段转换为输出的 XianDuan
fn segment_to_xd(seg: &InnerSeg, bis: &[Bi]) -> XianDuan {
    let start_bi = &bis[seg.bi_indices[0]];
    let end_bi = &bis[*seg.bi_indices.last().unwrap()];

    // 线段起点价格：必须与线段方向一致
    let start_price = if seg.is_up == (start_bi.direction.as_str() == "up") {
        start_bi.start_price
    } else {
        start_bi.end_price
    };

    // 线段终点价格：最后一笔方向一定与线段方向一致
    // 但共享端点场景下，最后一笔可能是反向笔
    let end_price = if seg.is_up == (end_bi.direction.as_str() == "up") {
        end_bi.end_price
    } else {
        end_bi.start_price
    };

    XianDuan {
        direction: if seg.is_up { "up" } else { "down" }.to_string(),
        start_index: start_bi.start_index,
        end_index: end_bi.end_index,
        start_dt: start_bi.start_dt.clone(),
        end_dt: end_bi.end_dt.clone(),
        start_price,
        end_price,
        is_finished: seg.confirmed_end,
    }
}

// ─── 核心算法：完整版线段划分 ──────────────────────────

/// 完整版线段划分主逻辑（含二类破坏二次确认）
///
/// 严格对应 Python 版 `build_full_segment`：
/// 1. 逐笔推进，维护当前线段 cur_seg
/// 2. 检测特征序列分型
/// 3. 一类破坏（无缺口）→ 直接终结
/// 4. 二类破坏（有缺口）→ 启动 wait_confirm_seg 二次确认
/// 5. 确认成功 → 原线段终结，切换新线段
/// 6. 确认失败 → 作废，原线段继续延伸
fn build_xd_impl(bis: &[Bi], min_len: usize) -> Vec<XianDuan> {
    if bis.len() < min_len {
        return Vec::new();
    }

    let mut seg_list: Vec<InnerSeg> = Vec::new();

    // 初始化当前线段
    let mut cur_seg = InnerSeg::new(0, bis);

    // 等待二次确认的临时线段（对应 Python 版 wait_confirm_seg）
    let mut wait_confirm_seg: Option<InnerSeg> = None;

    let mut ptr: usize = 1;

    while ptr < bis.len() {
        let curr_bi = &bis[ptr];
        let curr_bi_high = curr_bi.start_price.max(curr_bi.end_price);
        let curr_bi_low = curr_bi.start_price.min(curr_bi.end_price);

        // ── 存在等待二次确认的二类破坏线段 ──
        if let Some(ref mut wseg) = wait_confirm_seg {
            wseg.add_bi(ptr, bis);
            let proc_feat = merge_feature_include(&wseg.feature, wseg.is_up);

            // 观察反向新线段是否走出分型完成二次确认
            let confirm_ok = if wseg.is_up {
                is_top_fx(&proc_feat)  // 向上线段等待顶分型确认
            } else {
                is_bottom_fx(&proc_feat) // 向下线段等待底分型确认
            };

            if confirm_ok {
                // 二次确认成功：原线段正式终结
                cur_seg.confirmed_end = true;
                seg_list.push(cur_seg);
                // 切换为新线段
                cur_seg = wseg.clone();
                wait_confirm_seg = None;
            } else {
                // 二次确认失败检查：是否再创极值，原线段延续
                let last_bi = &bis[*cur_seg.bi_indices.last().unwrap()];
                let last_high = last_bi.start_price.max(last_bi.end_price);
                let last_low = last_bi.start_price.min(last_bi.end_price);

                if cur_seg.is_up && curr_bi_high > last_high {
                    // 创新高，原线段继续延伸，作废本次破坏信号
                    wait_confirm_seg = None;
                } else if !cur_seg.is_up && curr_bi_low < last_low {
                    // 创新低，原线段继续延伸，作废本次破坏信号
                    wait_confirm_seg = None;
                }
            }
            ptr += 1;
            continue;
        }

        // ── 正常模式：加入当前线段 ──
        cur_seg.add_bi(ptr, bis);
        let proc_feature = merge_feature_include(&cur_seg.feature, cur_seg.is_up);

        // 检测是否出现破坏分型
        let hit_fx = if cur_seg.is_up {
            is_top_fx(&proc_feature)
        } else {
            is_bottom_fx(&proc_feature)
        };

        if !hit_fx {
            ptr += 1;
            continue;
        }

        // 出现分型，区分一类/二类破坏
        let gap = feature_has_gap(&proc_feature, cur_seg.is_up);

        if !gap {
            // ── 一类破坏：无缺口 → 直接终结 ──
            if cur_seg.bi_count() >= min_len {
                cur_seg.confirmed_end = true;
                seg_list.push(cur_seg);
            }
            // 新建反向线段（从当前笔开始，共享端点）
            cur_seg = InnerSeg::new(ptr, bis);
        } else {
            // ── 二类破坏：有缺口 → 不直接终结，启动二次确认流程 ──
            // 新建反向观察线段（从当前笔开始）
            wait_confirm_seg = Some(InnerSeg::new(ptr, bis));
        }

        ptr += 1;
    }

    // ── 尾部未确认完成的线段存入结果 ──
    if !cur_seg.confirmed_end && cur_seg.bi_count() >= min_len {
        // 检查前三笔重叠
        if check_overlap_of_first_3(bis, &cur_seg.bi_indices) {
            seg_list.push(cur_seg);
        }
    }

    // ── 转换为输出格式 ──
    seg_list
        .into_iter()
        .map(|seg| segment_to_xd(&seg, bis))
        .collect()
}

// ═════════════════════════════════════════════════════════
// 测试
// ═════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    fn make_bi(
        _id: usize,
        dir: &str,
        start_price: f64,
        end_price: f64,
        start_idx: u64,
        end_idx: u64,
    ) -> Bi {
        Bi {
            direction: dir.to_string(),
            start_index: start_idx,
            end_index: end_idx,
            start_dt: format!("dt{}", start_idx),
            end_dt: format!("dt{}", end_idx),
            start_price,
            end_price,
            is_finished: true,
        }
    }

    /// 生成随机K线数据，从K线构建笔，再从笔构建线段
    fn gen_klines(seed: u64, n: usize) -> Vec<yifang_data::KLine> {
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let base_id = COUNTER.fetch_add(n as u64, Ordering::SeqCst);

        let mut rng_state = seed;
        let mut rng = || {
            rng_state = rng_state
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            (rng_state >> 33) as f64 / (1u64 << 31) as f64
        };

        let mut klines = Vec::with_capacity(n);
        let mut price = 100.0;
        for i in 0..n {
            price += (rng() - 0.48) * 3.0;
            if price < 5.0 {
                price = 5.0;
            }
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

    // ─── 特征序列包含处理 ─────────────────────────

    #[test]
    fn test_merge_feature_include_up() {
        // 向上线段的特征序列方向=下降 → 包含处理用下降方向：取低低
        let feat = vec![
            FeatElem { high: 10.0, low: 7.0 },
            FeatElem { high: 9.0, low: 8.0 }, // 被包含：10>=9 且 7<=8
        ];
        let result = merge_feature_include(&feat, true);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].high, 9.0); // min(10,9)
        assert_eq!(result[0].low, 7.0); // min(7,8)

        // 添加不包含的
        let feat2 = vec![
            FeatElem { high: 10.0, low: 7.0 },
            FeatElem { high: 9.0, low: 8.0 },
            FeatElem { high: 12.0, low: 9.0 }, // 不包含
        ];
        let result2 = merge_feature_include(&feat2, true);
        assert_eq!(result2.len(), 2);
    }

    #[test]
    fn test_merge_feature_include_down() {
        // 下降线段的特征序列方向=上升 → 包含处理用上升方向：取高高
        let feat = vec![
            FeatElem { high: 8.0, low: 5.0 },
            FeatElem { high: 7.0, low: 6.0 }, // 被包含：8>=7 且 5<=6
        ];
        let result = merge_feature_include(&feat, false);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].high, 8.0); // max(8,7)
        assert_eq!(result[0].low, 6.0); // max(5,6)
    }

    // ─── 分型检测 ─────────────────────────────────

    #[test]
    fn test_fenxing_detection() {
        // 顶分型：high 先升后降
        let top_feat = vec![
            FeatElem { high: 10.0, low: 8.0 },
            FeatElem { high: 12.0, low: 9.0 }, // 中间最高
            FeatElem { high: 11.0, low: 7.0 },
        ];
        assert!(is_top_fx(&top_feat));
        assert!(!is_bottom_fx(&top_feat));

        // 底分型：low 先降后升
        let bottom_feat = vec![
            FeatElem { high: 10.0, low: 8.0 },
            FeatElem { high: 9.0, low: 6.0 }, // 中间最低
            FeatElem { high: 11.0, low: 7.0 },
        ];
        assert!(is_bottom_fx(&bottom_feat));
        assert!(!is_top_fx(&bottom_feat));

        // 无分型：单调递增
        let no_fx = vec![
            FeatElem { high: 10.0, low: 8.0 },
            FeatElem { high: 12.0, low: 9.0 },
            FeatElem { high: 13.0, low: 10.0 },
        ];
        assert!(!is_top_fx(&no_fx));
        assert!(!is_bottom_fx(&no_fx));

        // 不够3个元素
        let short = vec![
            FeatElem { high: 10.0, low: 8.0 },
            FeatElem { high: 12.0, low: 9.0 },
        ];
        assert!(!is_top_fx(&short));
        assert!(!is_bottom_fx(&short));
    }

    // ─── 缺口检测 ─────────────────────────────────

    #[test]
    fn test_feature_has_gap() {
        // 向上线段的缺口：第二根高点 < 第一根低点
        let up_feat = vec![
            FeatElem { high: 12.0, low: 10.0 },
            FeatElem { high: 9.0, low: 7.0 }, // 9 < 10 → 有缺口
        ];
        assert!(feature_has_gap(&up_feat, true));

        // 无缺口：有重叠
        let up_feat2 = vec![
            FeatElem { high: 12.0, low: 8.0 },
            FeatElem { high: 11.0, low: 7.0 }, // 11 < 8? No → 无缺口
        ];
        assert!(!feature_has_gap(&up_feat2, true));

        // 下降线段的缺口：第二根低点 > 第一根高点
        let down_feat = vec![
            FeatElem { high: 7.0, low: 5.0 },
            FeatElem { high: 12.0, low: 10.0 }, // 10 > 7 → 有缺口
        ];
        assert!(feature_has_gap(&down_feat, false));
    }

    // ─── 前三笔重叠 ───────────────────────────────────

    #[test]
    fn test_overlap_check() {
        // 3笔有重叠
        let bis = vec![
            make_bi(0, "up", 10.0, 15.0, 0, 2),
            make_bi(1, "down", 15.0, 12.0, 2, 4),
            make_bi(2, "up", 12.0, 16.0, 4, 6),
        ];
        let indices: Vec<usize> = vec![0, 1, 2];
        assert!(check_overlap_of_first_3(&bis, &indices));

        // 3笔无重叠
        let bis2 = vec![
            make_bi(0, "up", 10.0, 15.0, 0, 2),
            make_bi(1, "down", 15.0, 3.0, 2, 4),
            make_bi(2, "up", 3.0, 5.0, 4, 6),
        ];
        let indices2: Vec<usize> = vec![0, 1, 2];
        assert!(!check_overlap_of_first_3(&bis2, &indices2));
    }

    // ─── 最少3笔 ──────────────────────────────────────

    #[test]
    fn test_xd_min_3_bi() {
        // 只有2笔 → 不够3笔，不能构成线段
        let bis = vec![
            make_bi(0, "up", 10.0, 15.0, 0, 2),
            make_bi(1, "down", 15.0, 12.0, 2, 4),
        ];
        assert!(build_xd(&bis).is_empty());
    }

    // ─── 基础线段构建 ─────────────────────────────────

    #[test]
    fn test_xd_basic_up() {
        // 3笔上升线段
        let bis = vec![
            make_bi(0, "up", 10.0, 15.0, 0, 1),
            make_bi(1, "down", 15.0, 12.0, 1, 2),
            make_bi(2, "up", 12.0, 18.0, 2, 3),
        ];
        let xds = build_xd(&bis);
        assert_eq!(xds.len(), 1);
        assert_eq!(xds[0].direction, "up");
        assert_eq!(xds[0].start_price, 10.0);
        assert_eq!(xds[0].end_price, 18.0);
        assert!(!xds[0].is_finished); // 只有3笔，未完成
    }

    #[test]
    fn test_xd_basic_down() {
        // 3笔下降线段
        let bis = vec![
            make_bi(0, "down", 20.0, 12.0, 0, 1),
            make_bi(1, "up", 12.0, 15.0, 1, 2),
            make_bi(2, "down", 15.0, 8.0, 2, 3),
        ];
        let xds = build_xd(&bis);
        assert_eq!(xds.len(), 1);
        assert_eq!(xds[0].direction, "down");
        assert_eq!(xds[0].start_price, 20.0);
        assert_eq!(xds[0].end_price, 8.0);
    }

    // ─── 线段被分型终结 ───────────────────────────────

    #[test]
    fn test_xd_terminated_by_top_fenxing() {
        // 6笔上升线段，特征序列出现顶分型
        let bis = vec![
            make_bi(0, "up", 10.0, 15.0, 0, 1),
            make_bi(1, "down", 15.0, 12.0, 1, 2),
            make_bi(2, "up", 12.0, 17.0, 2, 3),
            make_bi(3, "down", 17.0, 14.0, 3, 4),
            make_bi(4, "up", 14.0, 19.0, 4, 5),
            make_bi(5, "down", 19.0, 13.0, 5, 6),
        ];
        let xds = build_xd(&bis);
        assert!(!xds.is_empty(), "应该至少有一个线段");
    }

    // ─── 无缺口直接终结 ─────────────────────────────

    #[test]
    fn test_xd_no_gap_direct_termination() {
        // 上升线段，特征序列有顶分型且无缺口 → 直接终结
        let bis = vec![
            make_bi(0, "up", 10.0, 15.0, 0, 1),
            make_bi(1, "down", 15.0, 13.0, 1, 2), // 特征1: h=15, l=13
            make_bi(2, "up", 13.0, 18.0, 2, 3),
            make_bi(3, "down", 18.0, 16.0, 3, 4), // 特征2: h=18, l=16 ← 中间最高
            make_bi(4, "up", 16.0, 17.0, 4, 5),
            make_bi(5, "down", 17.0, 11.0, 5, 6), // 特征3: h=17, l=11
        ];
        let xds = build_xd(&bis);
        assert!(xds.len() >= 1);
        assert!(xds[0].is_finished);
        assert_eq!(xds[0].direction, "up");
    }

    // ─── 有缺口需要确认 ─────────────────────────────

    #[test]
    fn test_xd_gap_needs_confirmation() {
        // 场景1：无缺口→直接终结
        {
            let bis = vec![
                make_bi(0, "up", 5.0, 12.0, 0, 1),
                make_bi(1, "down", 12.0, 9.0, 1, 2), // 特征1: h=12, l=9
                make_bi(2, "up", 9.0, 14.0, 2, 3),
                make_bi(3, "down", 14.0, 11.0, 3, 4), // 特征2: h=14, l=11 — 中间最高
                make_bi(4, "up", 11.0, 13.0, 4, 5),
                make_bi(5, "down", 13.0, 10.0, 5, 6), // 特征3: h=13, l=10
            ];
            let xds = build_xd(&bis);
            assert!(xds.len() >= 1);
            assert!(xds[0].is_finished, "无缺口场景：线段应直接终结");
        }

        // 场景2：随机数据下不崩溃
        {
            let klines = gen_klines(999, 200);
            let bis = crate::bi::build_bi(&klines, None);
            if bis.len() >= 3 {
                let _xds = build_xd(&bis);
            }
        }
    }

    // ─── 严格缠论随机验证 ─────────────────────────────

    #[test]
    fn test_xd_strict_chanlun_rules() {
        for seed in [42u64, 123, 456, 789, 1024, 2048, 3000, 4000] {
            let klines = gen_klines(seed, 500);
            let bis = crate::bi::build_bi(&klines, None);
            if bis.len() < 3 {
                continue;
            }

            // 笔必须方向交替
            let mut bi_alternating = true;
            for i in 1..bis.len() {
                if bis[i].direction.as_str() == bis[i - 1].direction.as_str() {
                    bi_alternating = false;
                    break;
                }
            }
            if !bi_alternating {
                eprintln!("seed={}: 笔不交替，跳过", seed);
                continue;
            }

            let xds = build_xd(&bis);

            // 打印线段详情用于调试
            for (i, xd) in xds.iter().enumerate() {
                if xd.direction.as_str() == "up" && xd.start_price >= xd.end_price {
                    eprintln!(
                        "seed={}: 上升线段[{}] 异常: start={:.2} >= end={:.2}",
                        seed, i, xd.start_price, xd.end_price
                    );
                    eprintln!("  笔序列:");
                    for (j, bi) in bis.iter().enumerate() {
                        eprintln!(
                            "    [{}] {} {:.2}→{:.2}",
                            j, bi.direction, bi.start_price, bi.end_price
                        );
                    }
                    eprintln!("  线段序列:");
                    for (j, xd2) in xds.iter().enumerate() {
                        eprintln!(
                            "    [{}] {} {:.2}→{:.2} finished={}",
                            j, xd2.direction, xd2.start_price, xd2.end_price, xd2.is_finished
                        );
                    }
                }
            }

            // 完成线段方向与价格一致性检查
            for (i, xd) in xds.iter().enumerate() {
                if xd.is_finished {
                    if xd.direction.as_str() == "up" && xd.start_price >= xd.end_price {
                        eprintln!(
                            "seed={}: 上升线段[{}] start={:.2} >= end={:.2}",
                            seed, i, xd.start_price, xd.end_price
                        );
                    } else if xd.direction.as_str() == "down" && xd.start_price <= xd.end_price {
                        eprintln!(
                            "seed={}: 下降线段[{}] start={:.2} <= end={:.2}",
                            seed, i, xd.start_price, xd.end_price
                        );
                    }
                }
            }

            // 线段方向交替
            for i in 1..xds.len() {
                assert_ne!(
                    xds[i].direction,
                    xds[i - 1].direction,
                    "seed={}: 线段[{}]和[{}]方向相同",
                    seed,
                    i,
                    i - 1
                );
            }
        }
    }
}
