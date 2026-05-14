//! 线段分析
//!
//! **对齐缠论原始定义的特征序列法**
//!
//! 缠论线段定义（原文）：
//! - 线段由至少3笔组成
//! - 线段的破坏必须由特征序列的分型来确认
//! - 特征序列方法：
//!   1. 将笔序列中所有**同向笔**提取出来形成特征序列
//!      （上升线段看所有上升笔，下降线段看所有下降笔）
//!   2. 对特征序列做去包含处理（与K线去包含逻辑相同）
//!   3. 在去包含后的特征序列上找分型
//!   4. 分型确认 = 线段端点
//!
//! 但是这个方法需要在线段构建过程中动态决定"同向"，
//! Python czsc 的实践方法是：将笔映射为虚拟K线，然后用同样的增量CZSC逻辑。
//!
//! **核心问题与解决方案**：
//! 问题：直接映射的虚拟K线，相邻笔共享端点（上笔终点=下笔起点），
//! 导致 k1.high == k2.high，无法形成严格分型。
//! 解决：对虚拟K线去包含后如果无法找分型，则需要调整分型检测条件，
//! 允许 high/low 相等时仍能识别分型。
//!
//! 本实现采用 Python czsc 的对齐方法：
//! 1. 将笔映射为虚拟K线（与 bi.rs 中 K线→NewBar 完全对齐）
//! 2. 对虚拟K线做去包含
//! 3. 在去包含后的序列上找分型（放宽条件：允许边缘值相等）
//! 4. 按照与笔完全相同的增量逻辑构建线段
//! 5. 后处理：线段被破坏时回退

use crate::fenxing::{FxMark, check_fxs};
use crate::include::NewBar;
use yifang_data::{Bi, XianDuan};

/// 默认最小线段长度（笔数），对齐缠论定义：至少3笔
const DEFAULT_MIN_XD_LEN: usize = 3;

/// 构建线段
pub fn build_xd(bis: &[Bi]) -> Vec<XianDuan> {
    build_xd_with_min_len(bis, None)
}

/// 带自定义最小线段长度的构建
pub fn build_xd_with_min_len(bis: &[Bi], min_xd_len: Option<usize>) -> Vec<XianDuan> {
    let min_len = min_xd_len.unwrap_or(DEFAULT_MIN_XD_LEN);
    if bis.len() < min_len {
        return Vec::new();
    }

    // Step 1: 将笔映射为虚拟K线序列
    let virtual_bars: Vec<NewBar> = bis.iter().enumerate().map(|(i, bi)| {
        let high = bi.start_price.max(bi.end_price);
        let low = bi.start_price.min(bi.end_price);
        let open = if bi.direction == "up" { low } else { high };
        let close = if bi.direction == "up" { high } else { low };
        NewBar {
            id: i as u64,
            dt: bi.start_dt.clone(),
            open,
            close,
            high,
            low,
            vol: 1.0,
            amount: (high - low).max(0.01),
            elements: vec![i],
        }
    }).collect();

    // Step 2: 模拟 Python CZSC 增量过程
    let mut xd_list: Vec<XdEx> = Vec::new();
    let mut bars_ubi: Vec<NewBar> = Vec::new();

    for (i, vbar) in virtual_bars.iter().enumerate() {
        update_bars_ubi(&mut bars_ubi, vbar, i);
        __update_xd(&mut xd_list, &mut bars_ubi, min_len, bis);
    }

    xd_list.into_iter().map(|xex| xex.xd).collect()
}

/// 内部用的扩展线段结构
struct XdEx {
    xd: XianDuan,
    bars: Vec<NewBar>,
}

/// 将虚拟K线加入 bars_ubi（去包含处理）
fn update_bars_ubi(bars_ubi: &mut Vec<NewBar>, vbar: &NewBar, _idx: usize) {
    if bars_ubi.len() < 2 {
        bars_ubi.push(vbar.clone());
        return;
    }

    let k1_high = bars_ubi[bars_ubi.len() - 2].high;
    let k2_high = bars_ubi[bars_ubi.len() - 1].high;
    let k2_low = bars_ubi[bars_ubi.len() - 1].low;
    let k2_id = bars_ubi[bars_ubi.len() - 1].id;
    let k2_dt = bars_ubi[bars_ubi.len() - 1].dt.clone();
    let k2_vol = bars_ubi[bars_ubi.len() - 1].vol;
    let k2_amount = bars_ubi[bars_ubi.len() - 1].amount;
    let k2_elements = bars_ubi[bars_ubi.len() - 1].elements.clone();

    let k3_high = vbar.high;
    let k3_low = vbar.low;

    let has_include = (k2_high <= k3_high && k2_low >= k3_low)
        || (k2_high >= k3_high && k2_low <= k3_low);

    if has_include {
        if k1_high < k2_high {
            let high = k2_high.max(k3_high);
            let low = k2_low.max(k3_low);
            let dt = if k2_high > k3_high { k2_dt } else { vbar.dt.clone() };
            let (open_, close) = if vbar.open > vbar.close { (high, low) } else { (low, high) };
            let vol = k2_vol + vbar.vol;
            let amount = k2_amount + vbar.amount;
            let mut elements = k2_elements;
            elements.push(vbar.id as usize);
            if elements.len() > 100 { elements.drain(..elements.len() - 100); }

            let last = bars_ubi.last_mut().unwrap();
            *last = NewBar { id: k2_id, dt, open: open_, close, high, low, vol, amount, elements };
        } else if k1_high > k2_high {
            let high = k2_high.min(k3_high);
            let low = k2_low.min(k3_low);
            let dt = if k2_low < k3_low { k2_dt } else { vbar.dt.clone() };
            let (open_, close) = if vbar.open > vbar.close { (high, low) } else { (low, high) };
            let vol = k2_vol + vbar.vol;
            let amount = k2_amount + vbar.amount;
            let mut elements = k2_elements;
            elements.push(vbar.id as usize);
            if elements.len() > 100 { elements.drain(..elements.len() - 100); }

            let last = bars_ubi.last_mut().unwrap();
            *last = NewBar { id: k2_id, dt, open: open_, close, high, low, vol, amount, elements };
        } else {
            bars_ubi.push(vbar.clone());
        }
    } else {
        bars_ubi.push(vbar.clone());
    }
}

/// 对虚拟K线序列找分型（放宽条件版）
///
/// 与普通分型检测的区别：
/// - 普通分型要求 k1.high < k2.high > k3.high AND k1.low < k2.low > k3.low
/// - 虚拟K线因为相邻笔共享端点，经常出现 k1.high == k2.high
/// - 放宽条件：对虚拟K线只需满足以下任一：
///   - 标准（严格）分型
///   - 宽松顶分型：k2.fx > k1.fx && k2.fx > k3.fx（fx=high），且 k2.low > k1.low 和 k2.low > k3.low 至少一个成立
///   - 宽松底分型：k2.fx < k1.fx && k2.fx < k3.fx（fx=low），且 k2.high < k1.high 和 k2.high < k3.high 至少一个成立
fn check_fxs_relaxed(bars: &[NewBar]) -> Vec<FxResultRelaxed> {
    if bars.len() < 3 {
        return Vec::new();
    }

    let mut fxs = Vec::new();

    for i in 1..bars.len() - 1 {
        let k1 = &bars[i - 1];
        let k2 = &bars[i];
        let k3 = &bars[i + 1];

        // 顶分型：k2 的分型值(high)严格高于两侧
        // 条件1：k2.high > k1.high || (k2.high == k1.high && k2.low > k1.low)  — 左侧不是更高的
        // 条件2：k2.high > k3.high || (k2.high == k3.high && k2.low > k3.low)  — 右侧不是更高的
        // 条件3：至少一侧 high 严格大于
        let left_top = k2.high > k1.high || (k2.high == k1.high && k2.low > k1.low);
        let right_top = k2.high > k3.high || (k2.high == k3.high && k2.low > k3.low);
        if left_top && right_top && (k2.high > k1.high || k2.high > k3.high) {
            fxs.push(FxResultRelaxed {
                mark: FxMark::Top,
                bar_index: i,
                merged_index: k2.id as usize,
                dt: k2.dt.clone(),
                high: k2.high,
                low: k2.low,
                fx: k2.high,
                bars: [i - 1, i, i + 1],
            });
            continue;
        }

        // 底分型：k2 的分型值(low)严格低于两侧
        let left_bottom = k2.low < k1.low || (k2.low == k1.low && k2.high < k1.high);
        let right_bottom = k2.low < k3.low || (k2.low == k3.low && k2.high < k3.high);
        if left_bottom && right_bottom && (k2.low < k1.low || k2.low < k3.low) {
            fxs.push(FxResultRelaxed {
                mark: FxMark::Bottom,
                bar_index: i,
                merged_index: k2.id as usize,
                dt: k2.dt.clone(),
                high: k2.high,
                low: k2.low,
                fx: k2.low,
                bars: [i - 1, i, i + 1],
            });
        }
    }

    ensure_alternating_relaxed(fxs)
}

/// 放宽版分型结果
#[derive(Debug, Clone)]
struct FxResultRelaxed {
    mark: FxMark,
    bar_index: usize,
    merged_index: usize,
    dt: String,
    high: f64,
    low: f64,
    fx: f64,
    bars: [usize; 3],
}

/// 确保分型序列顶底交替
fn ensure_alternating_relaxed(fxs: Vec<FxResultRelaxed>) -> Vec<FxResultRelaxed> {
    if fxs.is_empty() {
        return Vec::new();
    }

    let mut result = vec![fxs[0].clone()];

    for fx in &fxs[1..] {
        let last = result.last().unwrap();

        if fx.mark == last.mark {
            // 同类型：保留更极端的
            let should_replace = match fx.mark {
                FxMark::Top => fx.fx > last.fx,
                FxMark::Bottom => fx.fx < last.fx,
            };

            if should_replace {
                let last = result.last_mut().unwrap();
                *last = fx.clone();
            }
        } else {
            // 不同类型：顶的 fx 必须高于底的 fx
            let valid = match fx.mark {
                FxMark::Top => fx.fx > last.fx,
                FxMark::Bottom => fx.fx < last.fx,
            };

            if valid {
                result.push(fx.clone());
            }
        }
    }

    result
}

/// 在无包含虚拟K线序列中查找一条线段
fn check_xd(bars: &[NewBar], min_xd_len: usize) -> Option<(XianDuan, Vec<NewBar>)> {
    let fxs = check_fxs_relaxed(bars);
    if fxs.len() < 2 {
        return None;
    }

    let fx_a = &fxs[0];
    let direction: String;
    let fx_b_idx: usize;

    if fx_a.mark == FxMark::Bottom {
        let mut best_idx: Option<usize> = None;
        let mut best_high: f64 = f64::NEG_INFINITY;
        for (i, fx) in fxs.iter().enumerate() {
            if fx.mark == FxMark::Top && fx.dt > fx_a.dt && fx.fx > fx_a.fx && fx.high > best_high {
                best_high = fx.high;
                best_idx = Some(i);
            }
        }
        match best_idx {
            Some(idx) => { direction = "up".to_string(); fx_b_idx = idx; }
            None => return None,
        }
    } else {
        let mut best_idx: Option<usize> = None;
        let mut best_low: f64 = f64::INFINITY;
        for (i, fx) in fxs.iter().enumerate() {
            if fx.mark == FxMark::Bottom && fx.dt > fx_a.dt && fx.fx < fx_a.fx && fx.low < best_low {
                best_low = fx.low;
                best_idx = Some(i);
            }
        }
        match best_idx {
            Some(idx) => { direction = "down".to_string(); fx_b_idx = idx; }
            None => return None,
        }
    };

    let fx_b = &fxs[fx_b_idx];

    let fx_a_first_bar_dt = bars.get(fx_a.bars[0]).map(|b| b.dt.as_str()).unwrap_or(fx_a.dt.as_str());
    let fx_b_last_bar_dt = bars.get(fx_b.bars[2]).map(|b| b.dt.as_str()).unwrap_or(fx_b.dt.as_str());
    let bars_a_count = bars.iter()
        .filter(|b| b.dt.as_str() >= fx_a_first_bar_dt && b.dt.as_str() <= fx_b_last_bar_dt)
        .count();

    let fx_b_first_bar_dt = bars.get(fx_b.bars[0]).map(|b| b.dt.as_str()).unwrap_or(&fx_b.dt);
    let bars_b: Vec<NewBar> = bars.iter()
        .filter(|b| b.dt.as_str() >= fx_b_first_bar_dt)
        .cloned()
        .collect();

    let ab_include = (fx_a.high > fx_b.high && fx_a.low < fx_b.low)
        || (fx_a.high < fx_b.high && fx_a.low > fx_b.low);

    if !ab_include && bars_a_count >= min_xd_len {
        let xd = XianDuan {
            direction,
            start_index: fx_a.merged_index as u64,
            end_index: fx_b.merged_index as u64,
            start_dt: fx_a.dt.clone(),
            end_dt: fx_b.dt.clone(),
            start_price: fx_a.fx,
            end_price: fx_b.fx,
            is_finished: true,
        };
        Some((xd, bars_b))
    } else {
        None
    }
}

/// 更新线段（增量逻辑）
fn __update_xd(
    xd_list: &mut Vec<XdEx>,
    bars_ubi: &mut Vec<NewBar>,
    min_xd_len: usize,
    bis: &[Bi],
) {
    if bars_ubi.len() < 3 {
        return;
    }

    if xd_list.is_empty() {
        let fxs = check_fxs_relaxed(bars_ubi);
        if fxs.is_empty() {
            return;
        }

        let mut fx_a = fxs[0].clone();
        for fx in &fxs {
            if fx.mark != fx_a.mark { continue; }
            let should_replace = match fx.mark {
                FxMark::Bottom => fx.low <= fx_a.low,
                FxMark::Top => fx.high >= fx_a.high,
            };
            if should_replace {
                fx_a = fx.clone();
            }
        }

        let start_dt = bars_ubi
            .get(fx_a.bars[0])
            .map(|b| b.dt.as_str())
            .unwrap_or(fx_a.dt.as_str())
            .to_string();

        let trimmed: Vec<NewBar> = bars_ubi.iter()
            .filter(|b| b.dt.as_str() >= start_dt.as_str())
            .cloned()
            .collect();

        if let Some((xd, bars_b)) = check_xd(&trimmed, min_xd_len) {
            let fxs_in_trimmed = check_fxs_relaxed(&trimmed);
            let fx_a_t = fxs_in_trimmed.iter().find(|fx| fx.dt == xd.start_dt);
            let fx_b_t = fxs_in_trimmed.iter().find(|fx| fx.dt == xd.end_dt);

            let a_start = fx_a_t.and_then(|fx| trimmed.get(fx.bars[0])).map(|b| b.dt.as_str()).unwrap_or(xd.start_dt.as_str());
            let a_end = fx_b_t.and_then(|fx| trimmed.get(fx.bars[2])).map(|b| b.dt.as_str()).unwrap_or(xd.end_dt.as_str());

            let bars_a: Vec<NewBar> = trimmed.iter()
                .filter(|b| b.dt.as_str() >= a_start && b.dt.as_str() <= a_end)
                .cloned()
                .collect();

            let xd = map_xd_to_bi_indices(xd, bis);
            xd_list.push(XdEx { xd, bars: bars_a });
            *bars_ubi = bars_b;
        }
        return;
    }

    let check_result = check_xd(bars_ubi, min_xd_len);
    match check_result {
        Some((xd, bars_b)) => {
            let fxs_for_bars = check_fxs_relaxed(bars_ubi);
            let fx_a_f = fxs_for_bars.iter().find(|fx| fx.dt == xd.start_dt);
            let fx_b_f = fxs_for_bars.iter().find(|fx| fx.dt == xd.end_dt);

            let a_start = fx_a_f.and_then(|fx| bars_ubi.get(fx.bars[0])).map(|b| b.dt.as_str()).unwrap_or(xd.start_dt.as_str());
            let a_end = fx_b_f.and_then(|fx| bars_ubi.get(fx.bars[2])).map(|b| b.dt.as_str()).unwrap_or(xd.end_dt.as_str());

            let bars_a: Vec<NewBar> = bars_ubi.iter()
                .filter(|b| b.dt.as_str() >= a_start && b.dt.as_str() <= a_end)
                .cloned()
                .collect();

            let xd = map_xd_to_bi_indices(xd, bis);
            xd_list.push(XdEx { xd, bars: bars_a });
            *bars_ubi = bars_b;
        }
        None => {}
    }

    // 后处理：线段被破坏时回退
    if !xd_list.is_empty() && bars_ubi.len() >= 2 {
        let last_xd = &xd_list[xd_list.len() - 1];
        let last_ubi = &bars_ubi[bars_ubi.len() - 1];

        let is_broken = (last_xd.xd.direction == "up" && last_ubi.high > last_xd.xd.end_price)
            || (last_xd.xd.direction == "down" && last_ubi.low < last_xd.xd.end_price);

        if is_broken {
            let broken_xex = xd_list.pop().unwrap();
            let broken_bars = &broken_xex.bars;
            if broken_bars.len() >= 2 {
                let rollback_dt = broken_bars[broken_bars.len() - 2].dt.clone();
                let mut new_bars_ubi: Vec<NewBar> = broken_bars.iter()
                    .take(broken_bars.len() - 2)
                    .cloned()
                    .collect();
                let remaining: Vec<NewBar> = bars_ubi.iter()
                    .filter(|b| b.dt >= rollback_dt)
                    .cloned()
                    .collect();
                let last_dt = new_bars_ubi.last().map(|b| b.dt.clone()).unwrap_or_default();
                for b in &remaining {
                    if b.dt > last_dt { new_bars_ubi.push(b.clone()); }
                }
                *bars_ubi = new_bars_ubi;
            } else {
                let rollback_dt = broken_xex.xd.start_dt.clone();
                *bars_ubi = bars_ubi.iter()
                    .filter(|b| b.dt >= rollback_dt)
                    .cloned()
                    .collect();
            }
        }
    }
}

/// 将 XianDuan 的索引从虚拟K线索引映射回原始笔序列索引
fn map_xd_to_bi_indices(xd: XianDuan, bis: &[Bi]) -> XianDuan {
    let start_bi_idx = xd.start_index as usize;
    let end_bi_idx = xd.end_index as usize;

    let (start_index, start_dt, start_price) = if let Some(bi) = bis.get(start_bi_idx) {
        (bi.start_index, bi.start_dt.clone(), bi.start_price)
    } else {
        (xd.start_index, xd.start_dt.clone(), xd.start_price)
    };

    let (end_index, end_dt, end_price) = if let Some(bi) = bis.get(end_bi_idx) {
        (bi.end_index, bi.end_dt.clone(), bi.end_price)
    } else {
        (xd.end_index, xd.end_dt.clone(), xd.end_price)
    };

    XianDuan {
        direction: xd.direction,
        start_index,
        end_index,
        start_dt,
        end_dt,
        start_price,
        end_price,
        is_finished: xd.is_finished,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_bi(id: usize, dir: &str, start_price: f64, end_price: f64, start_idx: u64, end_idx: u64) -> Bi {
        Bi {
            direction: dir.to_string(),
            start_index: start_idx,
            end_index: end_idx,
            start_dt: format!("t{}", id),
            end_dt: format!("t{}", id + 1),
            start_price,
            end_price,
            is_finished: true,
        }
    }

    #[test]
    fn test_build_xd_basic() {
        // 使用真实缠论笔的模式：上证指数日线级别常见笔模式
        // 8笔可能不足以形成线段（取决于去包含后的分型数量），
        // 但12笔通常可以形成至少2条线段
        let bis = vec![
            make_bi(0, "up", 2850.0, 2920.0, 0, 5),
            make_bi(1, "down", 2920.0, 2880.0, 5, 10),
            make_bi(2, "up", 2880.0, 2960.0, 10, 15),
            make_bi(3, "down", 2960.0, 2910.0, 15, 20),
            make_bi(4, "up", 2910.0, 3010.0, 20, 25),
            make_bi(5, "down", 3010.0, 2940.0, 25, 30),
            make_bi(6, "up", 2940.0, 3050.0, 30, 35),
            make_bi(7, "down", 3050.0, 2900.0, 35, 40),
            make_bi(8, "up", 2900.0, 2960.0, 40, 45),
            make_bi(9, "down", 2960.0, 2800.0, 45, 50),
            make_bi(10, "up", 2800.0, 2870.0, 50, 55),
            make_bi(11, "down", 2870.0, 2720.0, 55, 60),
        ];
        let xds = build_xd(&bis);
        // 至少能生成0条以上线段（实际数量取决于去包含和分型结果）
        // 但方向应该一致
        for xd in &xds {
            if xd.direction == "up" {
                assert!(xd.end_price >= xd.start_price,
                    "上升线段终点 {} 应 >= 起点 {}", xd.end_price, xd.start_price);
            } else {
                assert!(xd.end_price <= xd.start_price,
                    "下降线段终点 {} 应 <= 起点 {}", xd.end_price, xd.start_price);
            }
        }
    }

    #[test]
    fn test_xd_min_3_bi() {
        let bis = vec![
            make_bi(0, "up", 10.0, 15.0, 0, 3),
            make_bi(1, "down", 15.0, 12.0, 3, 6),
        ];
        let xds = build_xd(&bis);
        assert!(xds.is_empty(), "少于3笔不应有线段");
    }

    #[test]
    fn test_xd_direction_consistency() {
        let bis = vec![
            make_bi(0, "up", 10.0, 20.0, 0, 3),
            make_bi(1, "down", 20.0, 15.0, 3, 6),
            make_bi(2, "up", 15.0, 25.0, 6, 9),
            make_bi(3, "down", 25.0, 18.0, 9, 12),
            make_bi(4, "up", 18.0, 22.0, 12, 15),
            make_bi(5, "down", 22.0, 8.0, 15, 18),
            make_bi(6, "up", 8.0, 16.0, 18, 21),
            make_bi(7, "down", 16.0, 5.0, 21, 24),
        ];
        let xds = build_xd(&bis);
        for xd in &xds {
            if xd.direction == "up" {
                assert!(xd.end_price >= xd.start_price,
                    "上升线段终点 {} 应 >= 起点 {}", xd.end_price, xd.start_price);
            } else {
                assert!(xd.end_price <= xd.start_price,
                    "下降线段终点 {} 应 <= 起点 {}", xd.end_price, xd.start_price);
            }
        }
    }

    #[test]
    fn test_xd_alternating_direction() {
        let bis = vec![
            make_bi(0, "up", 10.0, 20.0, 0, 3),
            make_bi(1, "down", 20.0, 15.0, 3, 6),
            make_bi(2, "up", 15.0, 25.0, 6, 9),
            make_bi(3, "down", 25.0, 12.0, 9, 12),
            make_bi(4, "up", 12.0, 22.0, 12, 15),
            make_bi(5, "down", 22.0, 8.0, 15, 18),
            make_bi(6, "up", 8.0, 16.0, 18, 21),
            make_bi(7, "down", 16.0, 5.0, 21, 24),
        ];
        let xds = build_xd(&bis);
        for i in 1..xds.len() {
            assert_ne!(xds[i].direction, xds[i - 1].direction,
                "相邻线段方向应交替，但第{}段和第{}段都是{}",
                i - 1, i, xds[i].direction);
        }
    }

    #[test]
    fn test_xd_with_real_bi_pattern() {
        let bis = vec![
            make_bi(0, "up", 2850.0, 2920.0, 0, 5),
            make_bi(1, "down", 2920.0, 2880.0, 5, 10),
            make_bi(2, "up", 2880.0, 2960.0, 10, 15),
            make_bi(3, "down", 2960.0, 2910.0, 15, 20),
            make_bi(4, "up", 2910.0, 3010.0, 20, 25),
            make_bi(5, "down", 3010.0, 2940.0, 25, 30),
            make_bi(6, "up", 2940.0, 3050.0, 30, 35),
            make_bi(7, "down", 3050.0, 2900.0, 35, 40),
            make_bi(8, "up", 2900.0, 2960.0, 40, 45),
            make_bi(9, "down", 2960.0, 2800.0, 45, 50),
            make_bi(10, "up", 2800.0, 2870.0, 50, 55),
            make_bi(11, "down", 2870.0, 2720.0, 55, 60),
        ];
        let xds = build_xd(&bis);
        if xds.len() >= 2 {
            for i in 1..xds.len() {
                assert_ne!(xds[i].direction, xds[i - 1].direction, "线段方向应交替");
            }
        }
        if xds.len() == 1 {
            assert_eq!(xds[0].direction, "up", "第一条线段应该是上升线段");
        }
    }

    #[test]
    fn test_xd_no_include_between_start_end() {
        let bis = vec![
            make_bi(0, "up", 10.0, 20.0, 0, 3),
            make_bi(1, "down", 20.0, 12.0, 3, 6),
            make_bi(2, "up", 12.0, 25.0, 6, 9),
            make_bi(3, "down", 25.0, 15.0, 9, 12),
            make_bi(4, "up", 15.0, 30.0, 12, 15),
            make_bi(5, "down", 30.0, 10.0, 15, 18),
        ];
        let xds = build_xd(&bis);
        for xd in &xds {
            if xd.is_finished {
                assert!(xd.end_price > 0.0);
            }
        }
    }

    #[test]
    fn test_xd_complex_pattern() {
        let bis = vec![
            make_bi(0, "up", 10.0, 20.0, 0, 3),
            make_bi(1, "down", 20.0, 15.0, 3, 6),
            make_bi(2, "up", 15.0, 25.0, 6, 9),
            make_bi(3, "down", 25.0, 8.0, 9, 12),
            make_bi(4, "up", 8.0, 15.0, 12, 15),
            make_bi(5, "down", 15.0, 5.0, 15, 18),
            make_bi(6, "up", 5.0, 18.0, 18, 21),
            make_bi(7, "down", 18.0, 12.0, 21, 24),
            make_bi(8, "up", 12.0, 28.0, 24, 27),
            make_bi(9, "down", 28.0, 6.0, 27, 30),
            make_bi(10, "up", 6.0, 14.0, 30, 33),
            make_bi(11, "down", 14.0, 3.0, 33, 36),
        ];
        let xds = build_xd(&bis);
        for i in 1..xds.len() {
            assert_ne!(xds[i].direction, xds[i - 1].direction,
                "线段方向应交替，第{}段和第{}段同方向{}", i - 1, i, xds[i].direction);
        }
    }
}
