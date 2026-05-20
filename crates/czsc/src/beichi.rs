//! 背驰检测 — 严格遵循缠论24/27/37课原始定义
//!
//! **背驰本质**：同级别同方向走势的力度衰减（缠论第27课）
//!
//! 缠论原文（27课）核心论述：
//! "走势力度可以用MACD的红绿柱面积来衡量"
//! "背驰就是后一段走势的力度小于前一段"
//!
//! 关键理解（纠正常见误区）：
//! - 背驰 ≠ 必须创新高新低。缠论原文从未要求"创新高新低"才能背驰
//! - 背驰唯一条件：同方向前后的力度比较，后段力度 < 前段力度
//! - 不创新高新低的背驰叫"标准背驰"，创新高新低但力度衰减的叫"扩展背驰"
//! - 两者都是有效的背驰，但意义不同：标准背驰确定性更高
//!
//! **趋势背驰**（24课）：
//! - 至少2个同方向无重叠中枢
//! - 最后一个中枢之后的离开段(c)力度 < 前一个离开段(b)力度
//!
//! **盘整背驰**（37课）：
//! - 只有1个中枢
//! - 中枢前后两段同方向走势力度比较，后段 < 前段
//!
//! **力度衡量**：
//! - 上涨走势：只累加MACD红柱（正值）面积和峰值
//! - 下跌走势：只累加MACD绿柱（负值）绝对值面积和峰值
//! - 面积是核心指标，峰值是辅助，必须两者同时衰减

use yifang_data::{Bi, BeiChi, MacdData, XianDuan, ZhongShu};

// ─── 走势段（用于力度比较的最小单位）──────────────────────

#[derive(Debug, Clone)]
struct TrendSection {
    /// 方向
    direction: String,
    /// 起点 K 线索引
    start_idx: u64,
    /// 终点 K 线索引
    end_idx: u64,
    /// 起点价格
    start_val: f64,
    /// 终点价格
    end_val: f64,
    /// 关联中枢索引（段起点在中枢内时关联）
    zs_idx: Option<usize>,
    /// MACD 面积（力度核心指标）
    macd_area: f64,
    /// MACD 峰值（力度辅助指标）
    macd_peak: f64,
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

    // 步骤1：将段列表转为走势段
    let sections = build_trend_sections(segments, zs_list, &extract);

    // 步骤2：按中枢方向分组，分别判断
    let groups = group_zs_by_direction(zs_list);

    for group_indices in &groups {
        if group_indices.len() == 1 {
            // 单中枢 → 盘整背驰
            let zs_idx = group_indices[0];
            if let Some(bd) = check_panzheng_backdivergence(&zs_list[zs_idx], &sections, macd) {
                results.push(make_beichi(bc_type, &bd));
            }
        } else if group_indices.len() >= 2 {
            // 多个同方向无重叠中枢 → 趋势背驰
            let group_zs: Vec<&ZhongShu> = group_indices.iter().map(|&i| &zs_list[i]).collect();
            if let Some(bd) = check_trend_backdivergence(&group_zs, &sections, macd) {
                results.push(make_beichi(bc_type, &bd));
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
/// - 上涨趋势组：后续中枢 zd > 前中枢 zg（无重叠向上递进）
/// - 下跌趋势组：后续中枢 zg < 前中枢 zd（无重叠向下递进）
/// - 方向改变则开新组
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
                let last = group.last().unwrap();
                let second_last = group[group.len() - 2];
                classify_zs_direction(&zs_list[second_last], &zs_list[*last])
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
/// 缠论原始定义：后中枢整体高于前中枢 → 上涨，反之 → 下跌
fn classify_zs_direction(prev: &ZhongShu, curr: &ZhongShu) -> String {
    if curr.zd > prev.zg {
        "up".to_string()
    } else if curr.zg < prev.zd {
        "down".to_string()
    } else if curr.zd > prev.zd {
        "up".to_string()
    } else {
        "down".to_string()
    }
}

// ─── MACD 力度计算 ─────────────────────────────────────

/// 计算单个走势段的MACD力度
///
/// 缠论标准（27课）：
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
/// 3. 后一段走势的MACD面积 < 前一段
/// 4. 后一段走势的MACD峰值 < 前一段的峰值
///
/// 注意：缠论原文从未要求"必须创新高/新低"才能背驰
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

    // 前一段力度为0，无法比较
    if prev.macd_area <= 0.0 || prev.macd_peak <= 0.0 {
        return None;
    }

    // 盘整背驰条件：后段力度 < 前段力度
    // 缠论27课：面积和峰值必须同时衰减
    let cond_area = curr.macd_area < prev.macd_area;
    let cond_peak = curr.macd_peak < prev.macd_peak;

    if cond_area && cond_peak {
        Some(BackDivergenceResult {
            bc_sub_type: "panzheng".to_string(),
            direction: curr.direction.clone(),
            index: curr.end_idx,
            prev_section: prev,
            curr_section: curr,
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
/// 3. 背驰段c的MACD面积 < 连接段b的面积
/// 4. 背驰段c的MACD峰值 < 连接段b的峰值
///
/// 重要纠正：缠论原文从未要求"c段必须创新高/新低"
/// "不创新高新低"的标准背驰和"创新高新低但力度衰减"的扩展背驰都有效
/// 标准背驰确定性更高，但扩展背驰也是背驰
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

    // 找b段和c段
    // 缠论定义：
    // b段 = 两个中枢之间的连接段（力度较强的参考段）
    // c段 = 最后一个中枢之后的离开段（力度较弱的背驰段）
    //
    // 比较策略：用趋势中最强的b段与最后的c段比较
    // 即：遍历所有相邻中枢对间的连接段，选面积最大的作为参考段b
    let first_zs = zs_group[0];
    let last_zs = zs_group.last().unwrap();

    // 收集所有中枢对之间的连接段（b段候选）
    let mut best_prev: Option<(usize, TrendSection)> = None;

    for w in zs_group.windows(2) {
        let zsa = w[0];
        let zsb = w[1];

        // 找到两个中枢之间的连接段
        for (i, sec) in sections.iter().enumerate() {
            if sec.direction == direction
                && sec.start_idx >= zsa.end_index
                && sec.start_idx < zsb.start_index
            {
                let mut candidate = sec.clone();
                calculate_section_power(&mut candidate, macd);
                if candidate.macd_area > 0.0 {
                    let is_better = match &best_prev {
                        None => true,
                        Some((_, best)) => candidate.macd_area > best.macd_area,
                    };
                    if is_better {
                        best_prev = Some((i, candidate));
                    }
                }
                break; // 只取第一个符合条件的段
            }
        }
        // 放宽条件：段起点在第一个中枢结束之后即可
        if best_prev.is_none() {
            for (i, sec) in sections.iter().enumerate() {
                if sec.direction == direction
                    && sec.start_idx >= zsa.end_index
                    && sec.start_idx < zsb.end_index
                {
                    let mut candidate = sec.clone();
                    calculate_section_power(&mut candidate, macd);
                    if candidate.macd_area > 0.0 {
                        best_prev = Some((i, candidate));
                        break;
                    }
                }
            }
        }
    }

    // c段：最后一个中枢之后的离开段
    let mut curr_section: Option<usize> = None;
    for (i, sec) in sections.iter().enumerate() {
        if sec.start_idx >= last_zs.end_index && curr_section.is_none() {
            // c段方向必须与趋势方向一致
            if sec.direction == direction {
                curr_section = Some(i);
            }
        }
    }

    let (_, prev) = best_prev?;
    let curr_i = curr_section?;

    let mut curr = sections[curr_i].clone();

    // c段方向必须与趋势方向一致
    if curr.direction != direction {
        return None;
    }

    calculate_section_power(&mut curr, macd);

    // 前一段力度为0，无法比较
    if prev.macd_area <= 0.0 || prev.macd_peak <= 0.0 {
        return None;
    }

    // 趋势背驰条件：c段力度 < b段力度
    // 缠论27课：面积和峰值必须同时衰减
    let cond_area = curr.macd_area < prev.macd_area;
    let cond_peak = curr.macd_peak < prev.macd_peak;

    if cond_area && cond_peak {
        Some(BackDivergenceResult {
            bc_sub_type: "trend".to_string(),
            direction: curr.direction.clone(),
            index: curr.end_idx,
            prev_section: prev,
            curr_section: curr,
        })
    } else {
        None
    }
}

// ─── 内部结果结构 ─────────────────────────────────────

/// 背驰判断结果（内部使用）
struct BackDivergenceResult {
    /// 背驰子类型: "trend" / "panzheng"
    bc_sub_type: String,
    /// 方向: "up" / "down"
    direction: String,
    /// 背驰段终点K线索引
    index: u64,
    /// 前一段走势（参考段）
    prev_section: TrendSection,
    /// 当前背驰段
    curr_section: TrendSection,
}

/// 转换为 BeiChi 公开结构
fn make_beichi(bc_type: &str, bd: &BackDivergenceResult) -> BeiChi {
    let direction_label = if bd.direction == "up" { "顶背驰" } else { "底背驰" };
    let sub_label = if bd.bc_sub_type == "trend" { "趋势" } else { "盘整" };
    let reason = format!(
        "{}{}: 前段面积 {:.2} 峰值 {:.2}, 当前段面积 {:.2} ({:.0}%) 峰值 {:.2} ({:.0}%)",
        sub_label,
        direction_label,
        bd.prev_section.macd_area,
        bd.prev_section.macd_peak,
        bd.curr_section.macd_area,
        if bd.prev_section.macd_area > 0.0 { bd.curr_section.macd_area / bd.prev_section.macd_area * 100.0 } else { 0.0 },
        bd.curr_section.macd_peak,
        if bd.prev_section.macd_peak > 0.0 { bd.curr_section.macd_peak / bd.prev_section.macd_peak * 100.0 } else { 0.0 },
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
    use yifang_data::TimeFrame;

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

    fn make_kline(id: u64, open: f64, close: f64, high: f64, low: f64) -> yifang_data::KLine {
        yifang_data::KLine {
            symbol: "test".to_string(),
            timeframe: TimeFrame::D,
            dt: format!("2024-01-{:02}", (id as usize % 28) + 1),
            id,
            open, close, high, low,
            vol: 1000.0,
            amount: 10000.0,
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
    fn test_panzheng_beichi_structure() {
        // 盘整背驰：单中枢，前后段方向相同，后段力度 < 前段
        let bis = vec![
            make_bi(0, "up", 10.0, 15.0, 0, 3),     // a段（进入）
            make_bi(1, "down", 15.0, 12.0, 3, 6),
            make_bi(2, "up", 12.0, 14.0, 6, 9),
            make_bi(3, "down", 14.0, 13.0, 9, 12),   // 中枢 [12,14]
            make_bi(4, "up", 13.0, 16.0, 12, 15),     // b段（离开，力度小）
        ];
        let zs = vec![make_zs("bi_zs", 3, 12, 14.0, 12.0)];

        // 构造MACD：a段力度大，b段力度小
        let mut macd_hist = vec![0.0; 16];
        for i in 0..=3 { macd_hist[i] = 5.0; }  // a段面积大
        for i in 12..=15 { macd_hist[i] = 1.0; } // b段面积小
        let macd = MacdData { dif: vec![0.0; 16], dea: vec![0.0; 16], macd_hist };

        let beichi = detect_bi_beichi(&bis, &macd, &zs);
        let pz: Vec<_> = beichi.iter().filter(|b| b.bc_sub_type == "panzheng").collect();
        assert!(!pz.is_empty(), "应检测到盘整背驰");
    }

    #[test]
    fn test_trend_beichi_structure() {
        // 趋势背驰：2个递进中枢，c段力度 < b段力度
        let bis = vec![
            make_bi(0, "up", 10.0, 15.0, 0, 3),
            make_bi(1, "down", 15.0, 12.0, 3, 6),
            make_bi(2, "up", 12.0, 14.0, 6, 9),
            make_bi(3, "down", 14.0, 13.0, 9, 12),   // ZS1: [12,14]
            make_bi(4, "up", 13.0, 22.0, 12, 16),     // b段
            make_bi(5, "down", 22.0, 17.0, 16, 19),
            make_bi(6, "up", 17.0, 19.0, 19, 22),
            make_bi(7, "down", 19.0, 18.0, 22, 25),   // ZS2: [17,19]
            make_bi(8, "up", 18.0, 23.0, 25, 29),     // c段（力度小）
        ];
        let zs = vec![
            make_zs("bi_zs", 3, 12, 14.0, 12.0),   // ZS1
            make_zs("bi_zs", 16, 25, 19.0, 17.0),  // ZS2: zd=17 > ZS1.zg=14
        ];

        // 构造MACD：b段面积大，c段面积小
        let mut macd_hist = vec![0.0; 30];
        for i in 12..=16 { macd_hist[i] = 5.0; }  // b段力度大
        for i in 25..=29 { macd_hist[i] = 1.0; }  // c段力度小
        let macd = MacdData { dif: vec![0.0; 30], dea: vec![0.0; 30], macd_hist };

        let beichi = detect_bi_beichi(&bis, &macd, &zs);
        let trend_bc: Vec<_> = beichi.iter().filter(|b| b.bc_sub_type == "trend").collect();
        assert!(!trend_bc.is_empty(), "应检测到趋势背驰");
        assert_eq!(trend_bc[0].direction, "up", "应为上涨趋势背驰");
    }

    #[test]
    fn test_calculate_section_power_up() {
        let mut section = TrendSection {
            direction: "up".to_string(),
            start_idx: 0,
            end_idx: 4,
            start_val: 10.0,
            end_val: 15.0,
            zs_idx: None,
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
            zs_idx: None,
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
    fn test_trend_beichi_without_new_extreme() {
        // 扩展背驰：c段不创新高，但力度衰减 → 仍是背驰
        // 缠论原文从未要求"必须创新高/新低"才能背驰
        let bis = vec![
            make_bi(0, "up", 10.0, 15.0, 0, 3),
            make_bi(1, "down", 15.0, 12.0, 3, 6),
            make_bi(2, "up", 12.0, 14.0, 6, 9),
            make_bi(3, "down", 14.0, 13.0, 9, 12),   // ZS1: [12,14]
            make_bi(4, "up", 13.0, 22.0, 12, 16),     // b段（创新高22）
            make_bi(5, "down", 22.0, 17.0, 16, 19),
            make_bi(6, "up", 17.0, 19.0, 19, 22),
            make_bi(7, "down", 19.0, 18.0, 22, 25),   // ZS2: [17,19]
            make_bi(8, "up", 18.0, 21.0, 25, 29),     // c段（不创新高21<22，但力度衰减→仍是背驰）
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
        assert!(!trend_bc.is_empty(), "不创新高但力度衰减应仍是趋势背驰");
    }

    #[test]
    fn test_no_beichi_when_power_not_decreasing() {
        // 后段力度 > 前段 → 不背驰
        let bis = vec![
            make_bi(0, "up", 10.0, 15.0, 0, 3),
            make_bi(1, "down", 15.0, 12.0, 3, 6),
            make_bi(2, "up", 12.0, 14.0, 6, 9),
            make_bi(3, "down", 14.0, 13.0, 9, 12),
            make_bi(4, "up", 13.0, 22.0, 12, 16),
            make_bi(5, "down", 22.0, 17.0, 16, 19),
            make_bi(6, "up", 17.0, 19.0, 19, 22),
            make_bi(7, "down", 19.0, 18.0, 22, 25),
            make_bi(8, "up", 18.0, 23.0, 25, 29),     // c段力度大 → 不背驰
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
    fn test_group_zs_by_direction() {
        let zs = vec![
            make_zs("bi_zs", 3, 12, 14.0, 12.0),   // ZS1
            make_zs("bi_zs", 15, 24, 19.0, 17.0),   // ZS2: zd=17 > zg=14 → 上涨递进
            make_zs("bi_zs", 27, 36, 24.0, 22.0),   // ZS3: zd=22 > zg=19 → 继续上涨递进
        ];
        let groups = group_zs_by_direction(&zs);
        assert_eq!(groups.len(), 1, "3个上涨递进中枢应为一组");
        assert_eq!(groups[0].len(), 3);
    }

    #[test]
    fn test_classify_zs_direction() {
        let prev = make_zs("bi_zs", 0, 10, 14.0, 12.0);
        let curr_up = make_zs("bi_zs", 10, 20, 19.0, 17.0);
        assert_eq!(classify_zs_direction(&prev, &curr_up), "up");

        let curr_down = make_zs("bi_zs", 10, 20, 11.0, 9.0);
        assert_eq!(classify_zs_direction(&prev, &curr_down), "down");
    }

    #[test]
    fn test_beichi_with_multiple_b_candidates() {
        // 多中枢趋势中，取最强的b段与c段比较
        let bis = vec![
            make_bi(0, "up", 10.0, 15.0, 0, 3),
            make_bi(1, "down", 15.0, 12.0, 3, 6),
            make_bi(2, "up", 12.0, 14.0, 6, 9),
            make_bi(3, "down", 14.0, 13.0, 9, 12),   // ZS1: [12,14]
            make_bi(4, "up", 13.0, 22.0, 12, 16),     // b1段（力度最大）
            make_bi(5, "down", 22.0, 19.0, 16, 19),
            make_bi(6, "up", 19.0, 21.0, 19, 22),
            make_bi(7, "down", 21.0, 20.0, 22, 25),   // ZS2: [19,21]
            make_bi(8, "up", 20.0, 28.0, 25, 28),     // b2段
            make_bi(9, "down", 28.0, 23.0, 28, 31),
            make_bi(10, "up", 23.0, 25.0, 31, 34),
            make_bi(11, "down", 25.0, 24.0, 34, 37),  // ZS3: [23,25]
            make_bi(12, "up", 24.0, 26.0, 37, 40),    // c段（力度小）
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
    }
}
