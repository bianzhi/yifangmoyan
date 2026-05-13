//! 背驰检测
//!
//! **对齐缠论原文定义**
//!
//! 背驰（背弛）是缠论的核心概念之一：
//! - 趋势力度减弱：同方向两段走势，后一段的力度小于前一段
//! - 力度可以用 MACD 面积、价格幅度、成交量等衡量
//!
//! 缠论中背驰的严格定义：
//! - **趋势背驰**：在趋势中（至少两个同级别中枢），最后一个中枢之后的离开段
//!   力度小于前一个离开段的力度，称为趋势背驰。
//! - **盘整背驰**：在盘整中（只有一个中枢），围绕中枢震荡的离开段
//!   力度小于前一个同方向离开段。
//!
//! 当前实现为简化版本，基于相邻同方向笔/线段的力度比较：
//! - 至少需要两根同方向的笔/线段
//! - 后一根力度 < 前一根力度 → 疑似背驰
//! - 力度 = MACD 面积 × 价格幅度（综合判断）

use yifang_data::{Bi, BeiChi, MacdData, XianDuan};

/// 笔背驰检测
///
/// 比较同方向相邻两笔的力度差异。
/// 力度用价格幅度比 + MACD 面积比 来衡量。
pub fn detect_bi_beichi(bis: &[Bi], macd: &MacdData) -> Vec<BeiChi> {
    detect_beichi_from_segments(
        bis,
        macd,
        "bi_beichi",
        |bi| (bi.start_index, bi.end_index, bi.direction.clone(), bi.start_price, bi.end_price),
    )
}

/// 线段背驰检测
pub fn detect_xd_beichi(xds: &[XianDuan], macd: &MacdData) -> Vec<BeiChi> {
    detect_beichi_from_segments(
        xds,
        macd,
        "xd_beichi",
        |xd| (xd.start_index, xd.end_index, xd.direction.clone(), xd.start_price, xd.end_price),
    )
}

fn detect_beichi_from_segments<T, F>(
    segments: &[T],
    macd: &MacdData,
    bc_type: &str,
    extract: F,
) -> Vec<BeiChi>
where
    F: Fn(&T) -> (u64, u64, String, f64, f64),
{
    let mut results = Vec::new();

    if segments.len() < 4 {
        return results;
    }

    // 找同方向相邻段对进行比较
    let mut last_up_idx: Option<usize> = None;
    let mut last_down_idx: Option<usize> = None;

    for (i, seg) in segments.iter().enumerate() {
        let (start_idx, end_idx, direction, start_price, end_price) = extract(seg);

        match direction.as_str() {
            "up" => {
                if let Some(prev_i) = last_up_idx {
                    let (prev_start, prev_end, _, prev_sp, prev_ep) = extract(&segments[prev_i]);
                    let prev_amplitude = (prev_ep - prev_sp).abs();
                    let curr_amplitude = (end_price - start_price).abs();

                    // MACD 面积比较
                    let prev_macd_area = calc_macd_area(macd, prev_start as usize, prev_end as usize);
                    let curr_macd_area = calc_macd_area(macd, start_idx as usize, end_idx as usize);

                    // 背驰判断：综合价格力度和MACD面积
                    // 价格力度减弱（后笔幅度 < 前笔幅度）
                    // MACD面积减弱（后笔MACD面积 < 前笔MACD面积）
                    // 两者同时满足才判定为背驰
                    let price_divergence = curr_amplitude < prev_amplitude;
                    let macd_divergence = curr_macd_area < prev_macd_area;

                    if price_divergence && macd_divergence {
                        results.push(BeiChi {
                            bc_type: bc_type.to_string(),
                            index: end_idx,
                            dt: String::new(), // 由前端填充
                            direction: "up".to_string(),
                        });
                    }
                }
                last_up_idx = Some(i);
            }
            "down" => {
                if let Some(prev_i) = last_down_idx {
                    let (prev_start, prev_end, _, prev_sp, prev_ep) = extract(&segments[prev_i]);
                    let prev_amplitude = (prev_ep - prev_sp).abs();
                    let curr_amplitude = (end_price - start_price).abs();

                    let prev_macd_area = calc_macd_area(macd, prev_start as usize, prev_end as usize);
                    let curr_macd_area = calc_macd_area(macd, start_idx as usize, end_idx as usize);

                    let price_divergence = curr_amplitude < prev_amplitude;
                    let macd_divergence = curr_macd_area < prev_macd_area;

                    if price_divergence && macd_divergence {
                        results.push(BeiChi {
                            bc_type: bc_type.to_string(),
                            index: end_idx,
                            dt: String::new(),
                            direction: "down".to_string(),
                        });
                    }
                }
                last_down_idx = Some(i);
            }
            _ => {}
        }
    }

    results
}

/// 计算 MACD 柱状图面积（绝对值之和）
fn calc_macd_area(macd: &MacdData, start: usize, end: usize) -> f64 {
    if macd.macd_hist.is_empty() {
        return 0.0;
    }
    let start = start.min(macd.macd_hist.len() - 1);
    let end = end.min(macd.macd_hist.len() - 1);
    macd.macd_hist[start..=end].iter().map(|v| v.abs()).sum()
}
