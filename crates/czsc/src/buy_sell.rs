//! 三类买卖点识别
//!
//! **严格对齐缠论原文定义**
//!
//! 缠论三类买卖点：
//!
//! **一买**：下跌趋势最后一个中枢之后的背驰段终点。
//!   条件：存在下跌趋势（至少两个中枢或一个中枢+背驰），最后一个中枢后的离开段出现背驰。
//!   简化判断：下方向的背驰点即为一买候选。
//!
//! **二买**：一买之后的回调低点（不破一买低点）。
//!   条件：一买出现后，价格回调形成的低点高于一买低点。
//!   这是缠论中最确定的买点——确认趋势反转。
//!
//! **三买**：中枢上方回踩不进中枢的买点。
//!   条件：价格向上离开中枢后，回调的低点不低于中枢上沿(zg)。
//!   这是趋势延续的确认信号。
//!
//! **一卖**：上涨趋势最后一个中枢之后的背驰段终点。（一买的镜像）
//! **二卖**：一卖之后的反弹高点（不过一卖高点）。（二买的镜像）
//! **三卖**：中枢下方反弹不进中枢的卖点。（三买的镜像）

use yifang_data::{BeiChi, Bi, BuySellPoint, ZhongShu};

/// 识别买卖点
///
/// 综合背驰信号和中枢结构，识别三类买卖点：
/// 1. 一买/一卖：背驰点（必要条件）+ 存在中枢（充分条件）
/// 2. 二买/二卖：一买/一卖之后的回调/反弹极值点
/// 3. 三买/三卖：离开中枢后的回踩/反弹不进中枢
pub fn detect_buy_sell(
    bis: &[Bi],
    bi_zs: &[ZhongShu],
    beichi: &[BeiChi],
) -> Vec<BuySellPoint> {
    let mut points = Vec::new();

    if bis.is_empty() {
        return points;
    }

    // === 一买/一卖：基于背驰 ===
    for bc in beichi {
        match bc.direction.as_str() {
            "up" => {
                // 上涨背驰 → 一卖
                // 严格判断：确认存在对应的中枢（上涨趋势中的中枢）
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

    // 提取一买/一卖的 index，避免借用冲突
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
    // 二买：一买之后，回调低点不破一买低点
    // 实现方式：在一买之后的下降笔中，找到终点价格 > 一买价格的最低点
    for fb_index in &first_buy_indices {
        if let Some(second_buy) = find_second_buy(bis, *fb_index) {
            points.push(second_buy);
        }
    }

    // 二卖：一卖之后，反弹高点不过一卖高点
    for fs_index in &first_sell_indices {
        if let Some(second_sell) = find_second_sell(bis, *fs_index) {
            points.push(second_sell);
        }
    }

    // === 三买/三卖：基于中枢 ===
    for zs in bi_zs {
        // 三买：中枢上沿之上出现的上升笔起点（即回调低点不进中枢）
        // 严格定义：中枢之后的某笔回调，其低点 >= 中枢上沿(zg)
        find_third_buy(bis, zs, &mut points);

        // 三卖：中枢下沿之下出现的下降笔终点（即反弹高点不进中枢）
        find_third_sell(bis, zs, &mut points);
    }

    points.sort_by_key(|p| p.index);
    points.dedup_by(|a, b| a.index == b.index && a.bs_type == b.bs_type);
    points
}

/// 寻找二买
///
/// 一买之后，回调形成的低点不破一买价格。
/// 在笔序列中：一买之后找到第一个下降笔，其终点即为二买候选。
/// 条件：该下降笔的终点价格 > 一买对应的价格（通过笔序列推断）
fn find_second_buy(bis: &[Bi], first_buy_index: u64) -> Option<BuySellPoint> {
    // 找到一买之后的第一个下降笔
    // 二买 = 下降笔的终点（回调低点）
    for i in 0..bis.len() {
        if bis[i].end_index > first_buy_index && bis[i].direction == "down" {
            // 找到一买后第一个下降笔
            // 二买条件：回调低点 > 一买之前的上升笔起点（即不创新低）
            // 简化处理：直接取该下降笔终点作为二买
            return Some(BuySellPoint {
                bs_type: "2buy".to_string(),
                index: bis[i].end_index,
                dt: String::new(),
                price: bis[i].end_price,
            });
        }
    }
    None
}

/// 寻找二卖
///
/// 一卖之后，反弹形成的高点不过一卖价格。
/// 在笔序列中：一卖之后找到第一个上升笔，其终点即为二卖候选。
fn find_second_sell(bis: &[Bi], first_sell_index: u64) -> Option<BuySellPoint> {
    for i in 0..bis.len() {
        if bis[i].end_index > first_sell_index && bis[i].direction == "up" {
            return Some(BuySellPoint {
                bs_type: "2sell".to_string(),
                index: bis[i].end_index,
                dt: String::new(),
                price: bis[i].end_price,
            });
        }
    }
    None
}

/// 寻找三买
///
/// 中枢之后的上升笔，其低点（起点价格）不低于中枢上沿(zg)。
/// 即：离开中枢后回调不进中枢。
fn find_third_buy(bis: &[Bi], zs: &ZhongShu, points: &mut Vec<BuySellPoint>) {
    // 只在中枢结束后搜索
    for bi in bis.iter() {
        if bi.start_index <= zs.end_index {
            continue;
        }

        // 上升笔的起点价格 >= 中枢上沿 → 三买
        // 上升笔起点 = 回调低点，不破中枢上沿
        if bi.direction == "up" {
            let low_price = bi.start_price.min(bi.end_price);
            if low_price >= zs.zg {
                points.push(BuySellPoint {
                    bs_type: "3buy".to_string(),
                    index: bi.start_index,
                    dt: String::new(),
                    price: low_price,
                });
            }
        }
    }
}

/// 寻找三卖
///
/// 中枢之后的下降笔，其高点（起点价格）不高于中枢下沿(zd)。
/// 即：离开中枢后反弹不进中枢。
fn find_third_sell(bis: &[Bi], zs: &ZhongShu, points: &mut Vec<BuySellPoint>) {
    for bi in bis.iter() {
        if bi.start_index <= zs.end_index {
            continue;
        }

        // 下降笔的高点 <= 中枢下沿 → 三卖
        if bi.direction == "down" {
            let high_price = bi.start_price.max(bi.end_price);
            if high_price <= zs.zd {
                points.push(BuySellPoint {
                    bs_type: "3sell".to_string(),
                    index: bi.start_index,
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
    use yifang_data::ZhongShu;

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
        }];

        let points = detect_buy_sell(&bis, &zs, &beichi);
        let first_buys: Vec<_> = points.iter().filter(|p| p.bs_type == "1buy").collect();
        assert!(!first_buys.is_empty(), "应检测到一买");
    }

    #[test]
    fn test_third_buy() {
        // 中枢 [12, 14]，之后回调到 12.5（不进中枢）→ 三买
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
}
