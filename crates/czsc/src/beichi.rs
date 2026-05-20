//! 背驰检测 — 严格遵循缠论24/27/37/53课原始定义
//!
//! # 缠论背驰的精确定义
//!
//! ## 1. 趋势背驰（24课/37课）
//!
//! 缠论37课原文：
//! "没有趋势，没有背驰。当说a+A+b+B+c中有背驰时，首先要a+A+b+B+c是一个趋势。"
//! "如果a+A+b+B+c是上涨，c一定要创出新高；a+A+b+B+c是下跌，c一定要创出新低。
//!  否则，就算c包含B的第三类买卖点，也可以对围绕B的次级别震荡用盘整背驰的方式
//!  进行判断。"
//!
//! 趋势背驰的必要条件（缺一不可）：
//! ① 走势类型为趋势（≥2个同方向、同级别、无重叠的中枢 A、B）
//! ② c段是最后一个中枢B之后的离开段，与走势同方向
//! ③ c段必须创新高（上涨趋势）或创新低（下跌趋势）
//! ④ c段的MACD力度（面积+峰值）< b段
//!
//! ## 2. 盘整背驰（27课/53课）
//!
//! 缠论53课原文：
//! "盘整背驰与背驰是两回事，不能混为一谈。盘整背驰只是利用背驰相应的力度分析
//!  方法进行的一个推广应用。a+B+c中，a和c的盘整背驰，其实都可以看成是B的中枢
//!  震荡。"
//!
//! 盘整背驰：只有一个中枢的盘整走势(a+A+b)中，b段力度 < a段力度。
//!
//! 盘整背驰的三种结局（24课/27课）：
//! - 情况一：b段不破中枢 → 必定回跌
//! - 情况二：b段破中枢、回抽不回中枢 → 第三类买卖点（盘整转化为趋势）
//! - 情况三：b段破中枢、回抽跌回中枢 → 继续盘整
//!
//! ## 3. 关键约束
//!
//! - "没有趋势，没有背驰"（37课）：趋势背驰必须出现在趋势中
//! - "盘整背驰与背驰是两回事"（53课）：盘整背驰≠趋势背驰，不能混谈
//! - 盘整背驰不产生一类买卖点（27课）："第一类买点，多数由趋势的背驰构成"
//! - 上涨趋势中不存在"底背驰"，下跌趋势中不存在"顶背驰"
//!   以a+A+b+B+c上涨为例：a、b、c都是向上的段，只看力度比较，
//!   c段力度<b段就是顶背驰。回抽段（a→A之间的下跌，b→B之间的下跌）
//!   是中枢震荡的一部分，不是独立走势，不存在"底背驰"。
//!
//! ## 4. 力度衡量（24课/27课）
//!
//! - 上涨走势段：只累加MACD红柱（正值）面积和峰值
//! - 下跌走势段：只累加MACD绿柱（负值）绝对值面积和峰值
//! - 面积是核心指标，峰值是辅助，面积衰减即可（峰值仅辅助参考）

use yifang_data::{Bi, BeiChi, MacdData, XianDuan, ZhongShu};

// ─── 走势段（用于力度比较的最小单位）──────────────────────

#[derive(Debug, Clone)]
struct TrendSection {
    /// 方向: "up" / "down"
    direction: String,
    /// 起点 K 线索引
    start_idx: u64,
    /// 终点 K 线索引
    end_idx: u64,
    /// 起点价格
    start_val: f64,
    /// 终点价格
    end_val: f64,
    /// MACD 面积（力度核心指标）
    macd_area: f64,
    /// MACD 峰值（力度辅助指标）
    macd_peak: f64,
}

impl TrendSection {
    /// 段的最高价格
    fn high(&self) -> f64 {
        self.start_val.max(self.end_val)
    }
    /// 段的最低价格
    fn low(&self) -> f64 {
        self.start_val.min(self.end_val)
    }
}

// ─── 公开接口 ──────────────────────────────────────────

/// 识别笔级别背驰
pub fn detect_bi_beichi(bis: &[Bi], macd: &MacdData, zs_list: &[ZhongShu]) -> Vec<BeiChi> {
    detect_beichi_inner(
        bis,
        macd,
        zs_list,
        "bi_beichi",
        |bi: &Bi| (bi.start_index, bi.end_index, bi.direction.clone(), bi.start_price, bi.end_price),
    )
}

/// 识别线段级别背驰
pub fn detect_xd_beichi(xds: &[XianDuan], macd: &MacdData, zs_list: &[ZhongShu]) -> Vec<BeiChi> {
    detect_beichi_inner(
        xds,
        macd,
        zs_list,
        "xd_beichi",
        |xd: &XianDuan| (xd.start_index, xd.end_index, xd.direction.clone(), xd.start_price, xd.end_price),
    )
}

// ─── 核心实现 ──────────────────────────────────────────

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

    // 将段列表转为 TrendSection
    let sections = build_trend_sections(segments, &extract);

    // 计算所有段的MACD力度（一次性计算，避免重复）
    let mut sections = sections;
    for sec in &mut sections {
        calculate_section_power(sec, macd);
    }

    // 按中枢递进方向分组
    let groups = group_zs_by_trend(zs_list);

    for group_indices in &groups {
        let group_zs: Vec<&ZhongShu> = group_indices.iter().map(|&i| &zs_list[i]).collect();

        if group_indices.len() >= 2 {
            // 多中枢递进 → 趋势 → 检测趋势背驰
            if let Some(bd) = check_trend_backdivergence(&group_zs, &sections, bc_type) {
                results.push(bd);
            }
        } else {
            // 单中枢 → 盘整 → 检测盘整背驰
            let zs_idx = group_indices[0];
            if let Some(bd) = check_panzheng_backdivergence(&zs_list[zs_idx], &sections, bc_type) {
                results.push(bd);
            }
        }
    }

    results
}

// ─── 走势段构建 ───────────────────────────────────────

fn build_trend_sections<T, F>(
    segments: &[T],
    extract: &F,
) -> Vec<TrendSection>
where
    F: Fn(&T) -> (u64, u64, String, f64, f64),
{
    segments
        .iter()
        .map(|seg| {
            let (start_idx, end_idx, direction, start_val, end_val) = extract(seg);
            TrendSection {
                direction,
                start_idx,
                end_idx,
                start_val,
                end_val,
                macd_area: 0.0,
                macd_peak: 0.0,
            }
        })
        .collect()
}

// ─── 中枢分组 ─────────────────────────────────────────

/// 按中枢递进方向分组：
/// 上涨趋势组：后续中枢 zd > 前中枢 zg（无重叠向上递进）
/// 下跌趋势组：后续中枢 zg < 前中枢 zd（无重叠向下递进）
/// 方向改变或有重叠则开新组
fn group_zs_by_trend(zs_list: &[ZhongShu]) -> Vec<Vec<usize>> {
    if zs_list.is_empty() {
        return Vec::new();
    }

    let mut groups: Vec<Vec<usize>> = vec![vec![0]];

    for i in 1..zs_list.len() {
        let prev = &zs_list[i - 1];
        let curr = &zs_list[i];

        // 无重叠且同方向
        let no_overlap_up = curr.zd > prev.zg;   // 上涨递进
        let no_overlap_down = curr.zg < prev.zd;  // 下跌递进
        let no_overlap = no_overlap_up || no_overlap_down;

        // 当前方向
        let direction = classify_zs_direction(prev, curr);

        // 获取当前组的主方向
        let group_dir = {
            let group = &groups.last().unwrap();
            if group.len() >= 2 {
                let last = group.last().unwrap();
                let second_last = group[group.len() - 2];
                classify_zs_direction(&zs_list[second_last], &zs_list[*last])
            } else if group.len() == 1 {
                // 单中枢无法确定方向，用当前方向
                direction.clone()
            } else {
                direction.clone()
            }
        };

        if no_overlap && direction == group_dir {
            groups.last_mut().unwrap().push(i);
        } else {
            groups.push(vec![i]);
        }
    }

    groups
}

/// 根据两个中枢位置关系判断方向
fn classify_zs_direction(prev: &ZhongShu, curr: &ZhongShu) -> String {
    if curr.zd > prev.zg {
        "up".to_string()
    } else if curr.zg < prev.zd {
        "down".to_string()
    } else if curr.zd > prev.zd {
        "up".to_string()  // 有重叠但整体上移
    } else {
        "down".to_string()
    }
}

// ─── MACD 力度计算 ─────────────────────────────────────

/// 计算单个走势段的MACD力度
///
/// 缠论标准（24课/27课）：
/// - 上涨走势段只累加正MACD值（红柱）面积和峰值
/// - 下跌走势段只累加负MACD值绝对值（绿柱）面积和峰值
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

// ─── 趋势背驰判断 ─────────────────────────────────────

/// 趋势背驰（缠论37课）
///
/// 必须 a+A+b+B+c 是趋势，且 c 段创新高/新低。
///
/// 37课原文：
/// "如果a+A+b+B+c是上涨，c一定要创出新高；a+A+b+B+c是下跌，c一定要创出新低。
///  否则，就算c包含B的第三类买卖点，也可以对围绕B的次级别震荡用盘整背驰的方式
///  进行判断。"
fn check_trend_backdivergence(
    zs_group: &[&ZhongShu],
    sections: &[TrendSection],
    bc_type: &str,
) -> Option<BeiChi> {
    // 趋势必须至少2个中枢
    if zs_group.len() < 2 {
        return None;
    }

    // 确认是同方向无重叠趋势
    let trend_direction = classify_zs_direction(zs_group[0], zs_group[1]);

    for w in zs_group.windows(2) {
        // 检查所有相邻中枢对都满足无重叠递进
        if trend_direction == "up" && w[1].zd <= w[0].zg {
            return None; // 上涨趋势后中枢低点必须 > 前中枢高点
        }
        if trend_direction == "down" && w[1].zg >= w[0].zd {
            return None; // 下跌趋势后中枢高点必须 < 前中枢低点
        }
    }

    let last_zs = zs_group.last().unwrap();
    let second_last_zs = zs_group[zs_group.len() - 2];

    // 找b段：最后两个中枢之间的连接段，方向与趋势一致
    //
    // 缠论37课：在 a+A1+b1+A2+b2+...+An+c 中，力度比较的是
    // 最后两个中枢之间的连接段 b(last-1) 与 c 段。
    //
    // 趋势背驰的物理含义是"最后的推动力度衰减"——看的是最后一段
    // 推动（b_last）和最新一段推动（c）哪个力度更弱。
    // 取max力度反而会选到最强的那段，违背了背驰的本意。
    let mut b_section: Option<&TrendSection> = None;

    // 优先：找起点在 second_last_zs 之后、start_index 在 last_zs 之前的连接段
    for sec in sections.iter() {
        if sec.direction == trend_direction
            && sec.start_idx >= second_last_zs.end_index
            && sec.start_idx < last_zs.start_index
        {
            if sec.macd_area > 0.0 {
                b_section = Some(sec);
                break;
            }
        }
    }
    // 放宽：段起点在 second_last_zs 结束之后、终点在 last_zs 结束之前
    if b_section.is_none() {
        for sec in sections.iter() {
            if sec.direction == trend_direction
                && sec.start_idx >= second_last_zs.end_index
                && sec.start_idx < last_zs.end_index
            {
                if sec.macd_area > 0.0 {
                    b_section = Some(sec);
                    break;
                }
            }
        }
    }

    let b_section = b_section?;

    // 找c段：最后一个中枢之后的离开段，方向与趋势一致
    let c_section = sections.iter().find(|sec| {
        sec.start_idx >= last_zs.end_index && sec.direction == trend_direction
    })?;

    // 37课必要条件：c段必须创新高/新低
    //
    // "如果a+A+b+B+c是上涨，c一定要创出新高；a+A+b+B+c是下跌，c一定要创出新低"
    //
    // 创新高/新低是指：c段的价格超过了前面所有同方向段的极值
    // 对于上涨趋势：c段最高价 > b段最高价（b段在趋势中创新了高，c段必须超过b段的新高）
    // 对于下跌趋势：c段最低价 < b段最低价
    let cond_new_extreme = if trend_direction == "up" {
        c_section.high() > b_section.high()
    } else {
        c_section.low() < b_section.low()
    };

    if !cond_new_extreme {
        // c段没创新高/新低 → 不构成趋势背驰
        // 但仍可能构成盘整背驰（37课："可以用盘整背驰的方式处理"）
        // 这在 check_panzheng_backdivergence 中单独检测
        return None;
    }

    // 37课必要条件：c段必须包含对最后一个中枢B的第三类买卖点
    //
    // "c必然是次级别的，也就是说，c至少包含对B的一个第三类买卖点，
    //  否则，就可以看成是B中枢的小级别波动，完全可以用盘整背驰来处理。"
    //
    // 第三类买卖点判定：
    // - 上涨趋势：c段内部的回抽低点 > B.zd（即c段的最低点未回到中枢下边界）
    //   这意味着c段确实"离开了"B中枢，且有一个回抽确认的过程
    // - 下跌趋势：c段内部的反弹高点 < B.zg（即c段的最高点未回到中枢上边界）
    //
    // 对于单个段（Bi/XianDuan），段本身就包含了次级别的走势结构（笔包含K线、
    // 线段包含笔），所以c段"最低点不回B"等价于段内回抽确认了第三类买卖点。
    let cond_third_bs = if trend_direction == "up" {
        c_section.low() > last_zs.zd
    } else {
        c_section.high() < last_zs.zg
    };

    if !cond_third_bs {
        // c段未包含第三类买卖点 → 不构成趋势背驰
        // 可用盘整背驰方式处理
        return None;
    }

    // c段力度 < b段力度
    // 缠论27课：面积是核心指标
    let cond_area = c_section.macd_area < b_section.macd_area;

    if !cond_area {
        return None;
    }

    // 生成背驰结果
    let direction_label = if trend_direction == "up" { "顶背驰" } else { "底背驰" };
    let reason = format!(
        "趋势{}: b段面积 {:.2} 峰值 {:.2}, c段面积 {:.2} ({:.0}%) 峰值 {:.2} ({:.0}%)",
        direction_label,
        b_section.macd_area,
        b_section.macd_peak,
        c_section.macd_area,
        if b_section.macd_area > 0.0 { c_section.macd_area / b_section.macd_area * 100.0 } else { 0.0 },
        c_section.macd_peak,
        if b_section.macd_peak > 0.0 { c_section.macd_peak / b_section.macd_peak * 100.0 } else { 0.0 },
    );

    Some(BeiChi {
        bc_type: bc_type.to_string(),  // 级别：bi_beichi / xd_beichi
        index: c_section.end_idx,
        dt: String::new(),
        direction: trend_direction,
        bc_sub_type: "trend".to_string(),  // 类型：趋势背驰
        reason,
    })
}

// ─── 盘整背驰判断 ─────────────────────────────────────

/// 盘整背驰（缠论27课/53课）
///
/// 缠论53课：
/// "盘整背驰与背驰是两回事，不能混为一谈。盘整背驰只是利用背驰相应的力度分析
///  方法进行的一个推广应用。a+B+c中，a和c的盘整背驰，其实都可以看成是B的中枢震荡。"
///
/// 盘整结构：a + A + b
/// - a = 中枢前的进入段
/// - A = 中枢
/// - b = 中枢后的离开段
/// - 背驰条件：b段力度 < a段力度，且a、b同方向
///
/// 盘整背驰只标记与走势方向一致的段：
/// - 如果中枢之前的走势方向是上涨，a和b都是向上的段，盘整背驰 = 顶背驰
/// - 如果中枢之前的走势方向是下跌，a和b都是向下的段，盘整背驰 = 底背驰
fn check_panzheng_backdivergence(
    zs: &ZhongShu,
    sections: &[TrendSection],
    bc_type: &str,
) -> Option<BeiChi> {
    // 找中枢前的进入段 a 和中枢后的离开段 b
    //
    // 要求 a、b 同方向，且 b 段力度 < a 段力度

    let mut prev_section: Option<&TrendSection> = None;  // 中枢前的进入段
    let mut curr_section: Option<&TrendSection> = None;   // 中枢后的离开段

    for sec in sections.iter() {
        // 段结束于中枢开始之前 → 进入段（取最后一个）
        if sec.end_idx <= zs.start_index {
            prev_section = Some(sec);
        }
        // 段开始于中枢结束之后 → 离开段（取第一个）
        if sec.start_idx >= zs.end_index && curr_section.is_none() {
            curr_section = Some(sec);
        }
    }

    let a_sec = prev_section?;
    let b_sec = curr_section?;

    // a和b必须同方向
    if a_sec.direction != b_sec.direction {
        return None;
    }

    // a段力度为0，无法比较
    if a_sec.macd_area <= 0.0 || a_sec.macd_peak <= 0.0 {
        return None;
    }

    // 盘整背驰条件：b段力度 < a段力度
    let cond_area = b_sec.macd_area < a_sec.macd_area;

    if !cond_area {
        return None;
    }

    // 盘整背驰24课区分两种情况：
    // 1) b段破了a段极值位置 → 破位盘整背驰，可能形成第三类买卖点
    // 2) b段未破a段极值位置 → 未破位盘整背驰，更可能回到中枢
    //
    // 破位判定：b段价格极值超过a段价格极值
    // - 向上：b段最高价 > a段最高价
    // - 向下：b段最低价 < a段最低价
    let broken = if b_sec.direction == "up" {
        b_sec.high() > a_sec.high()
    } else {
        b_sec.low() < a_sec.low()
    };

    // 盘整背驰的方向定义：
    // a、b是向上的段 → 围绕中枢的向上震荡力度衰减 → 顶背驰
    // a、b是向下的段 → 围绕中枢的向下震荡力度衰减 → 底背驰
    //
    // 注意：盘整背驰只是中枢震荡的力度比较，不是走势转折的标志
    // 所以"方向"是指震荡的方向，而不是走势级别的方向
    let direction_label = if b_sec.direction == "up" { "顶背驰" } else { "底背驰" };
    let broken_label = if broken { "破位" } else { "未破位" };
    let reason = format!(
        "盘整{}({}): a段面积 {:.2} 峰值 {:.2}, b段面积 {:.2} ({:.0}%) 峰值 {:.2} ({:.0}%)",
        direction_label,
        broken_label,
        a_sec.macd_area,
        a_sec.macd_peak,
        b_sec.macd_area,
        if a_sec.macd_area > 0.0 { b_sec.macd_area / a_sec.macd_area * 100.0 } else { 0.0 },
        b_sec.macd_peak,
        if a_sec.macd_peak > 0.0 { b_sec.macd_peak / a_sec.macd_peak * 100.0 } else { 0.0 },
    );

    Some(BeiChi {
        bc_type: bc_type.to_string(),  // 级别：bi_beichi / xd_beichi
        index: b_sec.end_idx,
        dt: String::new(),
        direction: b_sec.direction.clone(),
        bc_sub_type: "panzheng".to_string(),
        reason,
    })
}

// ─── 测试 ─────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_bi(id: usize, dir: &str, start: f64, end: f64, start_idx: u64, end_idx: u64) -> Bi {
        Bi {
            direction: dir.to_string(),
            start_price: start,
            end_price: end,
            start_index: start_idx,
            end_index: end_idx,
            start_dt: format!("2024-01-{:02}", id + 1),
            end_dt: format!("2024-01-{:02}", id + 2),
            is_finished: true,
        }
    }

    fn make_zs(zs_type: &str, start_idx: u64, end_idx: u64, zg: f64, zd: f64) -> ZhongShu {
        ZhongShu {
            zs_type: zs_type.to_string(),
            start_index: start_idx,
            end_index: end_idx,
            start_dt: String::new(),
            end_dt: String::new(),
            zg,
            zd,
            gg: zg + 1.0,
            dd: (zd - 1.0).max(0.0),
        }
    }

    #[test]
    fn test_no_beichi_without_zs() {
        let bis = vec![
            make_bi(0, "up", 10.0, 15.0, 0, 3),
            make_bi(1, "down", 15.0, 12.0, 3, 6),
            make_bi(2, "up", 12.0, 14.0, 6, 9),
            make_bi(3, "down", 14.0, 13.0, 9, 12),
        ];
        let macd = MacdData { dif: vec![0.0; 13], dea: vec![0.0; 13], macd_hist: vec![0.0; 13] };
        let beichi = detect_bi_beichi(&bis, &macd, &[]);
        assert!(beichi.is_empty(), "无中枢不应有背驰");
    }

    #[test]
    fn test_panzheng_beichi_up() {
        // 盘整背驰（向上）：单中枢，a、b都是向上段，b力度<a
        let bis = vec![
            make_bi(0, "up", 10.0, 15.0, 0, 3),     // a段（进入，力度大）
            make_bi(1, "down", 15.0, 12.0, 3, 6),
            make_bi(2, "up", 12.0, 14.0, 6, 9),
            make_bi(3, "down", 14.0, 13.0, 9, 12),   // 中枢 [12,14]
            make_bi(4, "up", 13.0, 16.0, 12, 15),     // b段（离开，力度小）
        ];
        let zs = vec![make_zs("bi_zs", 3, 12, 14.0, 12.0)];

        let mut macd_hist = vec![0.0; 16];
        for i in 0..=3 { macd_hist[i] = 5.0; }  // a段面积大
        for i in 12..=15 { macd_hist[i] = 1.0; } // b段面积小
        let macd = MacdData { dif: vec![0.0; 16], dea: vec![0.0; 16], macd_hist };

        let beichi = detect_bi_beichi(&bis, &macd, &zs);
        let pz: Vec<_> = beichi.iter().filter(|b| b.bc_sub_type == "panzheng").collect();
        assert!(!pz.is_empty(), "应检测到盘整背驰");
        assert_eq!(pz[0].direction, "up", "a、b同向上 → 盘整顶背驰");
    }

    #[test]
    fn test_trend_beichi_up_with_new_high() {
        // 趋势背驰（上涨）：2个递进中枢，c段创新高但力度<b
        let bis = vec![
            make_bi(0, "up", 10.0, 15.0, 0, 3),
            make_bi(1, "down", 15.0, 12.0, 3, 6),
            make_bi(2, "up", 12.0, 14.0, 6, 9),
            make_bi(3, "down", 14.0, 13.0, 9, 12),    // ZS1: [12,14]
            make_bi(4, "up", 13.0, 22.0, 12, 16),      // b段（创新高22）
            make_bi(5, "down", 22.0, 17.0, 16, 19),
            make_bi(6, "up", 17.0, 19.0, 19, 22),
            make_bi(7, "down", 19.0, 18.0, 22, 25),    // ZS2: [17,19]
            make_bi(8, "up", 18.0, 25.0, 25, 29),      // c段（创新高25>22, 但力度小）
        ];
        let zs = vec![
            make_zs("bi_zs", 3, 12, 14.0, 12.0),   // ZS1
            make_zs("bi_zs", 16, 25, 19.0, 17.0),  // ZS2: zd=17 > ZS1.zg=14
        ];

        let mut macd_hist = vec![0.0; 30];
        for i in 12..=16 { macd_hist[i] = 5.0; }  // b段力度大
        for i in 25..=29 { macd_hist[i] = 1.0; }  // c段力度小
        let macd = MacdData { dif: vec![0.0; 30], dea: vec![0.0; 30], macd_hist };

        let beichi = detect_bi_beichi(&bis, &macd, &zs);
        let trend_bc: Vec<_> = beichi.iter().filter(|b| b.bc_sub_type == "trend").collect();
        assert!(!trend_bc.is_empty(), "应检测到趋势背驰");
        assert_eq!(trend_bc[0].direction, "up", "应为上涨趋势顶背驰");
    }

    #[test]
    fn test_trend_beichi_down_with_new_low() {
        // 趋势背驰（下跌）：2个递进向下中枢，c段创新低但力度<b
        let bis = vec![
            make_bi(0, "down", 25.0, 20.0, 0, 3),
            make_bi(1, "up", 20.0, 22.0, 3, 6),
            make_bi(2, "down", 22.0, 21.0, 6, 9),
            make_bi(3, "up", 21.0, 22.0, 9, 12),    // ZS1: [20,22]
            make_bi(4, "down", 22.0, 12.0, 12, 16),  // b段
            make_bi(5, "up", 12.0, 16.0, 16, 19),
            make_bi(6, "down", 16.0, 14.0, 19, 22),
            make_bi(7, "up", 14.0, 15.0, 22, 25),    // ZS2: [14,15]
            make_bi(8, "down", 14.5, 10.0, 25, 29),  // c段（创新低10<12, c.high=14.5<ZS2.zg=15, 力度小）
        ];
        let zs = vec![
            make_zs("bi_zs", 3, 12, 22.0, 20.0),   // ZS1
            make_zs("bi_zs", 16, 25, 15.0, 14.0),  // ZS2: zg=15 < ZS1.zd=20
        ];

        let mut macd_hist = vec![0.0; 30];
        for i in 12..=16 { macd_hist[i] = -5.0; }  // b段力度大（负值）
        for i in 25..=29 { macd_hist[i] = -1.0; }  // c段力度小
        let macd = MacdData { dif: vec![0.0; 30], dea: vec![0.0; 30], macd_hist };

        let beichi = detect_bi_beichi(&bis, &macd, &zs);
        let trend_bc: Vec<_> = beichi.iter().filter(|b| b.bc_sub_type == "trend").collect();
        assert!(!trend_bc.is_empty(), "应检测到趋势背驰");
        assert_eq!(trend_bc[0].direction, "down", "应为下跌趋势底背驰");
    }

    #[test]
    fn test_no_trend_beichi_without_new_extreme() {
        // 37课关键约束：c段不创新高/新低 → 不构成趋势背驰
        // "如果a+A+b+B+c是上涨，c一定要创出新高"
        let bis = vec![
            make_bi(0, "up", 10.0, 15.0, 0, 3),
            make_bi(1, "down", 15.0, 12.0, 3, 6),
            make_bi(2, "up", 12.0, 14.0, 6, 9),
            make_bi(3, "down", 14.0, 13.0, 9, 12),    // ZS1: [12,14]
            make_bi(4, "up", 13.0, 22.0, 12, 16),      // b段（高点22）
            make_bi(5, "down", 22.0, 17.0, 16, 19),
            make_bi(6, "up", 17.0, 19.0, 19, 22),
            make_bi(7, "down", 19.0, 18.0, 22, 25),    // ZS2: [17,19]
            make_bi(8, "up", 18.0, 21.0, 25, 29),      // c段（高点21 < b段22，未创新高）
        ];
        let zs = vec![
            make_zs("bi_zs", 3, 12, 14.0, 12.0),
            make_zs("bi_zs", 16, 25, 19.0, 17.0),
        ];

        let mut macd_hist = vec![0.0; 30];
        for i in 12..=16 { macd_hist[i] = 5.0; }  // b段力度大
        for i in 25..=29 { macd_hist[i] = 1.0; }  // c段力度小
        let macd = MacdData { dif: vec![0.0; 30], dea: vec![0.0; 30], macd_hist };

        let beichi = detect_bi_beichi(&bis, &macd, &zs);
        let trend_bc: Vec<_> = beichi.iter().filter(|b| b.bc_sub_type == "trend").collect();
        assert!(trend_bc.is_empty(), "c段不创新高不应构成趋势背驰(37课)");
    }

    #[test]
    fn test_no_beichi_when_power_not_decreasing() {
        // c段力度 >= b段 → 不背驰
        let bis = vec![
            make_bi(0, "up", 10.0, 15.0, 0, 3),
            make_bi(1, "down", 15.0, 12.0, 3, 6),
            make_bi(2, "up", 12.0, 14.0, 6, 9),
            make_bi(3, "down", 14.0, 13.0, 9, 12),
            make_bi(4, "up", 13.0, 22.0, 12, 16),
            make_bi(5, "down", 22.0, 17.0, 16, 19),
            make_bi(6, "up", 17.0, 19.0, 19, 22),
            make_bi(7, "down", 19.0, 18.0, 22, 25),
            make_bi(8, "up", 18.0, 26.0, 25, 29),     // c段力度大
        ];
        let zs = vec![
            make_zs("bi_zs", 3, 12, 14.0, 12.0),
            make_zs("bi_zs", 16, 25, 19.0, 17.0),
        ];

        let mut macd_hist = vec![0.0; 30];
        for i in 12..=16 { macd_hist[i] = 2.0; }  // b段力度小
        for i in 25..=29 { macd_hist[i] = 5.0; }  // c段力度大
        let macd = MacdData { dif: vec![0.0; 30], dea: vec![0.0; 30], macd_hist };

        let beichi = detect_bi_beichi(&bis, &macd, &zs);
        let trend_bc: Vec<_> = beichi.iter().filter(|b| b.bc_sub_type == "trend").collect();
        assert!(trend_bc.is_empty(), "力度增大不应背驰");
    }

    #[test]
    fn test_panzheng_beichi_down() {
        // 盘整背驰（向下）：单中枢，a、b都是向下段，b力度<a
        let bis = vec![
            make_bi(0, "down", 20.0, 12.0, 0, 3),     // a段（进入，力度大）
            make_bi(1, "up", 12.0, 15.0, 3, 6),
            make_bi(2, "down", 15.0, 13.0, 6, 9),
            make_bi(3, "up", 13.0, 14.0, 9, 12),       // 中枢 [12,14]
            make_bi(4, "down", 14.0, 10.0, 12, 15),    // b段（离开，力度小）
        ];
        let zs = vec![make_zs("bi_zs", 3, 12, 14.0, 12.0)];

        let mut macd_hist = vec![0.0; 16];
        for i in 0..=3 { macd_hist[i] = -5.0; }  // a段力度大（负值）
        for i in 12..=15 { macd_hist[i] = -1.0; } // b段力度小
        let macd = MacdData { dif: vec![0.0; 16], dea: vec![0.0; 16], macd_hist };

        let beichi = detect_bi_beichi(&bis, &macd, &zs);
        let pz: Vec<_> = beichi.iter().filter(|b| b.bc_sub_type == "panzheng").collect();
        assert!(!pz.is_empty(), "应检测到盘整背驰");
        assert_eq!(pz[0].direction, "down", "a、b同向下 → 盘整底背驰");
    }

    #[test]
    fn test_calculate_section_power_up() {
        let mut section = TrendSection {
            direction: "up".to_string(),
            start_idx: 0,
            end_idx: 4,
            start_val: 10.0,
            end_val: 15.0,
            macd_area: 0.0,
            macd_peak: 0.0,
        };
        let macd = MacdData {
            dif: vec![0.0; 5],
            dea: vec![0.0; 5],
            macd_hist: vec![1.0, 2.0, 3.0, -1.0, 2.0],
        };
        calculate_section_power(&mut section, &macd);
        // 上涨只看正值: 1+2+3+2 = 8
        assert_eq!(section.macd_area, 8.0);
        assert_eq!(section.macd_peak, 3.0);
    }

    #[test]
    fn test_calculate_section_power_down() {
        let mut section = TrendSection {
            direction: "down".to_string(),
            start_idx: 0,
            end_idx: 4,
            start_val: 15.0,
            end_val: 10.0,
            macd_area: 0.0,
            macd_peak: 0.0,
        };
        let macd = MacdData {
            dif: vec![0.0; 5],
            dea: vec![0.0; 5],
            macd_hist: vec![-1.0, -2.0, -3.0, 1.0, -2.0],
        };
        calculate_section_power(&mut section, &macd);
        // 下跌只看负值绝对值: 1+2+3+2 = 8
        assert_eq!(section.macd_area, 8.0);
        assert_eq!(section.macd_peak, 3.0);
    }

    #[test]
    fn test_group_zs_by_trend_up() {
        let zs = vec![
            make_zs("bi_zs", 3, 12, 14.0, 12.0),   // ZS1
            make_zs("bi_zs", 15, 24, 19.0, 17.0),   // ZS2: zd=17 > zg=14 → 上涨递进
            make_zs("bi_zs", 27, 36, 24.0, 22.0),   // ZS3: zd=22 > zg=19 → 继续上涨递进
        ];
        let groups = group_zs_by_trend(&zs);
        assert_eq!(groups.len(), 1, "3个上涨递进中枢应为一组");
        assert_eq!(groups[0].len(), 3);
    }

    #[test]
    fn test_group_zs_by_trend_mixed() {
        let zs = vec![
            make_zs("bi_zs", 3, 12, 14.0, 12.0),   // ZS1
            make_zs("bi_zs", 15, 24, 19.0, 17.0),   // ZS2: 上涨递进
            make_zs("bi_zs", 27, 36, 11.0, 9.0),    // ZS3: 下跌（zg=11 < ZS2.zd=17）
        ];
        let groups = group_zs_by_trend(&zs);
        assert_eq!(groups.len(), 2, "方向变化应分为两组");
        assert_eq!(groups[0], vec![0, 1]);  // 上涨组
        assert_eq!(groups[1], vec![2]);      // 下跌组
    }

    #[test]
    fn test_beichi_with_three_zs() {
        // 3中枢上涨趋势 + 趋势背驰
        let bis = vec![
            make_bi(0, "up", 10.0, 15.0, 0, 3),
            make_bi(1, "down", 15.0, 12.0, 3, 6),
            make_bi(2, "up", 12.0, 14.0, 6, 9),
            make_bi(3, "down", 14.0, 13.0, 9, 12),    // ZS1: [12,14]
            make_bi(4, "up", 13.0, 22.0, 12, 16),      // b1段
            make_bi(5, "down", 22.0, 19.0, 16, 19),
            make_bi(6, "up", 19.0, 21.0, 19, 22),
            make_bi(7, "down", 21.0, 20.0, 22, 25),    // ZS2: [19,21]
            make_bi(8, "up", 20.0, 28.0, 25, 28),      // b2段
            make_bi(9, "down", 28.0, 23.0, 28, 31),
            make_bi(10, "up", 23.0, 25.0, 31, 34),
            make_bi(11, "down", 25.0, 24.0, 34, 37),   // ZS3: [23,25]
            make_bi(12, "up", 24.0, 30.0, 37, 40),     // c段（创新高30>28，但力度小）
        ];
        let zs = vec![
            make_zs("bi_zs", 3, 12, 14.0, 12.0),
            make_zs("bi_zs", 16, 25, 21.0, 19.0),
            make_zs("bi_zs", 28, 37, 25.0, 23.0),
        ];

        let mut macd_hist = vec![0.0; 41];
        for i in 12..=16 { macd_hist[i] = 5.0; }  // b1段力度大
        for i in 25..=28 { macd_hist[i] = 4.0; }  // b2段
        for i in 37..=40 { macd_hist[i] = 1.0; }  // c段力度小
        let macd = MacdData { dif: vec![0.0; 41], dea: vec![0.0; 41], macd_hist };

        let beichi = detect_bi_beichi(&bis, &macd, &zs);
        let trend_bc: Vec<_> = beichi.iter().filter(|b| b.bc_sub_type == "trend").collect();
        assert!(!trend_bc.is_empty(), "应检测到趋势背驰");
        assert_eq!(trend_bc[0].direction, "up");
    }

    #[test]
    fn test_no_panzheng_beichi_when_different_direction() {
        // a段向上、b段向下 → 方向不同 → 不构成盘整背驰
        let bis = vec![
            make_bi(0, "up", 10.0, 15.0, 0, 3),     // a段（向上）
            make_bi(1, "down", 15.0, 12.0, 3, 6),
            make_bi(2, "up", 12.0, 14.0, 6, 9),
            make_bi(3, "down", 14.0, 13.0, 9, 12),   // 中枢
            make_bi(4, "down", 13.0, 10.0, 12, 15),   // b段（向下，与a方向不同）
        ];
        let zs = vec![make_zs("bi_zs", 3, 12, 14.0, 12.0)];

        let mut macd_hist = vec![0.0; 16];
        for i in 0..=3 { macd_hist[i] = 5.0; }
        for i in 12..=15 { macd_hist[i] = -1.0; }  // b段是下跌
        let macd = MacdData { dif: vec![0.0; 16], dea: vec![0.0; 16], macd_hist };

        let beichi = detect_bi_beichi(&bis, &macd, &zs);
        let pz: Vec<_> = beichi.iter().filter(|b| b.bc_sub_type == "panzheng").collect();
        assert!(pz.is_empty(), "a、b方向不同不应构成盘整背驰");
    }

    #[test]
    fn test_no_trend_beichi_without_third_bs() {
        // 37课约束：c段不包含第三类买卖点 → 不构成趋势背驰
        // 上涨趋势中，c段低点回到中枢下边界之内 → 未确认离开中枢
        let bis = vec![
            make_bi(0, "up", 10.0, 15.0, 0, 3),
            make_bi(1, "down", 15.0, 12.0, 3, 6),
            make_bi(2, "up", 12.0, 14.0, 6, 9),
            make_bi(3, "down", 14.0, 13.0, 9, 12),    // ZS1: [12,14]
            make_bi(4, "up", 13.0, 22.0, 12, 16),      // b段
            make_bi(5, "down", 22.0, 17.0, 16, 19),
            make_bi(6, "up", 17.0, 19.0, 19, 22),
            make_bi(7, "down", 19.0, 18.0, 22, 25),    // ZS2: [17,19]
            make_bi(8, "up", 16.5, 25.0, 25, 29),      // c段（创新高25>22, 但c.low=16.5<ZS2.zd=17→未确认离开中枢）
        ];
        let zs = vec![
            make_zs("bi_zs", 3, 12, 14.0, 12.0),   // ZS1
            make_zs("bi_zs", 16, 25, 19.0, 17.0),  // ZS2: zd=17, c.low=16.5<17 → 不满足第三类买点
        ];

        let mut macd_hist = vec![0.0; 30];
        for i in 12..=16 { macd_hist[i] = 5.0; }  // b段力度大
        for i in 25..=29 { macd_hist[i] = 1.0; }  // c段力度小
        let macd = MacdData { dif: vec![0.0; 30], dea: vec![0.0; 30], macd_hist };

        let beichi = detect_bi_beichi(&bis, &macd, &zs);
        let trend_bc: Vec<_> = beichi.iter().filter(|b| b.bc_sub_type == "trend").collect();
        assert!(trend_bc.is_empty(), "c段不包含第三类买点不应构成趋势背驰(37课)");
    }
}
