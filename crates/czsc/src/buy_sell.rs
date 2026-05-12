//! 三类买卖点识别
//!
//! 缠论买卖点：
//! - 一买：下跌趋势最后一个中枢之后的背驰段终点
//! - 二买：一买之后的回调低点（不破一买低点）
//! - 三买：中枢上方回踩不进中枢的买点
//!
//! - 一卖：上涨趋势最后一个中枢之后的背驰段终点
//! - 二卖：一卖之后的反弹高点（不过一卖高点）
//! - 三卖：中枢下方反弹不进中枢的卖点

use yifang_data::{BeiChi, Bi, BuySellPoint, ZhongShu};

/// 识别买卖点
pub fn detect_buy_sell(
    bis: &[Bi],
    bi_zs: &[ZhongShu],
    beichi: &[BeiChi],
) -> Vec<BuySellPoint> {
    let mut points = Vec::new();

    if bis.is_empty() {
        return points;
    }

    // 遍历背驰点，作为一买/一卖的候选
    for bc in beichi {
        match bc.direction.as_str() {
            "up" => {
                // 上涨背驰 → 一卖
                points.push(BuySellPoint {
                    bs_type: "1sell".to_string(),
                    index: bc.index,
                    dt: bc.dt.clone(),
                    price: 0.0, // 前端根据 index 查 K 线价格
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

    // 在一买之后找二买
    for fb_index in &first_buy_indices {
        if let Some(second_buy) = find_second_point(bis, *fb_index, "buy") {
            points.push(second_buy);
        }
    }

    // 在一卖之后找二卖
    for fs_index in &first_sell_indices {
        if let Some(second_sell) = find_second_point(bis, *fs_index, "sell") {
            points.push(second_sell);
        }
    }

    // 识别三买/三卖：基于中枢
    for zs in bi_zs {
        // 中枢之后的笔
        for bi in bis.iter().rev() {
            if bi.end_index <= zs.end_index {
                break;
            }

            match bi.direction.as_str() {
                "up" => {
                    // 上升笔低点在中枢上沿之上 → 三买
                    if bi.start_price > zs.zg {
                        points.push(BuySellPoint {
                            bs_type: "3buy".to_string(),
                            index: bi.start_index,
                            dt: String::new(),
                            price: bi.start_price,
                        });
                    }
                }
                "down" => {
                    // 下降笔高点在中枢下沿之下 → 三卖
                    if bi.end_price < zs.zd {
                        points.push(BuySellPoint {
                            bs_type: "3sell".to_string(),
                            index: bi.end_index,
                            dt: String::new(),
                            price: bi.end_price,
                        });
                    }
                }
                _ => {}
            }
        }
    }

    points.sort_by_key(|p| p.index);
    points
}

/// 在一买/一卖之后寻找二买/二卖点
fn find_second_point(bis: &[Bi], first_index: u64, bs_side: &str) -> Option<BuySellPoint> {
    match bs_side {
        "buy" => {
            // 一买之后，找下一个向上笔的起点（回调低点）
            for bi in bis.iter() {
                if bi.start_index > first_index && bi.direction == "up" {
                    return Some(BuySellPoint {
                        bs_type: "2buy".to_string(),
                        index: bi.start_index,
                        dt: String::new(),
                        price: bi.start_price,
                    });
                }
            }
        }
        "sell" => {
            // 一卖之后，找下一个向下笔的起点（反弹高点）
            for bi in bis.iter() {
                if bi.start_index > first_index && bi.direction == "down" {
                    return Some(BuySellPoint {
                        bs_type: "2sell".to_string(),
                        index: bi.start_index,
                        dt: String::new(),
                        price: bi.start_price,
                    });
                }
            }
        }
        _ => {}
    }

    None
}
