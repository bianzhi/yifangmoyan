//! 三类买卖点识别（严格遵循缠论17/20/21课原始定义）
//!
//! ── 缠论原文定义 ──
//!
//! **第一类买卖点**（17课、21课、27课）：
//! - 一买：下跌趋势（≥2个同级别中枢且方向向下递进）最后一个中枢后出现的趋势底背驰点
//! - 一卖：上涨趋势（≥2个同级别中枢且方向向上递进）最后一个中枢后出现的趋势顶背驰点
//! - **铁律**：盘整背驰绝对不能产生第一类买卖点（27课明确：趋势背驰产生一类买卖点）
//!
//! **第二类买卖点**（21课）：
//! - 21课原文："第一买点出现后的第二段次级别走势低点就构成第二类买点"
//! - 二买 = 一买后，第一段向上走势（离开段）+ 第二段向下走势（回抽段）的低点
//! - 二卖 = 一卖后，第一段向下走势（离开段）+ 第二段向上走势（回抽段）的高点
//! - 21课原文**未附加**"不破一买/一卖价格"的约束，此处比原文更严格：
//!   - 回抽不破一买价 → 标准二买("2buy")
//!   - 回抽破了一买价 → 破位二买("2buy_break")，表示一买可能误判
//!
//! **第三类买卖点**（17课、20课、21课）：
//! - 三买：某级别中枢之上，次级别走势离开该中枢后，次级别走势回抽不回到中枢区间[Zd, Zg]
//!   即：回抽走势的低点 >= 中枢Zg（上沿），原文"不跌破ZG"包含等于的情况
//! - 三卖：某级别中枢之下，次级别走势离开该中枢后，次级别走势回抽不回到中枢区间[Zd, Zg]
//!   即：回抽走势的高点 <= 中枢Zd（下沿），原文"不升破ZD"包含等于的情况
//! - 关键：离开段必须实际脱离中枢（三买：离开段高点>Zg，三卖：离开段低点<Zd）
//!
//! **二三买/二三卖重合**（21课）：
//! - 21课原文："第一类买点出现后，一个次级别的走势凌厉地直接上破前面下跌的最后一个中枢，
//!   然后在其上产生一个次级别的回抽不触及该中枢，这时候，就会出现第二类买点与第三类买点重合的情况"
//! - 一旦出现重合，"一个大级别的上涨往往就会出现" → 标记为"2+3buy"/"2+3sell"
use yifang_data::{BeiChi, Bi, BuySellPoint, XianDuan, ZhongShu};

// ─── 公开接口 ──────────────────────────────────────────

/// 识别笔级别买卖点
pub fn detect_buy_sell(
    bis: &[Bi],
    bi_zs: &[ZhongShu],
    beichi: &[BeiChi],
) -> Vec<BuySellPoint> {
    detect_buy_sell_from_segments(
        bis,
        bi_zs,
        beichi,
        "bi_beichi",
        |bi: &Bi| (bi.start_index, bi.end_index, bi.direction.clone(), bi.start_price, bi.end_price),
    )
}

/// 识别线段级别买卖点
pub fn detect_xd_buy_sell(
    xds: &[XianDuan],
    xd_zs: &[ZhongShu],
    xd_beichi: &[BeiChi],
) -> Vec<BuySellPoint> {
    detect_buy_sell_from_segments(
        xds,
        xd_zs,
        xd_beichi,
        "xd_beichi",
        |xd: &XianDuan| (xd.start_index, xd.end_index, xd.direction.clone(), xd.start_price, xd.end_price),
    )
}

// ─── 核心实现 ──────────────────────────────────────────

fn detect_buy_sell_from_segments<T, F>(
    segments: &[T],
    zs_list: &[ZhongShu],
    beichi: &[BeiChi],
    bc_type_filter: &str,
    extract: F,
) -> Vec<BuySellPoint>
where
    F: Fn(&T) -> (u64, u64, String, f64, f64),
{
    let mut results = Vec::new();

    if segments.len() < 3 || zs_list.is_empty() {
        return results;
    }

    let seg_infos: Vec<SegInfo> = segments
        .iter()
        .map(|s| {
            let (start_idx, end_idx, direction, start_val, end_val) = extract(s);
            SegInfo { start_idx, end_idx, direction, start_val, end_val }
        })
        .collect();

    // 只取趋势背驰（盘整背驰不产生一类买卖点）
    let trend_bds: Vec<&BeiChi> = beichi
        .iter()
        .filter(|bd| bd.bc_type == bc_type_filter && bd.bc_sub_type == "trend")
        .collect();

    // 第一类买卖点：趋势背驰产生
    let (buy1_list, sell1_list) = find_buy1_sell1(&seg_infos, zs_list, &trend_bds);
    results.extend(buy1_list);
    results.extend(sell1_list);

    // 第二类买卖点：一买/一卖后的回抽不破前低/前高
    let (buy2_list, sell2_list) = find_buy2_sell2(&seg_infos, &results);
    results.extend(buy2_list);
    results.extend(sell2_list);

    // 第三类买卖点：离开中枢后回抽不回中枢
    let (buy3_list, sell3_list) = find_buy3_sell3(&seg_infos, zs_list);
    results.extend(buy3_list);
    results.extend(sell3_list);

    // 检测二三买/二三卖重合（21课原文明确重视此信号）
    // 当二买和三买在同一位置(index相同)时，合并为"2+3buy"
    // 当二卖和三卖在同一位置(index相同)时，合并为"2+3sell"
    results = merge_overlapping_2_3(results);

    results
}

// ─── 段信息 ───────────────────────────────────────────

struct SegInfo {
    start_idx: u64,
    end_idx: u64,
    direction: String,
    start_val: f64,
    end_val: f64,
}

// ─── 第一类买卖点 ─────────────────────────────────────

/// 第一类买卖点：趋势背驰产生
/// - 下跌趋势底背驰 → 一买
/// - 上涨趋势顶背驰 → 一卖
///
/// 缠论原文（17课）：
/// "趋势背驰产生第一类买卖点"
/// 缠论原文（21课）：
/// "盘整背驰...转化为第三类买卖点"（不是第一类！）
fn find_buy1_sell1(
    seg_infos: &[SegInfo],
    zs_list: &[ZhongShu],
    trend_bds: &[&BeiChi],
) -> (Vec<BuySellPoint>, Vec<BuySellPoint>) {
    let mut buy1_list = Vec::new();
    let mut sell1_list = Vec::new();

    for bd in trend_bds {
        // 找到背驰点之前结束的中枢索引
        let related_indices: Vec<usize> = zs_list
            .iter()
            .enumerate()
            .filter(|(_, zs)| zs.end_index <= bd.index)
            .map(|(i, _)| i)
            .collect();

        if related_indices.len() < 2 {
            continue;
        }

        // 将中枢按趋势方向分组（同方向递进为同一趋势）
        let groups = group_zs_indices_by_trend(zs_list, &related_indices);

        for group in &groups {
            // 趋势至少含2个递进中枢
            if group.len() < 2 {
                continue;
            }

            let trend_dir = classify_trend_direction_from_indices(zs_list, group);

            // 下跌趋势 + 底背驰 → 一买
            if trend_dir == "down" && bd.direction == "down" {
                let price = find_seg_end_price(seg_infos, bd.index);
                buy1_list.push(BuySellPoint {
                    bs_type: "1buy".to_string(),
                    index: bd.index,
                    dt: bd.dt.clone(),
                    price,
                });
            }
            // 上涨趋势 + 顶背驰 → 一卖
            else if trend_dir == "up" && bd.direction == "up" {
                let price = find_seg_end_price(seg_infos, bd.index);
                sell1_list.push(BuySellPoint {
                    bs_type: "1sell".to_string(),
                    index: bd.index,
                    dt: bd.dt.clone(),
                    price,
                });
            }
        }
    }

    (buy1_list, sell1_list)
}

// ─── 第二类买卖点 ─────────────────────────────────────

/// 第二类买卖点（缠论21课）
///
/// 缠论原文（21课）：
/// "第一买点出现后的第二段次级别走势低点就构成第二类买点"
///
/// 原文**未附加**"不破一买/一卖价格"的约束，但此处按更严格的实践标准区分：
/// - 回抽不破一买价 → 标准二买("2buy")
/// - 回抽破了一买价 → 破位二买("2buy_break")，表示一买可能误判，趋势或许还在延续
fn find_buy2_sell2(
    seg_infos: &[SegInfo],
    first_points: &[BuySellPoint],
) -> (Vec<BuySellPoint>, Vec<BuySellPoint>) {
    let mut buy2_list = Vec::new();
    let mut sell2_list = Vec::new();

    for buy1 in first_points.iter().filter(|p| p.bs_type == "1buy") {
        if let Some(buy2) = find_buy2_after_buy1(seg_infos, buy1) {
            buy2_list.push(buy2);
        }
    }

    for sell1 in first_points.iter().filter(|p| p.bs_type == "1sell") {
        if let Some(sell2) = find_sell2_after_sell1(seg_infos, sell1) {
            sell2_list.push(sell2);
        }
    }

    (buy2_list, sell2_list)
}

/// 二买：一买后的第一个向下回抽段的低点
///
/// 21课原文："第一买点出现后的第二段次级别走势低点就构成第二类买点"
/// 一买之后，走势首先向上离开（离开段），然后向下回抽（回抽段）。
/// 回抽段的终点就是二买位置。
///
/// 按实践严格标准区分：
/// - pullback_low >= buy1.price → 标准二买("2buy")
/// - pullback_low < buy1.price → 破位二买("2buy_break")
fn find_buy2_after_buy1(seg_infos: &[SegInfo], buy1: &BuySellPoint) -> Option<BuySellPoint> {
    // 从一买位置开始找后续段
    let after_segs: Vec<&SegInfo> = seg_infos.iter().filter(|s| s.start_idx >= buy1.index).collect();
    if after_segs.len() < 2 { return None; }

    // 找第一个向上离开段 + 紧随的向下回抽段
    let mut found_up = false;
    for seg in &after_segs {
        if !found_up && seg.direction == "up" {
            found_up = true;
            continue;
        }
        if found_up && seg.direction == "down" {
            // 回抽段的终点价格（段是向下的，终点是低点）
            let pullback_low = seg.end_val;
            // 21课原文仅说"第二段低点就是二买"，这里按实践严格标准区分
            let bs_type = if pullback_low >= buy1.price {
                "2buy" // 不破一买：标准二买
            } else {
                "2buy_break" // 破一买：破位二买（一买可能误判）
            };
            return Some(BuySellPoint {
                bs_type: bs_type.to_string(),
                index: seg.end_idx,
                dt: String::new(),
                price: pullback_low,
            });
        }
    }
    None
}

/// 二卖：一卖后的第一个向上回抽段的高点
///
/// 21课原文对称定义：一卖后第二段次级别走势高点就是二卖
/// 按实践严格标准区分：
/// - pullback_high <= sell1.price → 标准二卖("2sell")
/// - pullback_high > sell1.price → 破位二卖("2sell_break")
fn find_sell2_after_sell1(seg_infos: &[SegInfo], sell1: &BuySellPoint) -> Option<BuySellPoint> {
    let after_segs: Vec<&SegInfo> = seg_infos.iter().filter(|s| s.start_idx >= sell1.index).collect();
    if after_segs.len() < 2 { return None; }

    let mut found_down = false;
    for seg in &after_segs {
        if !found_down && seg.direction == "down" {
            found_down = true;
            continue;
        }
        if found_down && seg.direction == "up" {
            // 回抽段的终点价格（段是向上的，终点是高点）
            let pullback_high = seg.end_val;
            // 21课原文仅说"第二段高点就是二卖"，这里按实践严格标准区分
            let bs_type = if pullback_high <= sell1.price {
                "2sell" // 不破一卖：标准二卖
            } else {
                "2sell_break" // 破一卖：破位二卖（一卖可能误判）
            };
            return Some(BuySellPoint {
                bs_type: bs_type.to_string(),
                index: seg.end_idx,
                dt: String::new(),
                price: pullback_high,
            });
        }
    }
    None
}

// ─── 第三类买卖点 ─────────────────────────────────────

/// 第三类买卖点（缠论17课、20课、21课）
///
/// 缠论原文（20课）：
/// "第三类买卖点：中枢之上，次级别离开后次级别回抽不回到中枢"
///
/// 严格定义（17课原文"不跌破ZG"/"不升破ZD"包含等于）：
/// - 三买 = 有一个段离开中枢向上（段高点 > Zg），
///   之后有一个向下段回抽，回抽低点 >= Zg（不跌破中枢上沿，含等于）
/// - 三卖 = 有一个段离开中枢向下（段低点 < Zd），
///   之后有一个向上段回抽，回抽高点 <= Zd（不升破中枢下沿，含等于）
///
/// 关键：离开和回抽必须是完整的段（笔/线段）
/// 扫描中枢后所有段对，找到"离开+回抽"组合
fn find_buy3_sell3(
    seg_infos: &[SegInfo],
    zs_list: &[ZhongShu],
) -> (Vec<BuySellPoint>, Vec<BuySellPoint>) {
    let mut buy3_list = Vec::new();
    let mut sell3_list = Vec::new();

    for zs in zs_list {
        // 中枢结束后的所有段
        let after_segs: Vec<&SegInfo> = seg_infos.iter()
            .filter(|s| s.start_idx >= zs.end_index)
            .collect();
        if after_segs.len() < 2 { continue; }

        // 扫描所有段对：离开段 + 回抽段
        // 三买：需要找到一个向上的离开段（高点>Zg），其后紧接一个向下回抽段（低点>Zg）
        // 三卖：需要找到一个向下的离开段（低点<Zd），其后紧接一个向上回抽段（高点<Zd）

        for i in 0..after_segs.len() - 1 {
            let leave_seg = after_segs[i];
            let back_seg = after_segs[i + 1];

            // 三买：向上离开 + 向下回抽
            if leave_seg.direction == "up" && back_seg.direction == "down" {
                let leave_high = leave_seg.start_val.max(leave_seg.end_val);
                if leave_high > zs.zg {
                    // 离开段确实脱离了中枢上沿
                    let back_low = back_seg.start_val.min(back_seg.end_val);
                    if back_low >= zs.zg {
                        // 回抽不破中枢上沿（含等于）→ 三买
                        buy3_list.push(BuySellPoint {
                            bs_type: "3buy".to_string(),
                            index: back_seg.end_idx,
                            dt: String::new(),
                            price: back_low,
                        });
                    }
                    // 无论回抽是否成功，离开段已经脱离中枢，
                    // 后续的段对可能构成新的三买，继续扫描
                }
            }

            // 三卖：向下离开 + 向上回抽
            if leave_seg.direction == "down" && back_seg.direction == "up" {
                let leave_low = leave_seg.start_val.min(leave_seg.end_val);
                if leave_low < zs.zd {
                    // 离开段确实脱离了中枢下沿
                    let back_high = back_seg.start_val.max(back_seg.end_val);
                    if back_high <= zs.zd {
                        // 回抽不破中枢下沿（含等于）→ 三卖
                        sell3_list.push(BuySellPoint {
                            bs_type: "3sell".to_string(),
                            index: back_seg.end_idx,
                            dt: String::new(),
                            price: back_high,
                        });
                    }
                }
            }
        }
    }

    (buy3_list, sell3_list)
}

// ─── 中枢趋势分组辅助 ─────────────────────────────────

/// 将中枢索引按方向递进分组：
/// 连续同方向递进（上涨递进：后Zd > 前Zg，下跌递进：后Zg < 前Zd）归为一组，
/// 方向改变或有重叠则开始新组。
fn group_zs_indices_by_trend(zs_list: &[ZhongShu], indices: &[usize]) -> Vec<Vec<usize>> {
    if indices.is_empty() { return Vec::new(); }
    if indices.len() == 1 { return vec![vec![indices[0]]]; }

    let mut current_dir: Option<&str> = None;

    // 先找到第一个能确定方向的中枢对
    for i in 1..indices.len() {
        let prev = &zs_list[indices[i - 1]];
        let curr = &zs_list[indices[i]];
        if curr.zd > prev.zg {
            current_dir = Some("up");
            break;
        } else if curr.zg < prev.zd {
            current_dir = Some("down");
            break;
        }
    }

    let mut groups: Vec<Vec<usize>> = vec![vec![indices[0]]];

    for i in 1..indices.len() {
        let prev = &zs_list[indices[i - 1]];
        let curr = &zs_list[indices[i]];

        let pair_dir = if curr.zd > prev.zg {
            Some("up")
        } else if curr.zg < prev.zd {
            Some("down")
        } else {
            None
        };

        let same = match (current_dir, pair_dir) {
            (Some(d1), Some(d2)) => d1 == d2,
            _ => false,
        };

        if same {
            groups.last_mut().unwrap().push(indices[i]);
        } else {
            groups.push(vec![indices[i]]);
            current_dir = pair_dir;
        }
    }

    groups
}

/// 判断一组递进中枢的方向
fn classify_trend_direction_from_indices(zs_list: &[ZhongShu], group: &[usize]) -> String {
    if group.len() < 2 { return "unknown".to_string(); }
    let first = &zs_list[group[0]];
    let last = &zs_list[group[group.len() - 1]];
    if last.zd > first.zg { "up".to_string() }
    else if last.zg < first.zd { "down".to_string() }
    else { "unknown".to_string() }
}

/// 根据K线索引找到对应段的终点价格
fn find_seg_end_price(seg_infos: &[SegInfo], index: u64) -> f64 {
    seg_infos.iter()
        .find(|s| s.end_idx == index)
        .map(|s| s.end_val)
        .unwrap_or(0.0)
}

// ─── 二三买卖点重合检测 ────────────────────────────────

/// 检测二三买/二三卖重合（21课原文）
///
/// 21课原文："第一类买点出现后，一个次级别的走势凌厉地直接上破前面下跌的最后一个中枢，
/// 然后在其上产生一个次级别的回抽不触及该中枢，这时候，就会出现第二类买点与第三类买点重合的情况"
///
/// 检测规则：
/// - 二买(index=i) + 三买(index=i) → 合并为"2+3buy"
/// - 二卖(index=i) + 三卖(index=i) → 合并为"2+3sell"
/// - 也支持 "2buy_break" 与 "3buy" 重合 → "2+3buy_break"
fn merge_overlapping_2_3(mut points: Vec<BuySellPoint>) -> Vec<BuySellPoint> {
    // 找出所有重合位置：同一index同时出现2buy(或2buy_break)和3buy（或2sell和3sell）
    let mut merge_indices: Vec<(usize, usize)> = Vec::new(); // (2x_index, 3x_index) in points vec

    for i in 0..points.len() {
        let pi = &points[i];
        let is_buy2 = pi.bs_type == "2buy" || pi.bs_type == "2buy_break";
        let is_sell2 = pi.bs_type == "2sell" || pi.bs_type == "2sell_break";
        if !is_buy2 && !is_sell2 { continue; }

        for j in 0..points.len() {
            if i == j { continue; }
            let pj = &points[j];
            if pi.index != pj.index { continue; }

            let is_overlap = (is_buy2 && pj.bs_type == "3buy")
                || (is_sell2 && pj.bs_type == "3sell");
            if is_overlap {
                merge_indices.push((i, j));
            }
        }
    }

    // 合并：把2x和3x合并为"2+3x"，移除3x条目
    let mut remove_set: std::collections::HashSet<usize> = std::collections::HashSet::new();
    for (idx_2x, idx_3x) in &merge_indices {
        let new_type = if points[*idx_2x].bs_type.starts_with("2buy") {
            if points[*idx_2x].bs_type == "2buy_break" {
                "2+3buy_break"
            } else {
                "2+3buy"
            }
        } else {
            if points[*idx_2x].bs_type == "2sell_break" {
                "2+3sell_break"
            } else {
                "2+3sell"
            }
        };
        points[*idx_2x].bs_type = new_type.to_string();
        remove_set.insert(*idx_3x);
    }

    // 移除被合并的3x条目
    let mut removed: Vec<BuySellPoint> = Vec::new();
    for (i, p) in points.into_iter().enumerate() {
        if !remove_set.contains(&i) {
            removed.push(p);
        }
    }

    removed
}

// ─── 测试 ─────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use yifang_data::{Bi, XianDuan, ZhongShu, BeiChi};

    fn make_bi(_id: usize, dir: &str, start: f64, end: f64, si: u64, ei: u64) -> Bi {
        Bi { direction: dir.to_string(), start_price: start, end_price: end,
             start_index: si, end_index: ei, start_dt: String::new(), end_dt: String::new(),
             is_finished: true }
    }
    fn make_xd(_id: usize, dir: &str, start: f64, end: f64, si: u64, ei: u64) -> XianDuan {
        XianDuan { direction: dir.to_string(), start_price: start, end_price: end,
                   start_index: si, end_index: ei, start_dt: String::new(), end_dt: String::new(),
                   is_finished: true }
    }
    fn make_zs(zt: &str, si: u64, ei: u64, zg: f64, zd: f64) -> ZhongShu {
        ZhongShu { zs_type: zt.to_string(), start_index: si, end_index: ei,
                   start_dt: String::new(), end_dt: String::new(),
                   zg, zd, gg: zg + 1.0, dd: (zd - 1.0).max(0.0) }
    }
    fn make_bc(bct: &str, idx: u64, dir: &str, sub: &str) -> BeiChi {
        BeiChi { bc_type: bct.to_string(), index: idx, dt: String::new(),
                 direction: dir.to_string(), bc_sub_type: sub.to_string(), reason: String::new() }
    }

    // ─── 一买测试 ───

    #[test]
    fn test_first_buy_from_trend_beichi() {
        // 下跌趋势：2个向下递进中枢 + 趋势底背驰
        let bis = vec![
            make_bi(0,"up",20.,25.,0,3), make_bi(1,"down",25.,18.,3,6),
            make_bi(2,"up",18.,22.,6,9), make_bi(3,"down",22.,19.,9,12),
            make_bi(4,"up",19.,16.,12,15), make_bi(5,"down",16.,10.,15,18),
            make_bi(6,"up",10.,13.,18,21), make_bi(7,"down",13.,11.,21,24),
            make_bi(8,"down",11.,8.,24,27),
        ];
        // ZS1: [19,22] ZS2: [11,13] — 下跌递进（ZS2.zg=13 < ZS1.zd=19）
        let zs = vec![make_zs("bi_zs",6,12,22.,19.), make_zs("bi_zs",18,24,13.,11.)];
        let bc = vec![make_bc("bi_beichi",27,"down","trend")];
        let pts = detect_buy_sell(&bis, &zs, &bc);
        let b1: Vec<_> = pts.iter().filter(|p| p.bs_type == "1buy").collect();
        assert!(!b1.is_empty(), "应检测到一买");
        assert_eq!(b1[0].index, 27);
    }

    #[test]
    fn test_first_sell_from_trend_beichi() {
        // 上涨趋势：2个向上递进中枢 + 趋势顶背驰
        let bis = vec![
            make_bi(0,"up",10.,15.,0,3), make_bi(1,"down",15.,12.,3,6),
            make_bi(2,"up",12.,14.,6,9), make_bi(3,"down",14.,13.,9,12),
            make_bi(4,"up",13.,22.,12,15), make_bi(5,"down",22.,17.,15,18),
            make_bi(6,"up",17.,19.,18,21), make_bi(7,"down",19.,18.,21,24),
            make_bi(8,"up",18.,23.,24,27),
        ];
        let zs = vec![make_zs("bi_zs",3,12,14.,12.), make_zs("bi_zs",15,24,19.,17.)];
        let bc = vec![make_bc("bi_beichi",27,"up","trend")];
        let pts = detect_buy_sell(&bis, &zs, &bc);
        let s1: Vec<_> = pts.iter().filter(|p| p.bs_type == "1sell").collect();
        assert!(!s1.is_empty(), "应检测到一卖");
        assert_eq!(s1[0].index, 27);
    }

    #[test]
    fn test_no_first_buy_from_panzheng() {
        // 盘整背驰不应产生一买（铁律）
        let bis = vec![
            make_bi(0,"up",10.,15.,0,3), make_bi(1,"down",15.,12.,3,6),
            make_bi(2,"up",12.,14.,6,9), make_bi(3,"down",14.,13.,9,12),
            make_bi(4,"up",13.,14.,12,15),
        ];
        let zs = vec![make_zs("bi_zs",3,12,14.,12.)];
        let bc = vec![make_bc("bi_beichi",15,"up","panzheng")];
        let pts = detect_buy_sell(&bis, &zs, &bc);
        let b1: Vec<_> = pts.iter().filter(|p| p.bs_type == "1buy").collect();
        assert!(b1.is_empty(), "盘整背驰不应产生一买");
    }

    #[test]
    fn test_no_first_with_single_zs() {
        // 只有1个中枢，无趋势，不应产生一类买卖点
        let bis = vec![
            make_bi(0,"up",10.,15.,0,3), make_bi(1,"down",15.,12.,3,6),
            make_bi(2,"up",12.,14.,6,9), make_bi(3,"down",14.,13.,9,12),
        ];
        let zs = vec![make_zs("bi_zs",3,12,14.,12.)];
        let bc = vec![make_bc("bi_beichi",12,"up","trend")];
        let pts = detect_buy_sell(&bis, &zs, &bc);
        let b1: Vec<_> = pts.iter().filter(|p| p.bs_type == "1buy").collect();
        let s1: Vec<_> = pts.iter().filter(|p| p.bs_type == "1sell").collect();
        assert!(b1.is_empty() && s1.is_empty(), "单中枢不应产生一类买卖点");
    }

    // ─── 二买测试 ───

    #[test]
    fn test_second_buy_after_first_buy() {
        // 一买后回调不破一买低点 → 二买
        let bis = vec![
            make_bi(0,"up",20.,25.,0,3), make_bi(1,"down",25.,18.,3,6),
            make_bi(2,"up",18.,22.,6,9), make_bi(3,"down",22.,19.,9,12),
            make_bi(4,"up",19.,16.,12,15), make_bi(5,"down",16.,10.,15,18),
            make_bi(6,"up",10.,13.,18,21), make_bi(7,"down",13.,11.,21,24),
            make_bi(8,"down",11.,8.,24,27),
            // 一买在idx=27, price=8.0
            // 之后：向上段 + 向下回调
            make_bi(9,"up",8.,12.,27,30),     // 离开段
            make_bi(10,"down",12.,9.,30,33),  // 回抽段，低点9.0 > 8.0 → 二买
        ];
        let zs = vec![make_zs("bi_zs",6,12,22.,19.), make_zs("bi_zs",18,24,13.,11.)];
        let bc = vec![make_bc("bi_beichi",27,"down","trend")];
        let pts = detect_buy_sell(&bis, &zs, &bc);
        let b2: Vec<_> = pts.iter().filter(|p| p.bs_type == "2buy").collect();
        assert!(!b2.is_empty(), "应检测到二买");
        assert_eq!(b2[0].index, 33);
        assert!(b2[0].price >= 8.0, "二买价格应>=一买价格");
    }

    #[test]
    fn test_second_sell_after_first_sell() {
        // 一卖后反弹不过一卖高点 → 二卖
        let bis = vec![
            make_bi(0,"up",10.,15.,0,3), make_bi(1,"down",15.,12.,3,6),
            make_bi(2,"up",12.,14.,6,9), make_bi(3,"down",14.,13.,9,12),
            make_bi(4,"up",13.,22.,12,15), make_bi(5,"down",22.,17.,15,18),
            make_bi(6,"up",17.,19.,18,21), make_bi(7,"down",19.,18.,21,24),
            make_bi(8,"up",18.,23.,24,27),
            // 一卖在idx=27, price=23.0
            // 之后：向下段 + 向上反弹
            make_bi(9,"down",23.,19.,27,30),   // 离开段
            make_bi(10,"up",19.,21.,30,33),    // 反弹段，高点21.0 < 23.0 → 二卖
        ];
        let zs = vec![make_zs("bi_zs",3,12,14.,12.), make_zs("bi_zs",15,24,19.,17.)];
        let bc = vec![make_bc("bi_beichi",27,"up","trend")];
        let pts = detect_buy_sell(&bis, &zs, &bc);
        let s2: Vec<_> = pts.iter().filter(|p| p.bs_type == "2sell").collect();
        assert!(!s2.is_empty(), "应检测到二卖");
        assert_eq!(s2[0].index, 33);
    }

    #[test]
    fn test_second_buy_equal_to_first_buy() {
        // 二买价格等于一买价格（双底） → 仍然是二买
        let bis = vec![
            make_bi(0,"up",20.,25.,0,3), make_bi(1,"down",25.,18.,3,6),
            make_bi(2,"up",18.,22.,6,9), make_bi(3,"down",22.,19.,9,12),
            make_bi(4,"up",19.,16.,12,15), make_bi(5,"down",16.,10.,15,18),
            make_bi(6,"up",10.,13.,18,21), make_bi(7,"down",13.,11.,21,24),
            make_bi(8,"down",11.,8.,24,27),
            // 一买: idx=27, price=8.0
            make_bi(9,"up",8.,12.,27,30),
            make_bi(10,"down",12.,8.,30,33),  // 回调到8.0 = 一买价 → 仍算二买
        ];
        let zs = vec![make_zs("bi_zs",6,12,22.,19.), make_zs("bi_zs",18,24,13.,11.)];
        let bc = vec![make_bc("bi_beichi",27,"down","trend")];
        let pts = detect_buy_sell(&bis, &zs, &bc);
        let b2: Vec<_> = pts.iter().filter(|p| p.bs_type == "2buy").collect();
        assert!(!b2.is_empty(), "双底应是二买");
    }

    // ─── 三买测试 ───

    #[test]
    fn test_third_buy() {
        // 离开中枢向上 + 回抽不破中枢上沿 → 三买
        let bis = vec![
            make_bi(0,"up",10.,15.,0,3), make_bi(1,"down",15.,12.,3,6),
            make_bi(2,"up",12.,14.,6,9), make_bi(3,"down",14.,13.,9,12),
            // ZS: [12,14], zg=14, zd=12
            make_bi(4,"up",13.,20.,12,15),   // 向上离开，高点20>Zg=14
            make_bi(5,"down",20.,15.,15,18), // 回抽低点15>Zg=14 → 三买
        ];
        let zs = vec![make_zs("bi_zs",3,12,14.,12.)];
        let pts = detect_buy_sell(&bis, &zs, &[]);
        let b3: Vec<_> = pts.iter().filter(|p| p.bs_type == "3buy").collect();
        assert!(!b3.is_empty(), "应检测到三买");
        assert_eq!(b3[0].index, 18);
    }

    #[test]
    fn test_third_sell() {
        // 离开中枢向下 + 回抽不破中枢下沿 → 三卖
        let bis = vec![
            make_bi(0,"down",25.,20.,0,3), make_bi(1,"up",20.,23.,3,6),
            make_bi(2,"down",23.,21.,6,9), make_bi(3,"up",21.,22.,9,12),
            // ZS: [21,23], zg=23, zd=21
            make_bi(4,"down",22.,15.,12,15), // 向下离开，低点15<Zd=21
            make_bi(5,"up",15.,19.,15,18),   // 回抽高点19<Zd=21 → 三卖
        ];
        let zs = vec![make_zs("bi_zs",3,12,23.,21.)];
        let pts = detect_buy_sell(&bis, &zs, &[]);
        let s3: Vec<_> = pts.iter().filter(|p| p.bs_type == "3sell").collect();
        assert!(!s3.is_empty(), "应检测到三卖");
        assert_eq!(s3[0].index, 18);
    }

    #[test]
    fn test_no_third_buy_when_pullback_enters_zs() {
        // 回抽进入中枢区间 → 不构成三买
        let bis = vec![
            make_bi(0,"up",10.,15.,0,3), make_bi(1,"down",15.,12.,3,6),
            make_bi(2,"up",12.,14.,6,9), make_bi(3,"down",14.,13.,9,12),
            // ZS: [12,14], zg=14, zd=12
            make_bi(4,"up",13.,20.,12,15),    // 离开
            make_bi(5,"down",20.,13.,15,18),  // 回抽低点13 < Zg=14 → 不构成三买
        ];
        let zs = vec![make_zs("bi_zs",3,12,14.,12.)];
        let pts = detect_buy_sell(&bis, &zs, &[]);
        let b3: Vec<_> = pts.iter().filter(|p| p.bs_type == "3buy").collect();
        assert!(b3.is_empty(), "回抽进入中枢区间不应产生三买");
    }

    #[test]
    fn test_third_buy_from_later_segs() {
        // 中枢后有多段震荡，真正的三买在后面
        let bis = vec![
            make_bi(0,"up",10.,15.,0,3), make_bi(1,"down",15.,12.,3,6),
            make_bi(2,"up",12.,14.,6,9), make_bi(3,"down",14.,13.,9,12),
            // ZS: [12,14], zg=14, zd=12
            make_bi(4,"up",13.,14.,12,15),     // 震荡段（未脱离中枢）
            make_bi(5,"down",14.,13.,15,18),   // 震荡段（回到中枢内）
            make_bi(6,"up",13.,20.,18,21),     // 真正的离开段（高点20 > Zg=14）
            make_bi(7,"down",20.,15.,21,24),   // 回抽低点15 > Zg=14 → 三买
        ];
        let zs = vec![make_zs("bi_zs",3,12,14.,12.)];
        let pts = detect_buy_sell(&bis, &zs, &[]);
        let b3: Vec<_> = pts.iter().filter(|p| p.bs_type == "3buy").collect();
        assert!(!b3.is_empty(), "应检测到三买（来自后续段对）");
        assert_eq!(b3[0].index, 24);
    }

    #[test]
    fn test_no_third_buy_when_leave_doesnt_escape() {
        // 离开段高点 <= Zg，未脱离中枢 → 不构成三买
        let bis = vec![
            make_bi(0,"up",10.,15.,0,3), make_bi(1,"down",15.,12.,3,6),
            make_bi(2,"up",12.,14.,6,9), make_bi(3,"down",14.,13.,9,12),
            // ZS: [12,14], zg=14
            make_bi(4,"up",13.,14.,12,15),    // 离开段高点14 = Zg，未脱离
            make_bi(5,"down",14.,13.,15,18),
        ];
        let zs = vec![make_zs("bi_zs",3,12,14.,12.)];
        let pts = detect_buy_sell(&bis, &zs, &[]);
        let b3: Vec<_> = pts.iter().filter(|p| p.bs_type == "3buy").collect();
        assert!(b3.is_empty(), "离开段未脱离中枢不应产生三买");
    }

    // ─── 线段级别测试 ───

    #[test]
    fn test_xd_buy_sell() {
        let xds = vec![
            make_xd(0,"up",10.,15.,0,3), make_xd(1,"down",15.,12.,3,6),
            make_xd(2,"up",12.,14.,6,9), make_xd(3,"down",14.,13.,9,12),
            make_xd(4,"up",13.,20.,12,15), make_xd(5,"down",20.,15.,15,18),
        ];
        let zs = vec![make_zs("xd_zs",3,12,14.,12.)];
        let pts = detect_xd_buy_sell(&xds, &zs, &[]);
        let b3: Vec<_> = pts.iter().filter(|p| p.bs_type == "3buy").collect();
        assert!(!b3.is_empty(), "线段级别应检测到三买");
    }

    // ─── 边界条件 ───

    #[test]
    fn test_empty_zs_no_points() {
        let bis = vec![
            make_bi(0,"up",10.,15.,0,3), make_bi(1,"down",15.,12.,3,6),
        ];
        let pts = detect_buy_sell(&bis, &[], &[]);
        assert!(pts.is_empty());
    }

    #[test]
    fn test_few_segments_no_points() {
        let bis = vec![make_bi(0,"up",10.,15.,0,3)];
        let zs = vec![make_zs("bi_zs",0,3,15.,10.)];
        let pts = detect_buy_sell(&bis, &zs, &[]);
        assert!(pts.is_empty());
    }

    // ─── 中枢分组测试 ───

    #[test]
    fn test_group_zs_indices_by_trend() {
        // 两个向上递进中枢应分为一组
        let zs = vec![
            make_zs("bi_zs",3,12,14.,12.),   // ZS1
            make_zs("bi_zs",15,24,19.,17.),   // ZS2: zd=17 > zg=14 → 上涨递进
        ];
        let indices = vec![0usize, 1];
        let groups = group_zs_indices_by_trend(&zs, &indices);
        assert_eq!(groups.len(), 1, "上涨递进应为一组");
        assert_eq!(groups[0].len(), 2);
    }

    #[test]
    fn test_classify_trend_direction() {
        let zs = vec![
            make_zs("bi_zs",3,12,14.,12.),
            make_zs("bi_zs",15,24,19.,17.),
        ];
        let dir = classify_trend_direction_from_indices(&zs, &[0, 1]);
        assert_eq!(dir, "up", "应为上涨趋势");

        let zs2 = vec![
            make_zs("bi_zs",3,12,22.,19.),
            make_zs("bi_zs",15,24,13.,11.),
        ];
        let dir2 = classify_trend_direction_from_indices(&zs2, &[0, 1]);
        assert_eq!(dir2, "down", "应为下跌趋势");
    }

    // ─── 二买破位测试 ───

    #[test]
    fn test_second_buy_break_when_pullback_below_first_buy() {
        // 一买后回抽破了一买价格 → 破位二买("2buy_break")
        let bis = vec![
            make_bi(0,"up",20.,25.,0,3), make_bi(1,"down",25.,18.,3,6),
            make_bi(2,"up",18.,22.,6,9), make_bi(3,"down",22.,19.,9,12),
            make_bi(4,"up",19.,16.,12,15), make_bi(5,"down",16.,10.,15,18),
            make_bi(6,"up",10.,13.,18,21), make_bi(7,"down",13.,11.,21,24),
            make_bi(8,"down",11.,8.,24,27),
            // 一买: idx=27, price=8.0
            make_bi(9,"up",8.,12.,27,30),       // 离开段
            make_bi(10,"down",12.,7.,30,33),    // 回抽破一买价(low=7.0 < 8.0) → 破位二买
        ];
        let zs = vec![make_zs("bi_zs",6,12,22.,19.), make_zs("bi_zs",18,24,13.,11.)];
        let bc = vec![make_bc("bi_beichi",27,"down","trend")];
        let pts = detect_buy_sell(&bis, &zs, &bc);
        let b2_break: Vec<_> = pts.iter().filter(|p| p.bs_type == "2buy_break").collect();
        assert!(!b2_break.is_empty(), "回抽破一买价应产生破位二买");
        assert_eq!(b2_break[0].index, 33);
        assert!(b2_break[0].price < 8.0, "破位二买价格应低于一买价格");
    }

    // ─── 三买边界测试：回抽恰好触及ZG（等于） ───

    #[test]
    fn test_third_buy_when_pullback_equals_zg() {
        // 回抽低点恰好等于ZG → 仍构成三买（17课原文"不跌破ZG"含等于）
        let bis = vec![
            make_bi(0,"up",10.,15.,0,3), make_bi(1,"down",15.,12.,3,6),
            make_bi(2,"up",12.,14.,6,9), make_bi(3,"down",14.,13.,9,12),
            // ZS: [12,14], zg=14, zd=12
            make_bi(4,"up",13.,20.,12,15),   // 向上离开，高点20>Zg=14
            make_bi(5,"down",20.,14.,15,18), // 回抽低点14 = Zg=14 → 仍算三买
        ];
        let zs = vec![make_zs("bi_zs",3,12,14.,12.)];
        let pts = detect_buy_sell(&bis, &zs, &[]);
        let b3: Vec<_> = pts.iter().filter(|p| p.bs_type == "3buy").collect();
        assert!(!b3.is_empty(), "回抽恰好触及ZG仍应构成三买（原文'不跌破ZG'含等于）");
    }

    // ─── 三卖边界测试：回抽恰好触及ZD（等于） ───

    #[test]
    fn test_third_sell_when_pullback_equals_zd() {
        // 回抽高点恰好等于ZD → 仍构成三卖（17课原文"不升破ZD"含等于）
        let bis = vec![
            make_bi(0,"down",25.,20.,0,3), make_bi(1,"up",20.,23.,3,6),
            make_bi(2,"down",23.,21.,6,9), make_bi(3,"up",21.,22.,9,12),
            // ZS: [21,23], zg=23, zd=21
            make_bi(4,"down",22.,15.,12,15), // 向下离开，低点15<Zd=21
            make_bi(5,"up",15.,21.,15,18),   // 回抽高点21 = Zd=21 → 仍算三卖
        ];
        let zs = vec![make_zs("bi_zs",3,12,23.,21.)];
        let pts = detect_buy_sell(&bis, &zs, &[]);
        let s3: Vec<_> = pts.iter().filter(|p| p.bs_type == "3sell").collect();
        assert!(!s3.is_empty(), "回抽恰好触及ZD仍应构成三卖（原文'不升破ZD'含等于）");
    }

    // ─── 二三买重合测试 ───

    #[test]
    fn test_2plus3_buy_overlap() {
        // 一买后离开段直接上破下跌最后中枢 + 回抽不破ZG → 二三买重合("2+3buy")
        let bis = vec![
            make_bi(0,"up",20.,25.,0,3), make_bi(1,"down",25.,18.,3,6),
            make_bi(2,"up",18.,22.,6,9), make_bi(3,"down",22.,19.,9,12),
            make_bi(4,"up",19.,16.,12,15), make_bi(5,"down",16.,10.,15,18),
            make_bi(6,"up",10.,13.,18,21), make_bi(7,"down",13.,11.,21,24),
            make_bi(8,"down",11.,8.,24,27),
            // 一买: idx=27, price=8.0
            // 下跌最后中枢ZS2: zg=13, zd=11
            // 离开段直接上破ZS2高点 → 二三买重合
            make_bi(9,"up",8.,18.,27,30),     // 离开段，高点18>Zg=13
            make_bi(10,"down",18.,14.,30,33), // 回抽低点14>Zg=13 → 三买且不破一买(8)→也二买
        ];
        let zs = vec![make_zs("bi_zs",6,12,22.,19.), make_zs("bi_zs",18,24,13.,11.)];
        let bc = vec![make_bc("bi_beichi",27,"down","trend")];
        let pts = detect_buy_sell(&bis, &zs, &bc);
        let overlap: Vec<_> = pts.iter().filter(|p| p.bs_type == "2+3buy").collect();
        assert!(!overlap.is_empty(), "二三买在同一位置应重合为'2+3buy'");
        assert_eq!(overlap[0].index, 33);
    }
}
