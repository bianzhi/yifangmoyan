//! 三类买卖点识别（严格缠论17/20/21/37课）
//!
//! 核心规则：
//! - **一买**：下跌趋势（≥2个向下递进中枢）+ 最后一个中枢后出现趋势背驰
//!   只有趋势背驰才能产生第一类买点，盘整背驰绝对不能作为第一类买点。
//! - **一卖**：上涨趋势（≥2个向上递进中枢）+ 最后一个中枢后出现趋势背驰
//! - **二买**：一买之后的回调低点不破一买低点
//! - **二卖**：一卖之后的反弹高点不过一卖高点
//! - **三买**：中枢之上，次级别离开后次级别回抽不回到中枢区间（回抽低点 > 中枢上沿）
//! - **三卖**：中枢之下，次级别离开后次级别回抽不回到中枢区间（回抽高点 < 中枢下沿）
//!
//! 铁律：
//! - 盘整背驰绝对不能产生第一类买卖点
//! - 第三类买卖点的离开和回抽必须是完整的段（笔或线段），单笔回抽不能构成
//! - 第二类买卖点可出现在中枢内部/之上/之下，只需不破一买低点/一卖高点

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

    // 过滤出当前级别的趋势背驰
    let trend_bds: Vec<&BeiChi> = beichi
        .iter()
        .filter(|bd| bd.bc_type == bc_type_filter && bd.bc_sub_type == "trend")
        .collect();

    // 第一类买卖点
    let (buy1_list, sell1_list) = find_buy1_sell1(&seg_infos, zs_list, &trend_bds);
    results.extend(buy1_list);
    results.extend(sell1_list);

    // 第二类买卖点（依赖一买一卖）
    let (buy2_list, sell2_list) = find_buy2_sell2(&seg_infos, &results);
    results.extend(buy2_list);
    results.extend(sell2_list);

    // 第三类买卖点（扫描中枢后所有离开+回抽段对）
    let (buy3_list, sell3_list) = find_buy3_sell3(&seg_infos, zs_list);
    results.extend(buy3_list);
    results.extend(sell3_list);

    results.sort_by_key(|p| p.index);
    results.dedup_by(|a, b| a.index == b.index && a.bs_type == b.bs_type);
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
/// - 上涨趋势（≥2个向上递进中枢）+ 顶背驰 → 一卖
/// - 下跌趋势（≥2个向下递进中枢）+ 底背驰 → 一买
/// 铁律：盘整背驰绝对不能产生第一类买卖点
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

        let groups = group_zs_indices_by_trend(zs_list, &related_indices);

        for group in &groups {
            if group.len() < 2 {
                continue;
            }

            let trend_dir = classify_trend_direction_from_indices(zs_list, group);

            if trend_dir == "up" && bd.direction == "up" {
                let price = find_seg_end_price(seg_infos, bd.index);
                sell1_list.push(BuySellPoint {
                    bs_type: "1sell".to_string(),
                    index: bd.index,
                    dt: bd.dt.clone(),
                    price,
                });
            } else if trend_dir == "down" && bd.direction == "down" {
                let price = find_seg_end_price(seg_infos, bd.index);
                buy1_list.push(BuySellPoint {
                    bs_type: "1buy".to_string(),
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

/// 二买：一买后回抽不破一买低点
/// 二卖：一卖后回抽不过一卖高点
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

fn find_buy2_after_buy1(seg_infos: &[SegInfo], buy1: &BuySellPoint) -> Option<BuySellPoint> {
    // 买点之后的段：从一买位置开始找
    let after_segs: Vec<&SegInfo> = seg_infos.iter().filter(|s| s.start_idx >= buy1.index).collect();
    if after_segs.len() < 2 { return None; }

    // 找第一段向上（离开段），然后第一段向下（回抽段）
    let mut leave_idx = None;
    let mut pullback_idx = None;

    for (i, seg) in after_segs.iter().enumerate() {
        if leave_idx.is_none() && seg.direction == "up" {
            leave_idx = Some(i);
        }
        if leave_idx.is_some() && seg.direction == "down" {
            pullback_idx = Some(i);
            break;
        }
    }

    let _li = leave_idx?;
    let pi = pullback_idx?;
    let pullback = after_segs[pi];

    let pullback_low = pullback.end_val.min(pullback.start_val);
    if pullback_low > buy1.price {
        Some(BuySellPoint {
            bs_type: "2buy".to_string(),
            index: pullback.end_idx,
            dt: String::new(),
            price: pullback_low,
        })
    } else {
        None
    }
}

fn find_sell2_after_sell1(seg_infos: &[SegInfo], sell1: &BuySellPoint) -> Option<BuySellPoint> {
    let after_segs: Vec<&SegInfo> = seg_infos.iter().filter(|s| s.start_idx >= sell1.index).collect();
    if after_segs.len() < 2 { return None; }

    // 找第一段向下（离开段），然后第一段向上（回抽段）
    let mut leave_idx = None;
    let mut pullback_idx = None;

    for (i, seg) in after_segs.iter().enumerate() {
        if leave_idx.is_none() && seg.direction == "down" {
            leave_idx = Some(i);
        }
        if leave_idx.is_some() && seg.direction == "up" {
            pullback_idx = Some(i);
            break;
        }
    }

    let _li = leave_idx?;
    let pi = pullback_idx?;
    let pullback = after_segs[pi];

    let pullback_high = pullback.end_val.max(pullback.start_val);
    if pullback_high < sell1.price {
        Some(BuySellPoint {
            bs_type: "2sell".to_string(),
            index: pullback.end_idx,
            dt: String::new(),
            price: pullback_high,
        })
    } else {
        None
    }
}

// ─── 第三类买卖点 ─────────────────────────────────────

/// 三买：向上离开中枢 + 回抽低点 > 中枢上沿(zg)
/// 三卖：向下离开中枢 + 回抽高点 < 中枢下沿(zd)
///
/// 关键改进：扫描中枢后**所有**离开+回抽段对，而不是仅检查紧邻的前2段。
/// 原因：中枢后可能有多段震荡（中枢扩展），真正的离开+回抽不一定紧邻中枢。
fn find_buy3_sell3(
    seg_infos: &[SegInfo],
    zs_list: &[ZhongShu],
) -> (Vec<BuySellPoint>, Vec<BuySellPoint>) {
    let mut buy3_list = Vec::new();
    let mut sell3_list = Vec::new();

    for zs in zs_list {
        // 中枢结束后的所有段
        let after_segs: Vec<&SegInfo> = seg_infos.iter().filter(|s| s.start_idx >= zs.end_index).collect();
        if after_segs.len() < 2 { continue; }

        // 关键改进：扫描所有相邻的离开段+回抽段对
        // 离开段 = 价格超出中枢范围的段
        // 回抽段 = 紧跟离开段之后反方向的段
        for i in 0..after_segs.len() - 1 {
            let leave_seg = after_segs[i];
            let back_seg = after_segs[i + 1];

            // 三买：向上离开 + 回抽不破中枢上沿
            if leave_seg.direction == "up" && back_seg.direction == "down" {
                let leave_high = leave_seg.end_val.max(leave_seg.start_val);
                if leave_high > zs.zg {
                    let back_low = back_seg.end_val.min(back_seg.start_val);
                    if back_low > zs.zg {
                        buy3_list.push(BuySellPoint {
                            bs_type: "3buy".to_string(),
                            index: back_seg.end_idx,
                            dt: String::new(),
                            price: back_low,
                        });
                    }
                }
            }

            // 三卖：向下离开 + 回抽不破中枢下沿
            if leave_seg.direction == "down" && back_seg.direction == "up" {
                let leave_low = leave_seg.end_val.min(leave_seg.start_val);
                if leave_low < zs.zd {
                    let back_high = back_seg.end_val.max(back_seg.start_val);
                    if back_high < zs.zd {
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

/// 将中枢索引按方向递进分组：连续同方向递进归为一组，方向改变开始新组
/// 关键：需要追踪当前组的方向，后续中枢必须延续同方向才能加入
fn group_zs_indices_by_trend(zs_list: &[ZhongShu], indices: &[usize]) -> Vec<Vec<usize>> {
    if indices.is_empty() { return Vec::new(); }
    if indices.len() == 1 { return vec![vec![indices[0]]]; }

    let first_zs = &zs_list[indices[0]];
    let mut current_dir: Option<&str> = None;

    // 找到第一个能确定方向的中枢对
    for i in 1..indices.len() {
        let curr_zs = &zs_list[indices[i]];
        if curr_zs.zd > first_zs.zg {
            current_dir = Some("up");
            break;
        } else if curr_zs.zg < first_zs.zd {
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

/// 上涨递进：curr.zd > prev.zg；下跌递进：curr.zg < prev.zd
fn _is_same_trend_direction(prev: &ZhongShu, curr: &ZhongShu) -> bool {
    curr.zd > prev.zg || curr.zg < prev.zd
}

fn classify_trend_direction_from_indices(zs_list: &[ZhongShu], group: &[usize]) -> String {
    if group.len() < 2 { return "unknown".to_string(); }
    let first = &zs_list[group[0]];
    let last = &zs_list[group[group.len() - 1]];
    if last.zd > first.zg { "up".to_string() }
    else if last.zg < first.zd { "down".to_string() }
    else { "unknown".to_string() }
}

fn find_seg_end_price(seg_infos: &[SegInfo], index: u64) -> f64 {
    seg_infos.iter()
        .find(|s| s.end_idx == index)
        .map(|s| s.end_val)
        .unwrap_or(0.0)
}

// ─── 测试 ─────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use yifang_data::{Bi, ZhongShu, BeiChi};

    fn make_bi(id: usize, dir: &str, start: f64, end: f64, si: u64, ei: u64) -> Bi {
        let _ = id;
        Bi { direction: dir.to_string(), start_price: start, end_price: end,
             start_index: si, end_index: ei, start_dt: String::new(), end_dt: String::new(),
             is_finished: true }
    }
    fn make_xd(id: usize, dir: &str, start: f64, end: f64, si: u64, ei: u64) -> XianDuan {
        let _ = id;
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

    #[test]
    fn test_first_buy_from_trend_beichi() {
        let bis = vec![
            make_bi(0,"up",20.,25.,0,3), make_bi(1,"down",25.,18.,3,6),
            make_bi(2,"up",18.,22.,6,9), make_bi(3,"down",22.,19.,9,12),
            make_bi(4,"up",19.,16.,12,15), make_bi(5,"down",16.,10.,15,18),
            make_bi(6,"up",10.,13.,18,21), make_bi(7,"down",13.,11.,21,24),
            make_bi(8,"down",11.,8.,24,27),
        ];
        let zs = vec![make_zs("bi_zs",6,12,22.,19.), make_zs("bi_zs",18,24,13.,11.)];
        let bc = vec![make_bc("bi_beichi",27,"down","trend")];
        let pts = detect_buy_sell(&bis, &zs, &bc);
        let b1: Vec<_>=pts.iter().filter(|p|p.bs_type=="1buy").collect();
        assert!(!b1.is_empty()); assert_eq!(b1[0].index, 27);
    }

    #[test]
    fn test_first_sell_from_trend_beichi() {
        let bis = vec![
            make_bi(0,"up",10.,15.,0,3), make_bi(1,"down",15.,12.,3,6),
            make_bi(2,"up",12.,14.,6,9), make_bi(3,"down",14.,13.,9,12),
            make_bi(4,"up",13.,22.,12,16), make_bi(5,"down",22.,17.,16,19),
            make_bi(6,"up",17.,19.,19,22), make_bi(7,"down",19.,18.,22,25),
            make_bi(8,"up",18.,23.,25,29),
        ];
        let zs = vec![make_zs("bi_zs",3,12,14.,12.), make_zs("bi_zs",16,25,19.,17.)];
        let bc = vec![make_bc("bi_beichi",29,"up","trend")];
        let pts = detect_buy_sell(&bis, &zs, &bc);
        let s1: Vec<_>=pts.iter().filter(|p|p.bs_type=="1sell").collect();
        assert!(!s1.is_empty()); assert_eq!(s1[0].index, 29);
    }

    #[test]
    fn test_no_first_buy_from_panzheng() {
        let bis = vec![
            make_bi(0,"up",10.,15.,0,3), make_bi(1,"down",15.,12.,3,6),
            make_bi(2,"up",12.,14.,6,9), make_bi(3,"down",14.,13.,9,12),
        ];
        let zs = vec![make_zs("bi_zs",3,12,14.,12.)];
        let bc = vec![make_bc("bi_beichi",12,"down","panzheng")];
        let pts = detect_buy_sell(&bis, &zs, &bc);
        assert!(pts.iter().all(|p|p.bs_type!="1buy"));
        assert!(pts.iter().all(|p|p.bs_type!="1sell"));
    }

    #[test]
    fn test_no_first_with_single_zs() {
        let bis = vec![
            make_bi(0,"up",10.,15.,0,3), make_bi(1,"down",15.,12.,3,6),
            make_bi(2,"up",12.,14.,6,9), make_bi(3,"down",14.,13.,9,12),
            make_bi(4,"up",13.,16.,12,15),
        ];
        let zs = vec![make_zs("bi_zs",3,12,14.,12.)];
        let bc = vec![make_bc("bi_beichi",15,"up","trend")];
        let pts = detect_buy_sell(&bis, &zs, &bc);
        assert!(pts.iter().all(|p|p.bs_type!="1buy"));
        assert!(pts.iter().all(|p|p.bs_type!="1sell"));
    }

    #[test]
    fn test_third_buy() {
        let bis = vec![
            make_bi(0,"up",10.,15.,0,3), make_bi(1,"down",15.,12.,3,6),
            make_bi(2,"up",12.,14.,6,9), make_bi(3,"down",14.,13.,9,12),
            make_bi(4,"up",13.,18.,12,16), make_bi(5,"down",18.,15.,16,19),
        ];
        let zs = vec![make_zs("bi_zs",3,12,14.,12.)];
        let pts = detect_buy_sell(&bis, &zs, &[]);
        let b3: Vec<_>=pts.iter().filter(|p|p.bs_type=="3buy").collect();
        assert!(!b3.is_empty()); assert!(b3[0].price > 14.0);
    }

    #[test]
    fn test_third_sell() {
        let bis = vec![
            make_bi(0,"down",20.,15.,0,3), make_bi(1,"up",15.,18.,3,6),
            make_bi(2,"down",18.,16.,6,9), make_bi(3,"up",16.,17.,9,12),
            make_bi(4,"down",17.,12.,12,16), make_bi(5,"up",12.,14.,16,19),
        ];
        let zs = vec![make_zs("bi_zs",3,12,17.,15.)];
        let pts = detect_buy_sell(&bis, &zs, &[]);
        let s3: Vec<_>=pts.iter().filter(|p|p.bs_type=="3sell").collect();
        assert!(!s3.is_empty()); assert!(s3[0].price < 15.0);
    }

    #[test]
    fn test_no_third_buy_when_pullback_enters_zs() {
        let bis = vec![
            make_bi(0,"up",10.,15.,0,3), make_bi(1,"down",15.,12.,3,6),
            make_bi(2,"up",12.,14.,6,9), make_bi(3,"down",14.,13.,9,12),
            make_bi(4,"up",13.,18.,12,16), make_bi(5,"down",18.,11.,16,19),
        ];
        let zs = vec![make_zs("bi_zs",3,12,14.,12.)];
        let pts = detect_buy_sell(&bis, &zs, &[]);
        assert!(pts.iter().all(|p|p.bs_type!="3buy"));
    }

    #[test]
    fn test_second_buy_after_first_buy() {
        let bis = vec![
            make_bi(0,"up",20.,25.,0,3), make_bi(1,"down",25.,18.,3,6),
            make_bi(2,"up",18.,22.,6,9), make_bi(3,"down",22.,19.,9,12),
            make_bi(4,"up",19.,16.,12,15), make_bi(5,"down",16.,10.,15,18),
            make_bi(6,"up",10.,13.,18,21), make_bi(7,"down",13.,11.,21,24),
            make_bi(8,"down",11.,8.,24,27),
            make_bi(9,"up",8.,12.,27,30), make_bi(10,"down",12.,10.,30,33),
        ];
        let zs = vec![make_zs("bi_zs",6,12,22.,19.), make_zs("bi_zs",18,24,13.,11.)];
        let bc = vec![make_bc("bi_beichi",27,"down","trend")];
        let pts = detect_buy_sell(&bis, &zs, &bc);
        let b1: Vec<_>=pts.iter().filter(|p|p.bs_type=="1buy").collect();
        let b2: Vec<_>=pts.iter().filter(|p|p.bs_type=="2buy").collect();
        assert!(!b1.is_empty());
        assert!(!b2.is_empty());
        assert!(b2[0].price > b1[0].price);
    }

    #[test]
    fn test_second_sell_after_first_sell() {
        let bis = vec![
            make_bi(0,"up",10.,15.,0,3), make_bi(1,"down",15.,12.,3,6),
            make_bi(2,"up",12.,14.,6,9), make_bi(3,"down",14.,13.,9,12),
            make_bi(4,"up",13.,22.,12,16), make_bi(5,"down",22.,17.,16,19),
            make_bi(6,"up",17.,19.,19,22), make_bi(7,"down",19.,18.,22,25),
            make_bi(8,"up",18.,23.,25,29),
            make_bi(9,"down",23.,19.,29,32), make_bi(10,"up",19.,21.,32,35),
        ];
        let zs = vec![make_zs("bi_zs",3,12,14.,12.), make_zs("bi_zs",16,25,19.,17.)];
        let bc = vec![make_bc("bi_beichi",29,"up","trend")];
        let pts = detect_buy_sell(&bis, &zs, &bc);
        let s1: Vec<_>=pts.iter().filter(|p|p.bs_type=="1sell").collect();
        let s2: Vec<_>=pts.iter().filter(|p|p.bs_type=="2sell").collect();
        assert!(!s1.is_empty());
        assert!(!s2.is_empty());
        assert!(s2[0].price < s1[0].price);
    }

    #[test]
    fn test_xd_buy_sell() {
        let xds = vec![
            make_xd(0,"up",10.,15.,0,3), make_xd(1,"down",15.,12.,3,6),
            make_xd(2,"up",12.,14.,6,9), make_xd(3,"down",14.,13.,9,12),
            make_xd(4,"up",13.,22.,12,16), make_xd(5,"down",22.,17.,16,19),
            make_xd(6,"up",17.,19.,19,22), make_xd(7,"down",19.,18.,22,25),
            make_xd(8,"up",18.,23.,25,29),
        ];
        let zd_zs = vec![make_zs("xd_zs",3,12,14.,12.), make_zs("xd_zs",16,25,19.,17.)];
        let bc = vec![make_bc("xd_beichi",29,"up","trend")];
        let pts = detect_xd_buy_sell(&xds, &zd_zs, &bc);
        assert!(pts.iter().any(|p|p.bs_type=="1sell"));
    }

    #[test]
    fn test_empty_zs_no_points() {
        let bis = vec![make_bi(0,"up",10.,15.,0,3), make_bi(1,"down",15.,12.,3,6)];
        assert!(detect_buy_sell(&bis, &[], &[]).is_empty());
    }

    #[test]
    fn test_few_segments_no_points() {
        let bis = vec![make_bi(0,"up",10.,15.,0,3)];
        let zs = vec![make_zs("bi_zs",0,3,14.,12.)];
        assert!(detect_buy_sell(&bis, &zs, &[]).is_empty());
    }

    #[test]
    fn test_group_zs_indices_by_trend() {
        let z1 = make_zs("bi_zs",0,10,14.,12.);
        let z2 = make_zs("bi_zs",15,25,19.,17.);
        let z3 = make_zs("bi_zs",30,40,10.,8.);
        let zs = vec![z1, z2, z3];
        let groups = group_zs_indices_by_trend(&zs, &[0, 1, 2]);
        assert!(groups.len() >= 2);
        assert_eq!(groups[0].len(), 2);
        assert_eq!(groups[1], vec![2]);
    }

    #[test]
    fn test_classify_trend_direction() {
        let z1 = make_zs("bi_zs", 0, 10, 14., 12.);
        let z2 = make_zs("bi_zs", 15, 25, 19., 17.);
        let zs = vec![z1, z2];
        assert_eq!(classify_trend_direction_from_indices(&zs, &[0, 1]), "up");

        let z3 = make_zs("bi_zs", 0, 10, 14., 12.);
        let z4 = make_zs("bi_zs", 15, 25, 10., 8.);
        let zs2 = vec![z3, z4];
        assert_eq!(classify_trend_direction_from_indices(&zs2, &[0, 1]), "down");
    }

    #[test]
    fn test_third_buy_from_later_segs() {
        // 中枢后有震荡段，真正的离开+回抽在后面
        let bis = vec![
            make_bi(0,"up",10.,15.,0,3), make_bi(1,"down",15.,12.,3,6),
            make_bi(2,"up",12.,14.,6,9), make_bi(3,"down",14.,13.,9,12),
            // 中枢结束于12
            // 震荡段（还在中枢范围内）
            make_bi(4,"up",13.,14.,12,15), make_bi(5,"down",14.,12.5,15,18),
            // 真正的离开+回抽
            make_bi(6,"up",12.5,18.,18,22), make_bi(7,"down",18.,14.5,22,25),
        ];
        let zs = vec![make_zs("bi_zs",3,12,14.,12.)];
        let pts = detect_buy_sell(&bis, &zs, &[]);
        let b3: Vec<_>=pts.iter().filter(|p|p.bs_type=="3buy").collect();
        assert!(!b3.is_empty()); assert!(b3[0].price > 14.0);
    }
}
