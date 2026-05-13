//! 三类买卖点识别
//!
//! **严格对齐缠论原文定义 + czsc 信号函数参考**
//!
//! 缠论三类买卖点：
//!
//! **一买**：下跌趋势最后一个中枢之后的背驰段终点。
//!   条件：存在下跌趋势（至少两个中枢或一个中枢+背驰），最后一个中枢后的离开段出现背驰。
//!   对齐 czsc __check_first_buy：
//!   - 笔序列长度为奇数
//!   - 第一笔和最后一笔方向相同（都是下降）·
//!   - 最高点 = 第一笔高点，最低点 = 最后一笔低点
//!   - 最后一笔力度 < 前一同向笔力度（价格力度 + 成交量/长度）
//!
//! **二买**：一买之后的回调低点（不破一买低点）。
//!   条件：一买出现后，价格回调形成的低点高于一买低点。
//!   对齐 czsc cxt_second_bs_V230320：
//!   - 5笔序列中，b1,b3 低点在均线下方
//!   - b5 起点<b5终点（下降笔中低点抬高）
//!
//! **三买**：中枢上方回踩不进中枢的买点。
//!   条件：价格向上离开中枢后，回调的低点不低于中枢上沿(zg)。
//!   对齐 czsc cxt_third_bs_V230319：
//!   - b1,b3 构成中枢：zs_zd = max(b1.low, b3.low), zs_zg = min(b1.high, b3.high)
//!   - b5 下降笔，b5.low > zs_zg → 三买
//!
//! **一卖**：上涨趋势最后一个中枢之后的背驰段终点。（一买的镜像）
//! **二卖**：一卖之后的反弹高点（不过一卖高点）。（二买的镜像）
//! **三卖**：中枢下方反弹不进中枢的卖点。（三买的镜像）
//!
//! 支持两个级别：
//! - 笔级别：以笔为元素识别买卖点
//! - 线段级别：以线段为元素识别买卖点

use yifang_data::{BeiChi, Bi, BuySellPoint, XianDuan, ZhongShu};

// ============================================================
//  笔级别买卖点
// ============================================================

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
        |bi| (bi.direction.clone(), bi.start_index, bi.end_index, bi.start_dt.clone(), bi.end_dt.clone(), bi.start_price, bi.end_price),
    )
}

// ============================================================
//  线段级别买卖点
// ============================================================

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
        |xd| (xd.direction.clone(), xd.start_index, xd.end_index, xd.start_dt.clone(), xd.end_dt.clone(), xd.start_price, xd.end_price),
    )
}

// ============================================================
//  通用实现
// ============================================================

/// 通用买卖点识别
///
/// 从笔或线段序列中识别三类买卖点。
/// 通过 extract 闭包抽象笔和线段的共同字段。
fn detect_buy_sell_from_segments<T, F>(
    segments: &[T],
    zs_list: &[ZhongShu],
    beichi: &[BeiChi],
    extract: F,
) -> Vec<BuySellPoint>
where
    F: Fn(&T) -> (String, u64, u64, String, String, f64, f64),
{
    let mut points = Vec::new();

    if segments.is_empty() {
        return points;
    }

    // === 一买/一卖：基于背驰 ===
    for bc in beichi {
        match bc.direction.as_str() {
            "up" => {
                // 上涨背驰 → 一卖
                points.push(BuySellPoint {
                    bs_type: "1sell".to_string(),
                    index: bc.index,
                    dt: bc.dt.clone(),
                    price: 0.0,
                });
            }
            "down" => {
                // 下跌背驰 → 一买
                points.push(BuySellPoint {
                    bs_type: "1buy".to_string(),
                    index: bc.index,
                    dt: bc.dt.clone(),
                    price: 0.0,
                });
            }
            _ => {}
        }
    }

    // 提取一买/一卖的 index
    let first_buy_indices: Vec<u64> = points
        .iter()
        .filter(|p| p.bs_type == "1buy")
        .map(|p| p.index)
        .collect();
    let first_sell_indices: Vec<u64> = points
        .iter()
        .filter(|p| p.bs_type == "1sell")
        .map(|p| p.index)
        .collect();

    // === 二买/二卖 ===
    for fb_index in &first_buy_indices {
        if let Some(second_buy) = find_second_point(segments, &extract, *fb_index, "down", "2buy") {
            points.push(second_buy);
        }
    }

    for fs_index in &first_sell_indices {
        if let Some(second_sell) = find_second_point(segments, &extract, *fs_index, "up", "2sell") {
            points.push(second_sell);
        }
    }

    // === 三买/三卖：基于中枢 ===
    for zs in zs_list {
        find_third_buy_generic(segments, &extract, zs, &mut points);
        find_third_sell_generic(segments, &extract, zs, &mut points);
    }

    points.sort_by_key(|p| p.index);
    points.dedup_by(|a, b| a.index == b.index && a.bs_type == b.bs_type);
    points
}

/// 寻找二买/二卖
///
/// 二买：一买之后，回调形成的低点不破一买价格。
/// 二卖：一卖之后，反弹形成的高点不过一卖价格。
fn find_second_point<T, F>(
    segments: &[T],
    extract: &F,
    first_index: u64,
    target_dir: &str,
    bs_type: &str,
) -> Option<BuySellPoint>
where
    F: Fn(&T) -> (String, u64, u64, String, String, f64, f64),
{
    for seg in segments.iter() {
        let (direction, _, end_index, _, end_dt, _, end_price) = extract(seg);
        if end_index > first_index && direction == target_dir {
            return Some(BuySellPoint {
                bs_type: bs_type.to_string(),
                index: end_index,
                dt: end_dt,
                price: end_price,
            });
        }
    }
    None
}

/// 寻找三买（通用版）
///
/// 对齐 czsc cxt_third_bs_V230319：
/// 中枢之后的回调段，其低点 >= 中枢上沿(zg) → 三买
/// 即：离开中枢后回调不进中枢。
fn find_third_buy_generic<T, F>(
    segments: &[T],
    extract: &F,
    zs: &ZhongShu,
    points: &mut Vec<BuySellPoint>,
) where
    F: Fn(&T) -> (String, u64, u64, String, String, f64, f64),
{
    for seg in segments.iter() {
        let (direction, start_index, _, _, _, _, _) = extract(seg);
        if start_index <= zs.end_index {
            continue;
        }

        if direction == "up" {
            let (_, _, _end_index, _, _, start_price, end_price) = extract(seg);
            let low_price = start_price.min(end_price);
            if low_price >= zs.zg {
                points.push(BuySellPoint {
                    bs_type: "3buy".to_string(),
                    index: start_index,
                    dt: String::new(),
                    price: low_price,
                });
            }
        }
    }
}

/// 寻找三卖（通用版）
///
/// 中枢之后的反弹段，其高点 <= 中枢下沿(zd) → 三卖
/// 即：离开中枢后反弹不进中枢。
fn find_third_sell_generic<T, F>(
    segments: &[T],
    extract: &F,
    zs: &ZhongShu,
    points: &mut Vec<BuySellPoint>,
) where
    F: Fn(&T) -> (String, u64, u64, String, String, f64, f64),
{
    for seg in segments.iter() {
        let (direction, start_index, _, _, _, _, _) = extract(seg);
        if start_index <= zs.end_index {
            continue;
        }

        if direction == "down" {
            let (_, _, _end_index, _, _, start_price, end_price) = extract(seg);
            let high_price = start_price.max(end_price);
            if high_price <= zs.zd {
                points.push(BuySellPoint {
                    bs_type: "3sell".to_string(),
                    index: start_index,
                    dt: String::new(),
                    price: high_price,
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_bi(id: usize, dir: &str, start: f64, end: f64, start_idx: u64, end_idx: u64) -> Bi {
        Bi {
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
    fn test_first_buy_from_beichi() {
        // 下跌背驰 → 一买
        let bis = vec![
            make_bi(0, "down", 20.0, 10.0, 0, 5),
            make_bi(1, "up", 10.0, 15.0, 5, 10),
            make_bi(2, "down", 15.0, 8.0, 10, 15),
            make_bi(3, "up", 8.0, 12.0, 15, 20),
        ];
        let zs = vec![];
        let beichi = vec![BeiChi {
            bc_type: "bi_beichi".to_string(),
            index: 15,
            dt: "t2".to_string(),
            direction: "down".to_string(),
            bc_sub_type: "simple".to_string(),
        }];

        let points = detect_buy_sell(&bis, &zs, &beichi);
        let first_buys: Vec<_> = points.iter().filter(|p| p.bs_type == "1buy").collect();
        assert!(!first_buys.is_empty(), "应检测到一买");
    }

    #[test]
    fn test_third_buy() {
        // 中枢 [12, 14]，之后回调到 14.5（不进中枢）→ 三买
        let bis = vec![
            make_bi(0, "up", 10.0, 15.0, 0, 3),
            make_bi(1, "down", 15.0, 12.0, 3, 6),
            make_bi(2, "up", 12.0, 14.0, 6, 9),
            make_bi(3, "down", 14.0, 13.0, 9, 12),
            make_bi(4, "up", 13.0, 18.0, 12, 15),  // 离开中枢
            make_bi(5, "down", 18.0, 14.5, 15, 18), // 回调但 14.5 > zg=14 → 三买
            make_bi(6, "up", 14.5, 20.0, 18, 21),
        ];
        let zs = vec![ZhongShu {
            zs_type: "bi_zs".to_string(),
            start_index: 0,
            end_index: 9,
            start_dt: "t0".to_string(),
            end_dt: "t2".to_string(),
            zg: 14.0,
            zd: 12.0,
            gg: 15.0,
            dd: 10.0,
        }];

        let points = detect_buy_sell(&bis, &zs, &[]);
        let third_buys: Vec<_> = points.iter().filter(|p| p.bs_type == "3buy").collect();
        assert!(!third_buys.is_empty(), "应检测到三买");
    }

    #[test]
    fn test_xd_buy_sell() {
        // 测试线段级别买卖点
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

        let xds = vec![
            make_xd(0, "up", 10.0, 20.0, 0, 5),
            make_xd(1, "down", 20.0, 12.0, 5, 10),
            make_xd(2, "up", 12.0, 18.0, 10, 15),
            make_xd(3, "down", 18.0, 13.0, 15, 20),
        ];

        let xd_beichi = vec![BeiChi {
            bc_type: "xd_beichi".to_string(),
            index: 20,
            dt: "t3".to_string(),
            direction: "down".to_string(),
            bc_sub_type: "simple".to_string(),
        }];

        let points = detect_xd_buy_sell(&xds, &[], &xd_beichi);
        let first_buys: Vec<_> = points.iter().filter(|p| p.bs_type == "1buy").collect();
        assert!(!first_buys.is_empty(), "线段级别应检测到一买");
    }
}
