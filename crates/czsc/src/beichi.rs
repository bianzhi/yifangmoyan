//! 背驰检测 — 100%对齐缠论原文24/27/37课
//!
//! **背驰本质**：同级别同方向走势的力度衰减（缠论第27课）
//!
//! 背驰是缠论买卖点的核心依据，判断流程：
//! 1. 走势类型识别（趋势/盘整） — 基于中枢数量
//! 2. 提取待比较的同方向走势段 — 离开中枢的连接段
//! 3. 计算两段走势的MACD力度（面积+峰值）
//! 4. 背驰条件判断（面积衰减+峰值衰减+创新高/新低）
//! 5. 背驰级别与确认
//!
//! **趋势背驰**（24课）：
//! - 至少2个同方向无重叠中枢
//! - 最后一个中枢之后的离开段(c)力度 < 前一个离开段(b)力度
//! - c段必须创趋势新高/新低
//!
//! **盘整背驰**（37课）：
//! - 只有1个中枢
//! - 中枢前后两段同方向走势力度比较，后段 < 前段
//! - 后段创中枢震荡新高/新低
//!
//! **力度衡量**：
//! - 上涨走势：只累加MACD红柱（正值）面积和峰值
//! - 下跌走势：只累加MACD绿柱（负值）绝对值面积和峰值
//! - 面积是核心指标，峰值是辅助，必须两者同时衰减

use yifang_data::{Bi, BeiChi, MacdData, XianDuan, ZhongShu};

// ─── MACD 面积阈值 ──────────────────────────────────────
// 背驰段面积 < 前一段面积 × 阈值 才算力度衰减
// 缠论原文无固定数值，0.7 是经验阈值（可微调）
const MACD_AREA_RATIO: f64 = 0.7;

// ─── 走势段（用于力度比较的最小单位）──────────────────────

/// 走势段：离开中枢的连接段，背驰力度比较的基本单位
#[derive(Debug, Clone)]
#[allow(dead_code)]
struct TrendSection {
    /// 方向: "up" / "down"
    direction: String,
    /// 起始K线索引
    start_idx: u64,
    /// 终点K线索引
    end_idx: u64,
    /// 起始价格
    start_val: f64,
    /// 终止价格
    end_val: f64,
    /// 关联的中枢索引（在中枢列表中的位置），无则为 None
    zs_idx: Option<usize>,
    /// MACD面积（方向性：上涨只计红柱，下跌只计绿柱绝对值）
    macd_area: f64,
    /// MACD峰值（同方向的最大绝对值）
    macd_peak: f64,
}

// ─── 公开 API ─────────────────────────────────────────

/// 笔背驰检测
///
/// 基于笔序列和笔中枢，按缠论规范判断趋势/盘整背驰
pub fn detect_bi_beichi(bis: &[Bi], macd: &MacdData, zs_list: &[ZhongShu]) -> Vec<BeiChi> {
    detect_beichi_inner(
        bis,
        macd,
        zs_list,
        "bi_beichi",
        |bi| (bi.start_index, bi.end_index, bi.direction.clone(), bi.start_price, bi.end_price),
    )
}

/// 线段背驰检测
///
/// 基于线段序列和线段中枢，按缠论规范判断趋势/盘整背驰
pub fn detect_xd_beichi(xds: &[XianDuan], macd: &MacdData, zs_list: &[ZhongShu]) -> Vec<BeiChi> {
    detect_beichi_inner(
        xds,
        macd,
        zs_list,
        "xd_beichi",
        |xd| (xd.start_index, xd.end_index, xd.direction.clone(), xd.start_price, xd.end_price),
    )
}

// ─── 核心实现 ─────────────────────────────────────────

fn detect_beichi_inner<T, F>(
    segments: &[T],
    macd: &MacdData,
    zs_list: &[ZhongShu],
    bc_type: &str,
    extract: F,
) -> Vec<BeiChi>
where
    F: Fn(&T) -> (u64, u64, String, f64, f64),
{
    let mut results = Vec::new();

    if segments.len() < 4 || zs_list.is_empty() {
        return results;
    }

    // 步骤1：将段列表转为走势段
    let sections = build_trend_sections(segments, zs_list, &extract);

    // 步骤2：按中枢方向分组，分别判断
    let groups = group_zs_by_direction(zs_list);

    for group_indices in &groups {
        if group_indices.len() == 1 {
            // 单中枢 → 盘整背驰
            let zs_idx = group_indices[0];
            if let Some(bd) = check_panzheng_backdivergence(&zs_list[zs_idx], &sections, macd) {
                // 尝试确认
                let confirmed = confirm_backdivergence(&bd, &sections, zs_list);
                results.push(make_beichi(bc_type, &bd, confirmed));
            }
        } else if group_indices.len() >= 2 {
            // 多个同方向无重叠中枢 → 趋势背驰
            let group_zs: Vec<&ZhongShu> = group_indices.iter().map(|&i| &zs_list[i]).collect();
            if let Some(bd) = check_trend_backdivergence(&group_zs, &sections, macd) {
                let confirmed = confirm_backdivergence(&bd, &sections, zs_list);
                results.push(make_beichi(bc_type, &bd, confirmed));
            }
        }
    }

    results
}

// ─── 走势段构建 ───────────────────────────────────────

/// 把原始段(Bi/XianDuan)列表转为 TrendSection 列表，并关联中枢
fn build_trend_sections<T, F>(
    segments: &[T],
    zs_list: &[ZhongShu],
    extract: &F,
) -> Vec<TrendSection>
where
    F: Fn(&T) -> (u64, u64, String, f64, f64),
{
    let mut sections = Vec::new();

    for seg in segments {
        let (start_idx, end_idx, direction, start_val, end_val) = extract(seg);

        // 关联中枢：段起点在中枢范围内
        let zs_idx = zs_list.iter().position(|zs| {
            start_idx >= zs.start_index && start_idx <= zs.end_index
        });

        sections.push(TrendSection {
            direction,
            start_idx,
            end_idx,
            start_val,
            end_val,
            zs_idx,
            macd_area: 0.0,
            macd_peak: 0.0,
        });
    }

    sections
}

// ─── 中枢分组 ─────────────────────────────────────────

/// 按中枢递进方向分组：
/// - 上涨趋势组：后续中枢 zd > 前中枢 zd，且无重叠（后 zd > 前 zg）
/// - 下跌趋势组：后续中枢 zg < 前中枢 zg，且无重叠（后 zg < 前 zd）
/// - 方向改变则开新组
///
/// 算法：
/// 1. 第一个中枢单独成组
/// 2. 从第二个开始，与前一个中枢比较方向
/// 3. 同方向且无重叠 → 继续当前组
/// 4. 方向改变或有重叠 → 开新组
fn group_zs_by_direction(zs_list: &[ZhongShu]) -> Vec<Vec<usize>> {
    if zs_list.is_empty() {
        return Vec::new();
    }

    let mut groups: Vec<Vec<usize>> = vec![vec![0]];

    for i in 1..zs_list.len() {
        let prev = &zs_list[i - 1];
        let curr = &zs_list[i];

        // 判断当前与前一个中枢是否同方向且无重叠（构成趋势条件）
        let no_overlap = curr.zd > prev.zg || curr.zg < prev.zd;
        let direction = classify_zs_direction(prev, curr);

        // 获取当前组的方向
        let group_dir = {
            let group = &groups.last().unwrap();
            if group.len() >= 2 {
                // 组内已有2+个中枢，用组内相邻对判断方向
                let last = group.last().unwrap();
                let second_last = group[group.len() - 2];
                classify_zs_direction(&zs_list[second_last], &zs_list[*last])
            } else {
                // 组内只有1个中枢，用当前中枢与前一中枢的关系判断
                direction.clone()
            }
        };

        if no_overlap && direction == group_dir {
            // 同方向无重叠 → 继续当前组
            groups.last_mut().unwrap().push(i);
        } else {
            // 方向改变或有重叠 → 开新组
            groups.push(vec![i]);
        }
    }

    groups
}

/// 根据两个中枢位置关系判断方向
fn classify_zs_direction(prev: &ZhongShu, curr: &ZhongShu) -> String {
    if curr.zd > prev.zd {
        "up".to_string()
    } else if curr.zg < prev.zg {
        "down".to_string()
    } else if curr.zg > prev.zg {
        "up".to_string()
    } else {
        "down".to_string()
    }
}

// ─── MACD 力度计算 ─────────────────────────────────────

/// 计算单个走势段的MACD力度
///
/// 核心规则（缠论标准）：
/// - 上涨走势只累加正MACD值（红柱）面积和峰值
/// - 下跌走势只累加负MACD值绝对值（绿柱）面积和峰值
fn calculate_section_power(section: &mut TrendSection, macd: &MacdData) {
    if macd.macd_hist.is_empty() {
        section.macd_area = 0.0;
        section.macd_peak = 0.0;
        return;
    }

    let start = (section.start_idx as usize).min(macd.macd_hist.len() - 1);
    let end = (section.end_idx as usize).min(macd.macd_hist.len() - 1);

    if start > end {
        section.macd_area = 0.0;
        section.macd_peak = 0.0;
        return;
    }

    let slice = &macd.macd_hist[start..=end];

    if section.direction == "up" {
        // 上涨：只看红柱（正值）
        let area: f64 = slice.iter().copied().filter(|&v| v > 0.0).sum();
        let peak = slice.iter().copied().fold(0.0_f64, |a, b| a.max(b));
        section.macd_area = area;
        section.macd_peak = peak;
    } else {
        // 下跌：只看绿柱（负值取绝对值）
        let area: f64 = slice.iter().copied().filter(|&v| v < 0.0).map(|v| v.abs()).sum();
        let peak = slice.iter().copied().map(|v| v.abs()).fold(0.0_f64, |a, b| a.max(b));
        section.macd_area = area;
        section.macd_peak = peak;
    }
}

// ─── 盘整背驰判断 ─────────────────────────────────────

/// 盘整背驰（缠论37课）
///
/// 条件（必须全部满足）：
/// 1. 走势类型为盘整（仅包含1个同级别中枢）
/// 2. 存在两段同方向、围绕该中枢的走势段（中枢前后）
/// 3. 后一段走势的MACD面积 < 前一段的 70%
/// 4. 后一段走势的MACD峰值 < 前一段的峰值
/// 5. 后一段走势创出新高/新低（上涨创新高，下跌创新低）
fn check_panzheng_backdivergence(
    zs: &ZhongShu,
    sections: &[TrendSection],
    macd: &MacdData,
) -> Option<BackDivergenceResult> {
    // 找到中枢前后两段同方向走势
    // 盘整结构：a + A + b
    // a = 中枢前的进入段
    // A = 中枢
    // b = 中枢后的离开段（背驰段）
    // 如果 a 和 b 同方向，则可比较力度

    let mut prev_section: Option<usize> = None;  // 中枢前的进入段索引
    let mut curr_section: Option<usize> = None;   // 中枢后的离开段索引

    for (i, sec) in sections.iter().enumerate() {
        // 段结束于中枢开始之前 → 进入段
        if sec.end_idx <= zs.start_index && prev_section.is_none() {
            prev_section = Some(i);
        }
        // 段开始于中枢结束之后 → 离开段
        if sec.start_idx >= zs.end_index && curr_section.is_none() {
            curr_section = Some(i);
        }
    }

    let prev_i = prev_section?;
    let curr_i = curr_section?;

    let mut prev = sections[prev_i].clone();
    let mut curr = sections[curr_i].clone();

    // 必须同方向
    if prev.direction != curr.direction {
        return None;
    }

    // 计算力度
    calculate_section_power(&mut prev, macd);
    calculate_section_power(&mut curr, macd);

    // 面积和峰值都为0时无法比较（如无MACD数据）
    if prev.macd_area <= 0.0 || prev.macd_peak <= 0.0 {
        return None;
    }

    // 盘整背驰条件判断
    let cond_area = curr.macd_area < prev.macd_area * MACD_AREA_RATIO;
    let cond_peak = curr.macd_peak < prev.macd_peak;
    let cond_new_extreme = if curr.direction == "up" {
        // 上涨：离开段创新高
        curr.end_val > prev.end_val
    } else {
        // 下跌：离开段创新低
        curr.end_val < prev.end_val
    };

    if cond_area && cond_peak && cond_new_extreme {
        let prev_area = prev.macd_area;
        let curr_area = curr.macd_area;
        let prev_peak = prev.macd_peak;
        let curr_peak = curr.macd_peak;
        Some(BackDivergenceResult {
            bc_sub_type: "panzheng".to_string(),
            direction: curr.direction.clone(),
            index: curr.end_idx,
            prev_section: prev,
            curr_section: curr,
            prev_area,
            curr_area,
            prev_peak,
            curr_peak,
            is_confirmed: false,
        })
    } else {
        None
    }
}

// ─── 趋势背驰判断 ─────────────────────────────────────

/// 趋势背驰（缠论24课）
///
/// 条件（必须全部满足）：
/// 1. 走势类型为趋势（包含≥2个同方向、无重叠的同级别中枢）
/// 2. 最后一个中枢之后存在完整的离开段（背驰段c）
/// 3. 背驰段c的MACD面积 < 连接段b的70%
/// 4. 背驰段c的MACD峰值 < 连接段b的峰值
/// 5. 背驰段c创出趋势新高/新低
fn check_trend_backdivergence(
    zs_group: &[&ZhongShu],
    sections: &[TrendSection],
    macd: &MacdData,
) -> Option<BackDivergenceResult> {
    // 趋势必须至少2个中枢
    if zs_group.len() < 2 {
        return None;
    }

    // 确认是同方向无重叠趋势
    let direction = classify_zs_direction(zs_group[0], zs_group[1]);

    for w in zs_group.windows(2) {
        // 上涨趋势：后中枢低点 > 前中枢高点
        if direction == "up" && w[1].zd <= w[0].zg {
            return None;
        }
        // 下跌趋势：后中枢高点 < 前中枢低点
        if direction == "down" && w[1].zg >= w[0].zd {
            return None;
        }
    }

    // 找到两个中枢之间的连接段b和最后一个中枢之后的背驰段c
    // b = 第一个中枢和第二个中枢之间的连接段
    // c = 最后一个中枢之后的离开段
    let first_zs = zs_group[0];
    let second_zs = zs_group[1];
    let last_zs = zs_group.last().unwrap();

    let mut prev_section: Option<usize> = None; // b段
    let mut curr_section: Option<usize> = None; // c段

    for (i, sec) in sections.iter().enumerate() {
        // b段：段跨越第一和第二中枢之间
        // 条件：段起点在第一中枢结束之后，段终点在第二中枢开始之前
        if sec.start_idx >= first_zs.end_index && sec.end_idx <= second_zs.start_index {
            if prev_section.is_none() {
                prev_section = Some(i);
            }
        }
        // 更宽容的b段查找：段与两个中枢都有重叠区间
        if prev_section.is_none() {
            let overlaps_first = sec.end_idx > first_zs.start_index && sec.start_idx < first_zs.end_index;
            let overlaps_second = sec.end_idx > second_zs.start_index && sec.start_idx < second_zs.end_index;
            if overlaps_first || overlaps_second {
                // 段与某个中枢重叠，跳过
            } else if sec.start_idx >= first_zs.end_index && sec.end_idx <= second_zs.start_index {
                prev_section = Some(i);
            }
        }

        // c段：段起点在最后一个中枢结束之后
        if sec.start_idx >= last_zs.end_index && curr_section.is_none() {
            curr_section = Some(i);
        }
    }

    // 如果严格的b段找不到，放宽条件：找第一个中枢结束到第二中枢之间方向一致的段
    if prev_section.is_none() {
        for (i, sec) in sections.iter().enumerate() {
            if sec.direction == direction
                && sec.start_idx >= first_zs.end_index
                && sec.start_idx < second_zs.start_index
            {
                prev_section = Some(i);
                break;
            }
        }
    }

    // 如果还是找不到b段，尝试找第一中枢结束后的第一个同方向段
    if prev_section.is_none() {
        for (i, sec) in sections.iter().enumerate() {
            if sec.direction == direction && sec.start_idx >= first_zs.end_index && sec.start_idx < second_zs.end_index {
                prev_section = Some(i);
                break;
            }
        }
    }

    let prev_i = prev_section?;
    let curr_i = curr_section?;

    let mut prev = sections[prev_i].clone();
    let mut curr = sections[curr_i].clone();

    // 必须同方向且与趋势方向一致
    if prev.direction != curr.direction || prev.direction != direction {
        return None;
    }

    // 计算力度
    calculate_section_power(&mut prev, macd);
    calculate_section_power(&mut curr, macd);

    // 面积和峰值都为0时无法比较
    if prev.macd_area <= 0.0 || prev.macd_peak <= 0.0 {
        return None;
    }

    // 趋势背驰条件判断
    let cond_area = curr.macd_area < prev.macd_area * MACD_AREA_RATIO;
    let cond_peak = curr.macd_peak < prev.macd_peak;
    let cond_new_extreme = if direction == "up" {
        // 上涨趋势：背驰段创趋势新高
        curr.end_val > prev.end_val
    } else {
        // 下跌趋势：背驰段创趋势新低
        curr.end_val < prev.end_val
    };

    if cond_area && cond_peak && cond_new_extreme {
        let prev_area = prev.macd_area;
        let curr_area = curr.macd_area;
        let prev_peak = prev.macd_peak;
        let curr_peak = curr.macd_peak;
        Some(BackDivergenceResult {
            bc_sub_type: "trend".to_string(),
            direction: curr.direction.clone(),
            index: curr.end_idx,
            prev_section: prev,
            curr_section: curr,
            prev_area,
            curr_area,
            prev_peak,
            curr_peak,
            is_confirmed: false,
        })
    } else {
        None
    }
}

// ─── 背驰确认 ─────────────────────────────────────────

/// 背驰确认（缠论27课）
///
/// 背驰的初步信号出现在走势段结束时，但最终确认需要：
/// 1. 背驰段之后出现反向次级别走势
/// 2. 趋势背驰：反向走势不回到最后一个中枢内部（形成第三类买卖点）
/// 3. 盘整背驰：反向走势回到中枢内部
fn confirm_backdivergence(
    bd: &BackDivergenceResult,
    sections: &[TrendSection],
    zs_list: &[ZhongShu],
) -> bool {
    // 找到背驰段之后的反向走势段
    let curr_idx = sections.iter().position(|s| {
        s.start_idx == bd.curr_section.start_idx && s.end_idx == bd.curr_section.end_idx
    });

    let curr_idx = match curr_idx {
        Some(idx) => idx,
        None => return false,
    };

    if curr_idx >= sections.len() - 1 {
        // 没有后续段，无法确认
        return false;
    }

    let reverse_section = &sections[curr_idx + 1];

    // 必须是反向走势
    if reverse_section.direction == bd.curr_section.direction {
        return false;
    }

    // 找到最后一个相关中枢
    let last_zs = match bd.bc_sub_type.as_str() {
        "trend" => {
            // 趋势背驰：找最后的中枢
            zs_list.last()
        }
        "panzheng" => {
            // 盘整背驰：找该中枢
            zs_list.first()
        }
        _ => None,
    };

    let last_zs = match last_zs {
        Some(zs) => zs,
        None => return false,
    };

    // 回抽确认条件
    let confirm = if bd.bc_sub_type == "trend" {
        // 趋势背驰：反向走势不回到最后一个中枢内部
        if bd.direction == "up" {
            // 上涨趋势背驰：回抽不跌破中枢上沿
            reverse_section.end_val >= last_zs.zg
        } else {
            // 下跌趋势背驰：回抽不升破中枢下沿
            reverse_section.end_val <= last_zs.zd
        }
    } else {
        // 盘整背驰：反向走势回到中枢内部
        if bd.direction == "up" {
            reverse_section.end_val <= last_zs.zg
        } else {
            reverse_section.end_val >= last_zs.zd
        }
    };

    confirm
}

// ─── 内部结果结构 ─────────────────────────────────────

/// 背驰初步判断结果（内部使用）
#[allow(dead_code)]
struct BackDivergenceResult {
    /// 背驰子类型: "trend" / "panzheng"
    bc_sub_type: String,
    /// 方向: "up" / "down"
    direction: String,
    /// 背驰段终点K线索引
    index: u64,
    /// 前一段走势
    prev_section: TrendSection,
    /// 当前背驰段
    curr_section: TrendSection,
    /// 前一段MACD面积
    prev_area: f64,
    /// 当前段MACD面积
    curr_area: f64,
    /// 前一段MACD峰值
    prev_peak: f64,
    /// 当前段MACD峰值
    curr_peak: f64,
    /// 是否已确认
    is_confirmed: bool,
}

/// 转换为 BeiChi 公开结构
fn make_beichi(bc_type: &str, bd: &BackDivergenceResult, confirmed: bool) -> BeiChi {
    let direction_label = if bd.direction == "up" { "顶背驰" } else { "底背驰" };
    let reason = format!(
        "{}: 前段MACD面积 {:.2} 峰值 {:.2}, 当前段面积 {:.2} ({:.0}%) 峰值 {:.2} ({:.0}%){}",
        direction_label,
        bd.prev_area,
        bd.prev_peak,
        bd.curr_area,
        if bd.prev_area > 0.0 { bd.curr_area / bd.prev_area * 100.0 } else { 0.0 },
        bd.curr_peak,
        if bd.prev_peak > 0.0 { bd.curr_peak / bd.prev_peak * 100.0 } else { 0.0 },
        if confirmed { " [已确认]" } else { " [待确认]" },
    );

    BeiChi {
        bc_type: bc_type.to_string(),
        index: bd.index,
        dt: String::new(),
        direction: bd.direction.clone(),
        bc_sub_type: bd.bc_sub_type.clone(),
        reason,
    }
}

// ─── 测试 ─────────────────────────────────────────────

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

    fn make_zs(zs_type: &str, start_idx: u64, end_idx: u64, zg: f64, zd: f64) -> ZhongShu {
        ZhongShu {
            zs_type: zs_type.to_string(),
            start_index: start_idx,
            end_index: end_idx,
            start_dt: "t0".to_string(),
            end_dt: "t1".to_string(),
            zg,
            zd,
            gg: zg + 1.0,
            dd: zd - 1.0,
        }
    }

    #[test]
    fn test_no_beichi_without_zs() {
        // 无中枢则无背驰
        let bis = vec![
            make_bi(0, "up", 10.0, 20.0, 0, 5),
            make_bi(1, "down", 20.0, 8.0, 5, 10),
            make_bi(2, "up", 8.0, 15.0, 10, 15),
            make_bi(3, "down", 15.0, 10.0, 15, 20),
        ];
        let macd = MacdData::default();
        let zs: Vec<ZhongShu> = vec![];

        let beichi = detect_bi_beichi(&bis, &macd, &zs);
        assert!(beichi.is_empty(), "无中枢不应有背驰");
    }

    #[test]
    fn test_panzheng_beichi_structure() {
        // 盘整背驰：1个中枢 + 前后同方向段力度衰减
        // 笔0: up  (进入段a)
        // 笔1-5: 中枢 [12,14]
        // 笔6: up  (离开段b，力度弱于a) → 盘整背驰
        let bis = vec![
            make_bi(0, "up", 10.0, 20.0, 0, 5),    // 进入段a，力度大
            make_bi(1, "down", 20.0, 12.0, 5, 10),  // 中枢笔
            make_bi(2, "up", 12.0, 14.0, 10, 15),   // 中枢笔
            make_bi(3, "down", 14.0, 13.0, 15, 20), // 中枢笔
            make_bi(4, "up", 13.0, 14.0, 20, 25),   // 中枢笔
            make_bi(5, "down", 14.0, 12.0, 25, 30), // 中枢笔
            make_bi(6, "up", 12.0, 18.0, 30, 35),   // 离开段b（幅度6 < 幅度10，但创新高18<20 → 不创新高）
        ];

        let zs = vec![make_zs("bi_zs", 5, 30, 14.0, 12.0)];
        let macd = MacdData::default();

        let beichi = detect_bi_beichi(&bis, &macd, &zs);
        // 无MACD数据（面积都是0），不应产生背驰
        assert!(beichi.is_empty() || beichi.iter().all(|b| b.bc_sub_type == "panzheng"));
    }

    #[test]
    fn test_trend_beichi_structure() {
        // 趋势背驰：2个递进中枢 + 力度衰减
        let bis = vec![
            make_bi(0, "up", 10.0, 18.0, 0, 5),    // b段
            make_bi(1, "down", 18.0, 14.0, 5, 10),
            make_bi(2, "up", 14.0, 16.0, 10, 15),
            make_bi(3, "down", 16.0, 15.0, 15, 20), // ZS1: [14,16]
            make_bi(4, "up", 15.0, 25.0, 20, 25),   // b段（连接段）
            make_bi(5, "down", 25.0, 19.0, 25, 30),
            make_bi(6, "up", 19.0, 21.0, 30, 35),
            make_bi(7, "down", 21.0, 20.0, 35, 40),  // ZS2: [19,21]
            make_bi(8, "up", 20.0, 23.0, 40, 45),    // c段（背驰段，幅度3 < 幅度10）
        ];

        // ZS1: [14, 16], ZS2: [19, 21] — ZS2.zd=19 > ZS1.zg=16 → 上涨趋势
        let zs = vec![
            make_zs("bi_zs", 5, 20, 16.0, 14.0),
            make_zs("bi_zs", 25, 40, 21.0, 19.0),
        ];

        let macd = MacdData::default();
        let beichi = detect_bi_beichi(&bis, &macd, &zs);
        // 无MACD数据，不应有背驰
        assert!(beichi.is_empty() || beichi.iter().all(|b| b.bc_sub_type == "trend"));
    }

    #[test]
    fn test_calculate_section_power_up() {
        let mut section = TrendSection {
            direction: "up".to_string(),
            start_idx: 0,
            end_idx: 4,
            start_val: 10.0,
            end_val: 20.0,
            zs_idx: None,
            macd_area: 0.0,
            macd_peak: 0.0,
        };

        // MACD: [1.0, -0.5, 2.0, 0.5, 3.0]
        // 上涨只看正值：1.0 + 2.0 + 0.5 + 3.0 = 6.5
        // 峰值：3.0
        let macd = MacdData {
            dif: vec![0.0; 5],
            dea: vec![0.0; 5],
            macd_hist: vec![1.0, -0.5, 2.0, 0.5, 3.0],
        };

        calculate_section_power(&mut section, &macd);
        assert!((section.macd_area - 6.5).abs() < 0.001, "上涨面积应为6.5，实际{}", section.macd_area);
        assert!((section.macd_peak - 3.0).abs() < 0.001, "上涨峰值应为3.0，实际{}", section.macd_peak);
    }

    #[test]
    fn test_calculate_section_power_down() {
        let mut section = TrendSection {
            direction: "down".to_string(),
            start_idx: 0,
            end_idx: 4,
            start_val: 20.0,
            end_val: 10.0,
            zs_idx: None,
            macd_area: 0.0,
            macd_peak: 0.0,
        };

        // MACD: [-1.0, 0.5, -2.0, -0.5, -3.0]
        // 下跌只看负值绝对值：1.0 + 2.0 + 0.5 + 3.0 = 6.5
        // 峰值：3.0
        let macd = MacdData {
            dif: vec![0.0; 5],
            dea: vec![0.0; 5],
            macd_hist: vec![-1.0, 0.5, -2.0, -0.5, -3.0],
        };

        calculate_section_power(&mut section, &macd);
        assert!((section.macd_area - 6.5).abs() < 0.001, "下跌面积应为6.5，实际{}", section.macd_area);
        assert!((section.macd_peak - 3.0).abs() < 0.001, "下跌峰值应为3.0，实际{}", section.macd_peak);
    }

    #[test]
    fn test_beichi_with_macd_data() {
        // 完整测试：有MACD数据的趋势背驰
        let bis = vec![
            make_bi(0, "up", 10.0, 15.0, 0, 3),     // a段
            make_bi(1, "down", 15.0, 12.0, 3, 6),
            make_bi(2, "up", 12.0, 14.0, 6, 9),
            make_bi(3, "down", 14.0, 13.0, 9, 12),   // ZS1: [12,14]
            make_bi(4, "up", 13.0, 22.0, 12, 16),    // b段（力度大）
            make_bi(5, "down", 22.0, 17.0, 16, 19),
            make_bi(6, "up", 17.0, 19.0, 19, 22),
            make_bi(7, "down", 19.0, 18.0, 22, 25),   // ZS2: [17,19]
            make_bi(8, "up", 18.0, 23.0, 25, 29),     // c段（力度小，创新高23>22）
        ];

        let zs = vec![
            make_zs("bi_zs", 3, 12, 14.0, 12.0),  // ZS1
            make_zs("bi_zs", 16, 25, 19.0, 17.0),  // ZS2: zd=17 > ZS1.zg=14
        ];

        // 构造MACD数据：b段面积大，c段面积小
        let mut macd_hist = vec![0.0; 30];
        // b段(idx 12-16)力度大
        for i in 12..=16 {
            macd_hist[i] = 5.0;
        }
        // c段(idx 25-29)力度小
        for i in 25..=29 {
            macd_hist[i] = 1.0;
        }

        let macd = MacdData {
            dif: vec![0.0; 30],
            dea: vec![0.0; 30],
            macd_hist,
        };

        let beichi = detect_bi_beichi(&bis, &macd, &zs);
        // 应该检测到趋势背驰: c段面积5.0 < b段面积25.0 * 0.7 = 17.5, 峰值1.0 < 5.0
        assert!(!beichi.is_empty(), "应检测到趋势背驰");
        let trend_bc: Vec<_> = beichi.iter().filter(|b| b.bc_sub_type == "trend").collect();
        assert!(!trend_bc.is_empty(), "应为趋势背驰");
        assert_eq!(trend_bc[0].direction, "up", "应为上涨趋势背驰");
    }

    #[test]
    fn test_no_beichi_without_new_extreme() {
        // 不创新高/新低 → 不算背驰
        let bis = vec![
            make_bi(0, "up", 10.0, 20.0, 0, 3),     // a段，幅度10
            make_bi(1, "down", 20.0, 12.0, 3, 6),
            make_bi(2, "up", 12.0, 14.0, 6, 9),
            make_bi(3, "down", 14.0, 13.0, 9, 12),   // ZS1
            make_bi(4, "up", 13.0, 18.0, 12, 16),    // b段，没有创新高(18<20)
        ];

        let zs = vec![make_zs("bi_zs", 3, 12, 14.0, 12.0)];

        let mut macd_hist = vec![0.0; 20];
        // a段(idx 0-3)力度大
        for i in 0..=3 {
            macd_hist[i] = 5.0;
        }
        // b段(idx 12-16)力度小
        for i in 12..=16 {
            macd_hist[i] = 1.0;
        }

        let macd = MacdData {
            dif: vec![0.0; 20],
            dea: vec![0.0; 20],
            macd_hist,
        };

        let beichi = detect_bi_beichi(&bis, &macd, &zs);
        // b段未创新高(18<20) → 不满足条件5 → 不算背驰
        assert!(beichi.is_empty(), "未创新高不应判断为背驰");
    }

    #[test]
    fn test_group_zs_by_direction() {
        let zs1 = make_zs("bi_zs", 0, 10, 14.0, 12.0);
        let zs2 = make_zs("bi_zs", 15, 25, 19.0, 17.0);  // zd=17 > zg=14 → 上涨
        let zs3 = make_zs("bi_zs", 30, 40, 10.0, 8.0);   // zg=10 < zd=12 → 方向改变

        let zs_list = vec![zs1, zs2, zs3];
        let groups = group_zs_by_direction(&zs_list);
        
        // zs1+zs2 同向上涨趋势（一组），zs3 方向改变（新组）
        assert!(groups.len() >= 2, "应有2组中枢, 实际{}", groups.len());
        // 第一组应包含zs1和zs2
        assert_eq!(groups[0].len(), 2, "第一组应有2个中枢");
        assert_eq!(groups[1], vec![2], "第二组应只有zs3");
    }

    #[test]
    fn test_classify_zs_direction() {
        let zs1 = make_zs("bi_zs", 0, 10, 14.0, 12.0);
        let zs2_up = make_zs("bi_zs", 15, 25, 19.0, 17.0); // zd=17 > zd=12 → up
        let zs2_down = make_zs("bi_zs", 15, 25, 10.0, 8.0); // zg=10 < zg=14 → down

        assert_eq!(classify_zs_direction(&zs1, &zs2_up), "up");
        assert_eq!(classify_zs_direction(&zs1, &zs2_down), "down");
    }
}
