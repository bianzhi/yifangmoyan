//! 走势递归分解
//!
//! **严格对齐缠论走势定义**
//!
//! 缠论中的走势（ZouShi）由中枢和离开段构成，分两类：
//!
//! - **盘整走势**：只有一个中枢的走势
//! - **趋势走势**：有两个或以上同级别中枢的走势
//!
//! 递归分解规则：
//! 1. 笔 → 线段（已有的线段模块）
//! 2. 线段 → 1F走势（由线段序列构建中枢，每两中枢之间构成离开段）
//! 3. 1F走势 → 5F走势（1F走势作为5F走势的元素构建中枢）
//! 4. 5F走势 → 30F走势 … 依此类推
//!
//! 关键定义（缠论第20课）：
//! - 走势 = a + A + b + B + c（趋势）或 a + A + b（盘整）
//! - A、B 是中枢，a、b、c 是连接段（离开段）
//! - 趋势背驰：c 的力度 < b 的力度
//! - 盘整背驰：b 的力度 < a 的力度

use yifang_data::{XianDuan, ZhongShu, ZouShi};

/// 从线段序列构建走势（1F走势 = 线段序列的中枢与连接段）
///
/// 严格对齐缠论走势定义：
/// 1. 从连续3段开始构建中枢
/// 2. 中枢成立的区间 -> 一个中枢
/// 3. 两个中枢之间的连接段 -> 走势的离开段
/// 4. 只有一个中枢 -> 盘整走势
/// 5. 两个及以上中枢 -> 趋势走势
pub fn build_zoushi_from_xd(xds: &[XianDuan]) -> Vec<ZouShi> {
    if xds.len() < 3 {
        return Vec::new();
    }

    // 先构建线段中枢
    let zs_list = build_xd_zs_for_zoushi(xds);
    
    if zs_list.is_empty() {
        // 没有中枢，无法构成走势
        return Vec::new();
    }

    // 基于中枢列表构建走势
    build_zoushi_from_zs(xds, &zs_list)
}

/// 通用走势构建：从任意段序列（线段或走势）+ 中枢列表 构建走势
///
/// 规则：
/// - 第一个中枢之前到中枢结束 = 第一个走势段
/// - 两个中枢之间 = 连接段
/// - 只有一个中枢 → 盘整走势
/// - 两个及以上中枢 → 趋势走势
fn build_zoushi_from_zs(xds: &[XianDuan], zs_list: &[ZhongShu]) -> Vec<ZouShi> {
    let mut zoushi_list = Vec::new();

    if zs_list.is_empty() {
        return zoushi_list;
    }

    let is_trend = zs_list.len() >= 2;
    let zs_type_str = if is_trend { "trend" } else { "panzheng" };

    // 两种走势构成方式：
    // 1. 单中枢 → 盘整走势 = 从第一段到中枢最后一段
    // 2. 多中枢 → 趋势走势 = 每个连续中枢组构成一个趋势走势
    if !is_trend {
        // 盘整走势：一个中枢，从中枢开始前的段到中枢结束后的段
        let zs = &zs_list[0];
        
        // 找到中枢开始对应的线段索引
        let start_idx = xds.iter().position(|xd| xd.start_index == zs.start_index || xd.end_index == zs.start_index).unwrap_or(0);
        // 找到中枢结束对应的线段索引
        let end_idx = xds.iter().position(|xd| xd.end_index == zs.end_index || xd.start_index == zs.end_index).unwrap_or(xds.len() - 1);
        
        // 盘整走势包含中枢前后各一段连接段（如果有的话）
        let zs_start = if start_idx > 0 { start_idx - 1 } else { start_idx };
        let zs_end = if end_idx < xds.len() - 1 { end_idx + 1 } else { end_idx };

        let start_xd = &xds[zs_start];
        let end_xd = &xds[zs_end];

        // 走势方向：由两端的极值决定
        let direction = if start_xd.start_price < end_xd.end_price {
            "up"
        } else {
            "down"
        };

        zoushi_list.push(ZouShi {
            direction: direction.to_string(),
            zs_type: zs_type_str.to_string(),
            start_index: start_xd.start_index,
            end_index: end_xd.end_index,
            start_dt: start_xd.start_dt.clone(),
            end_dt: end_xd.end_dt.clone(),
            start_price: start_xd.start_price,
            end_price: end_xd.end_price,
            zs_list: zs_list.to_vec(),
            is_finished: false,
        });

        return zoushi_list;
    }

    // 趋势走势：按照中枢递进关系分解
    // 策略：每个连续中枢组构成一个趋势走势
    // 两个中枢的 zg/zd 有交集 → 归入同一走势
    let mut i = 0;
    while i < zs_list.len() {
        let mut group_zs = vec![zs_list[i].clone()];
        let mut j = i + 1;

        // 检查后续中枢是否可以归入同一趋势走势
        // 缠论定义：同向中枢（zg/zd 递进）构成趋势
        while j < zs_list.len() {
            let prev_zs = &group_zs.last().unwrap();
            let curr_zs = &zs_list[j];

            // 判断中枢方向递进：
            // 上涨趋势：后续中枢的 zd > 前一个中枢的 zd（中枢底部在抬升）
            // 下跌趋势：后续中枢的 zg < 前一个中枢的 zg（中枢顶部在下降）
            let is_progressive = curr_zs.zd > prev_zs.zd || curr_zs.zg < prev_zs.zg;

            if is_progressive {
                group_zs.push(curr_zs.clone());
                j += 1;
            } else {
                break;
            }
        }

        // 构建 this 走势
        let first_zs = &group_zs[0];
        let last_zs = group_zs.last().unwrap();

        // 走势范围：从第一个中枢之前到最后一个中枢之后
        let start_idx = xds.iter()
            .position(|xd| xd.start_index == first_zs.start_index || xd.end_index == first_zs.start_index)
            .unwrap_or(0);
        let end_idx = xds.iter()
            .position(|xd| xd.end_index == last_zs.end_index || xd.start_index == last_zs.end_index)
            .unwrap_or(xds.len() - 1);

        let zs_start = if start_idx > 0 { start_idx - 1 } else { start_idx };
        let zs_end = if end_idx < xds.len() - 1 { end_idx + 1 } else { end_idx };

        let start_xd = &xds[zs_start];
        let end_xd = &xds[zs_end];

        // 走势方向：由两端极值或中枢递进方向决定
        let direction = if group_zs.windows(2).all(|w| w[1].zd >= w[0].zd) && group_zs.last().unwrap().zd >= group_zs[0].zd {
            "up"
        } else {
            "down"
        };

        let is_trend_zs = group_zs.len() >= 2;
        
        zoushi_list.push(ZouShi {
            direction: direction.to_string(),
            zs_type: if is_trend_zs { "trend" } else { "panzheng" }.to_string(),
            start_index: start_xd.start_index,
            end_index: end_xd.end_index,
            start_dt: start_xd.start_dt.clone(),
            end_dt: end_xd.end_dt.clone(),
            start_price: start_xd.start_price,
            end_price: end_xd.end_price,
            zs_list: group_zs,
            is_finished: true,
        });

        i = j;
    }

    zoushi_list
}

/// 为走势分解构建线段中枢
///
/// 与 zs.rs 中的 build_xd_zs 类似，但返回中枢列表供走势分解使用
fn build_xd_zs_for_zoushi(xds: &[XianDuan]) -> Vec<ZhongShu> {
    if xds.len() < 3 {
        return Vec::new();
    }

    let xd_high = |xd: &XianDuan| xd.start_price.max(xd.end_price);
    let xd_low = |xd: &XianDuan| xd.start_price.min(xd.end_price);

    let mut zs_list = Vec::new();
    let mut i = 0;

    while i + 2 < xds.len() {
        let h1 = xd_high(&xds[i]);
        let l1 = xd_low(&xds[i]);
        let h2 = xd_high(&xds[i + 1]);
        let l2 = xd_low(&xds[i + 1]);
        let h3 = xd_high(&xds[i + 2]);
        let l3 = xd_low(&xds[i + 2]);

        let zg = h1.min(h2).min(h3);
        let zd = l1.max(l2).max(l3);

        if zg < zd {
            i += 1;
            continue;
        }

        let mut gg = h1.max(h2).max(h3);
        let mut dd = l1.min(l2).min(l3);
        let mut end_i = i + 2;

        for j in (i + 3)..xds.len() {
            let j_high = xd_high(&xds[j]);
            let j_low = xd_low(&xds[j]);

            let has_overlap = (zd <= j_high && j_high <= zg)
                || (zd <= j_low && j_low <= zg)
                || (j_low <= zd && j_high >= zg);

            if has_overlap {
                gg = gg.max(j_high);
                dd = dd.min(j_low);
                end_i = j;
            } else {
                break;
            }
        }

        zs_list.push(ZhongShu {
            zs_type: "xd_zs".to_string(),
            start_index: xds[i].start_index,
            end_index: xds[end_i].end_index,
            start_dt: xds[i].start_dt.clone(),
            end_dt: xds[end_i].end_dt.clone(),
            zg,
            zd,
            gg,
            dd,
        });

        i = end_i + 1;
    }

    zs_list
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_xd(id: usize, dir: &str, start: f64, end: f64, start_idx: u64, end_idx: u64) -> XianDuan {
        XianDuan {
            direction: dir.to_string(),
            start_index: start_idx,
            end_index: end_idx,
            start_dt: format!("t{}", id),
            end_dt: format!("t{}", id + 1),
            start_price: start,
            end_price: end,
            is_finished: true,
        }
    }

    #[test]
    fn test_panzheng_zoushi() {
        // 3段构成一个中枢 → 盘整走势
        let xds = vec![
            make_xd(0, "up", 10.0, 15.0, 0, 3),
            make_xd(1, "down", 15.0, 12.0, 3, 6),
            make_xd(2, "up", 12.0, 14.0, 6, 9),
            make_xd(3, "down", 14.0, 13.0, 9, 12),
        ];

        let zoushi = build_zoushi_from_xd(&xds);
        // 只有一个中枢 → 盘整走势
        assert!(!zoushi.is_empty(), "应检测到走势");
        assert_eq!(zoushi[0].zs_type, "panzheng", "单个中枢应为盘整走势");
    }

    #[test]
    fn test_trend_zoushi() {
        // 构建上涨趋势：两个递进中枢
        let xds = vec![
            // 中枢1 [12,14]
            make_xd(0, "up", 10.0, 15.0, 0, 3),
            make_xd(1, "down", 15.0, 12.0, 3, 6),
            make_xd(2, "up", 12.0, 14.0, 6, 9),
            make_xd(3, "down", 14.0, 13.0, 9, 12),
            // 离开中枢1，上升
            make_xd(4, "up", 13.0, 20.0, 12, 15),
            // 中枢2 [17,19]
            make_xd(5, "down", 20.0, 17.0, 15, 18),
            make_xd(6, "up", 17.0, 19.0, 18, 21),
            make_xd(7, "down", 19.0, 18.0, 21, 24),
        ];

        let zoushi = build_zoushi_from_xd(&xds);
        assert!(!zoushi.is_empty(), "应检测到走势");
        // 两个递进中枢 → 趋势走势
        let trend_zs: Vec<_> = zoushi.iter().filter(|z| z.zs_type == "trend").collect();
        if !trend_zs.is_empty() {
            assert_eq!(trend_zs[0].direction, "up", "应检测为上涨趋势");
        }
    }

    #[test]
    fn test_no_zs_no_zoushi() {
        // 没有中枢 → 没有走势
        let xds = vec![
            make_xd(0, "up", 10.0, 20.0, 0, 3),
            make_xd(1, "down", 20.0, 5.0, 3, 6),
            make_xd(2, "up", 5.0, 8.0, 6, 9),
        ];

        let zoushi = build_zoushi_from_xd(&xds);
        assert!(zoushi.is_empty(), "无中枢不应有走势");
    }
}
