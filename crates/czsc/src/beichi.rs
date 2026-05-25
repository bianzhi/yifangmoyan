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

use yifang_data::{Bi, BeiChi, MacdData, XianDuan, ZhongShu, ZouShi};

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
    /// DIF 极值: 上涨段取最高, 下跌段取最低（27课黄白线判断用）
    dif_peak: f64,
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
            if let Some(bd) = check_trend_backdivergence(&group_zs, &sections, macd, bc_type) {
                results.push(bd);
            } else {
                // 37课兜底：趋势背驰条件不满足时，回退到盘整背驰判断
                // "否则，就算c包含B的第三类买卖点，也可以对围绕B的次级别震荡
                //  用盘整背驰的方式进行判断。"（37课原文）
                for &zs_idx in group_indices.iter() {
                    if let Some(bd) = check_panzheng_backdivergence(&zs_list[zs_idx], &sections, macd, bc_type) {
                        results.push(bd);
                    }
                }
            }
        } else {
            // 单中枢 → 盘整 → 检测盘整背驰
            let zs_idx = group_indices[0];
            if let Some(bd) = check_panzheng_backdivergence(&zs_list[zs_idx], &sections, macd, bc_type) {
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
                dif_peak: 0.0,
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
        section.dif_peak = 0.0;
        return;
    }

    let start = (section.start_idx as usize).min(macd.macd_hist.len() - 1);
    let end = (section.end_idx as usize).min(macd.macd_hist.len() - 1);

    if start > end {
        section.macd_area = 0.0;
        section.macd_peak = 0.0;
        section.dif_peak = 0.0;
        return;
    }

    let hist_slice = &macd.macd_hist[start..=end];

    if section.direction == "up" {
        // 上涨：只看红柱（正值）
        let area: f64 = hist_slice.iter().copied().filter(|&v| v > 0.0).sum();
        let peak = hist_slice.iter().copied().fold(0.0_f64, |a, b| a.max(b));
        section.macd_area = area;
        section.macd_peak = peak;
        // DIF极值: 上涨段取最高（27课黄白线判断）
        if start < macd.dif.len() && end < macd.dif.len() {
            section.dif_peak = macd.dif[start..=end]
                .iter()
                .cloned()
                .fold(f64::MIN, f64::max);
        }
    } else {
        // 下跌：只看绿柱（负值取绝对值）
        let area: f64 = hist_slice.iter().copied().filter(|&v| v < 0.0).map(|v| v.abs()).sum();
        let peak = hist_slice.iter().copied().map(|v| v.abs()).fold(0.0_f64, |a, b| a.max(b));
        section.macd_area = area;
        section.macd_peak = peak;
        // DIF极值: 下跌段取最低（27课黄白线判断）
        if start < macd.dif.len() && end < macd.dif.len() {
            section.dif_peak = macd.dif[start..=end]
                .iter()
                .cloned()
                .fold(f64::MAX, f64::min);
        }
    }
}

// ─── 趋势背驰判断 ─────────────────────────────────────

/// 趋势背驰（缠论37课 + 27课黄白线判断）
///
/// 必须 a+A+b+B+c 是趋势，且 c 段创新高/新低。
///
/// 37课原文：
/// "如果a+A+b+B+c是上涨，c一定要创出新高；a+A+b+B+c是下跌，c一定要创出新低。
///  否则，就算c包含B的第三类买卖点，也可以对围绕B的次级别震荡用盘整背驰的方式
///  进行判断。"
///
/// 27课原文（两个独立的背驰判断标准，满足其一即可）：
/// "回抽0轴的黄白线再次下跌不创新低，而且柱子的面积是明显小于第1段的，
///  一般来说，只要其中一个符合就可以是一个背弛的信号，两个都满足就更标准了。"
fn check_trend_backdivergence(
    zs_group: &[&ZhongShu],
    sections: &[TrendSection],
    macd: &MacdData,
    bc_type: &str,
) -> Option<BeiChi> {
    // 趋势必须至少2个中枢
    if zs_group.len() < 2 {
        return None;
    }

    // 确认是同方向无重叠趋势
    let trend_direction = classify_zs_direction(zs_group[0], zs_group[1]);

    for w in zs_group.windows(2) {
        if trend_direction == "up" && w[1].zd <= w[0].zg {
            return None;
        }
        if trend_direction == "down" && w[1].zg >= w[0].zd {
            return None;
        }
    }

    let last_zs = zs_group.last().unwrap();
    let second_last_zs = zs_group[zs_group.len() - 2];

    // 找b段：连接倒数第二中枢和最后一个中枢的离开段
    // b段必须从倒数第二个中枢出发，到达最后一个中枢区域
    // 条件：start在2nd_last_zs内或之后，end到达last_zs区域(start_index)
    let mut b_section: Option<&TrendSection> = None;
    for sec in sections.iter() {
        if sec.direction == trend_direction
            && sec.start_idx >= second_last_zs.start_index
            && sec.start_idx < last_zs.start_index
            && sec.end_idx >= last_zs.start_index  // ★ b段必须连接到最后一个中枢
        {
            if sec.macd_area > 0.0 {
                b_section = Some(sec);
                break;
            }
        }
    }
    if b_section.is_none() {
        for sec in sections.iter() {
            if sec.direction == trend_direction
                && sec.start_idx >= second_last_zs.start_index
                && sec.start_idx < last_zs.start_index
            {
                if sec.macd_area > 0.0 {
                    b_section = Some(sec);
                    break;
                }
            }
        }
    }

    let b_section = b_section?;

    // 找c段：最后一个中枢的离开段
    // 缠论核心：c段必须价格突破中枢范围（下跌突破ZD，上涨突破ZG）
    // 限制搜索范围：c段应在last_zs结束后合理范围内（不超过一个中枢长度）
    let zs_len = last_zs.end_index.saturating_sub(last_zs.start_index);
    let c_max_start = last_zs.end_index.saturating_add(zs_len);
    let c_section = sections.iter()
        .filter(|sec| {
            sec.start_idx >= last_zs.start_index
                && sec.start_idx <= c_max_start  // 限制c段不能跨太远
                && sec.direction == trend_direction
        })
        .filter(|sec| {
            if trend_direction == "down" {
                sec.low() < last_zs.zd  // 价格跌破中枢下沿
            } else {
                sec.high() > last_zs.zg  // 价格升破中枢上沿
            }
        })
        .last();  // 取最后一个突破中枢的段（即真正的c段）
    let c_section = match c_section {
        Some(c) => c,
        None => {
            return None;
        }
    };

    // 必要条件：c段必须创新高/新低
    let cond_new_extreme = if trend_direction == "up" {
        c_section.high() > b_section.high()
    } else {
        c_section.low() < b_section.low()
    };

    if !cond_new_extreme {
        return None;
    }

    // ─── 力度比较（两个独立标准，OR 关系）───

    // 标准A：MACD柱子面积缩小（24课/27课）
    let cond_area = c_section.macd_area < b_section.macd_area;

    // 标准B：黄白线回抽0轴后力度衰减（27课）
    // "回抽0轴的黄白线再次下跌不创新低"
    let cond_dif = check_dif_divergence_trend(
        b_section, c_section, last_zs, macd, &trend_direction,
    );

    // 27课原文："只要其中一个符合就可以是一个背弛的信号"
    if !cond_area && !cond_dif {
        return None;
    }

    // 生成背驰结果
    let direction_label = if trend_direction == "up" { "顶背驰" } else { "底背驰" };
    let matched = if cond_area && cond_dif { "面积+DIF" }
        else if cond_area { "面积" }
        else { "DIF" };
    let reason = format!(
        "趋势{}[{}]: b面积{:.2}峰{:.2} DIF{:.4}, c面积{:.2}({:.0}%)峰{:.2}({:.0}%) DIF{:.4}",
        direction_label, matched,
        b_section.macd_area,
        b_section.macd_peak,
        b_section.dif_peak,
        c_section.macd_area,
        if b_section.macd_area > 0.0 { c_section.macd_area / b_section.macd_area * 100.0 } else { 0.0 },
        c_section.macd_peak,
        if b_section.macd_peak > 0.0 { c_section.macd_peak / b_section.macd_peak * 100.0 } else { 0.0 },
        c_section.dif_peak,
    );

    Some(BeiChi {
        bc_type: bc_type.to_string(),
        index: c_section.end_idx,
        dt: String::new(),
        direction: trend_direction,
        bc_sub_type: "trend".to_string(),
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
/// 27课原文（两个独立标准，OR关系）：
/// "回抽0轴的黄白线再次下跌不创新低，而且柱子的面积是明显小于第1段的，
///  一般来说，只要其中一个符合就可以是一个背弛的信号"
///
/// 盘整背驰只标记与走势方向一致的段：
/// - 如果中枢之前的走势方向是上涨，a和b都是向上的段，盘整背驰 = 顶背驰
/// - 如果中枢之前的走势方向是下跌，a和b都是向下的段，盘整背驰 = 底背驰
fn check_panzheng_backdivergence(
    zs: &ZhongShu,
    sections: &[TrendSection],
    macd: &MacdData,
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

    // ─── 力度比较（两个独立标准，OR 关系）───

    // 标准A：MACD柱子面积缩小（24课/27课）
    let cond_area = b_sec.macd_area < a_sec.macd_area;

    // 标准B：黄白线回抽0轴后力度衰减（27课）
    let cond_dif = check_dif_divergence_panzheng(
        a_sec, b_sec, zs, macd, &a_sec.direction,
    );

    // 27课原文："只要其中一个符合就可以是一个背弛的信号"
    if !cond_area && !cond_dif {
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
    let matched = if cond_area && cond_dif { "面积+DIF" }
        else if cond_area { "面积" }
        else { "DIF" };
    let reason = format!(
        "盘整{}({})[{}]: a面积{:.2}峰{:.2} DIF{:.4}, b面积{:.2}({:.0}%)峰{:.2}({:.0}%) DIF{:.4}",
        direction_label, broken_label, matched,
        a_sec.macd_area,
        a_sec.macd_peak,
        a_sec.dif_peak,
        b_sec.macd_area,
        if a_sec.macd_area > 0.0 { b_sec.macd_area / a_sec.macd_area * 100.0 } else { 0.0 },
        b_sec.macd_peak,
        if a_sec.macd_peak > 0.0 { b_sec.macd_peak / a_sec.macd_peak * 100.0 } else { 0.0 },
        b_sec.dif_peak,
    );

    Some(BeiChi {
        bc_type: bc_type.to_string(),
        index: b_sec.end_idx,
        dt: String::new(),
        direction: b_sec.direction.clone(),
        bc_sub_type: "panzheng".to_string(),
        reason,
    })
}

// ─── DIF 黄白线背驰检查 ───────────────────────────────

/// 趋势背驰的 DIF 黄白线判断（27课标准B）
///
/// "回抽0轴的黄白线再次下跌不创新低"（27课）
///
/// 条件：
/// 1. DIF 在最后一个中枢期间回抽0轴附近（|DIF| < 总DIF波幅的15%）
/// 2. c段 DIF极值 < b段 DIF极值（顶背驰）或 c段DIF最低 > b段DIF最低（底背驰）
fn check_dif_divergence_trend(
    b_section: &TrendSection,
    c_section: &TrendSection,
    last_zs: &ZhongShu,
    macd: &MacdData,
    direction: &str,
) -> bool {
    if macd.dif.is_empty() {
        return false;
    }
    let n = macd.dif.len();

    // 计算整体DIF波幅，用于判断"回抽0轴"
    let dif_range = macd.dif.iter().cloned().fold(f64::MIN, f64::max)
        - macd.dif.iter().cloned().fold(f64::MAX, f64::min);
    if dif_range <= 0.0 {
        return false;
    }
    let near_zero = dif_range * 0.15; // 15%波幅以内视为接近0轴

    // 检查DIF在中枢期间是否回抽0轴
    let zs_start = (last_zs.start_index as usize).min(n - 1);
    let zs_end = (last_zs.end_index as usize).min(n - 1);
    let dif_during_zs = &macd.dif[zs_start..=zs_end];
    let pulled_to_zero = dif_during_zs.iter().any(|&v| v.abs() < near_zero);

    if !pulled_to_zero {
        return false;
    }

    // 比较b段和c段的DIF极值
    let b_start = (b_section.start_idx as usize).min(n - 1);
    let b_end = (b_section.end_idx as usize).min(n - 1);
    let c_start = (c_section.start_idx as usize).min(n - 1);
    let c_end = (c_section.end_idx as usize).min(n - 1);

    if b_start > b_end || c_start > c_end {
        return false;
    }

    if direction == "up" {
        // 顶背驰：c段DIF不创新高
        let b_dif_peak = macd.dif[b_start..=b_end].iter().cloned().fold(f64::MIN, f64::max);
        let c_dif_peak = macd.dif[c_start..=c_end].iter().cloned().fold(f64::MIN, f64::max);
        c_dif_peak < b_dif_peak
    } else {
        // 底背驰：c段DIF不创新低
        let b_dif_low = macd.dif[b_start..=b_end].iter().cloned().fold(f64::MAX, f64::min);
        let c_dif_low = macd.dif[c_start..=c_end].iter().cloned().fold(f64::MAX, f64::min);
        c_dif_low > b_dif_low
    }
}

/// 盘整背驰的 DIF 黄白线判断（27课标准B）
///
/// 同趋势背驰的 DIF 判断，但比较对象是 a 段和 b 段
fn check_dif_divergence_panzheng(
    a_section: &TrendSection,
    b_section: &TrendSection,
    zs: &ZhongShu,
    macd: &MacdData,
    direction: &str,
) -> bool {
    if macd.dif.is_empty() {
        return false;
    }
    let n = macd.dif.len();

    // 计算整体DIF波幅
    let dif_range = macd.dif.iter().cloned().fold(f64::MIN, f64::max)
        - macd.dif.iter().cloned().fold(f64::MAX, f64::min);
    if dif_range <= 0.0 {
        return false;
    }
    let near_zero = dif_range * 0.15;

    // 检查DIF在中枢期间是否回抽0轴
    let zs_start = (zs.start_index as usize).min(n - 1);
    let zs_end = (zs.end_index as usize).min(n - 1);
    let dif_during_zs = &macd.dif[zs_start..=zs_end];
    let pulled_to_zero = dif_during_zs.iter().any(|&v| v.abs() < near_zero);

    if !pulled_to_zero {
        return false;
    }

    let a_start = (a_section.start_idx as usize).min(n - 1);
    let a_end = (a_section.end_idx as usize).min(n - 1);
    let b_start = (b_section.start_idx as usize).min(n - 1);
    let b_end = (b_section.end_idx as usize).min(n - 1);

    if a_start > a_end || b_start > b_end {
        return false;
    }

    if direction == "up" {
        let a_dif_peak = macd.dif[a_start..=a_end].iter().cloned().fold(f64::MIN, f64::max);
        let b_dif_peak = macd.dif[b_start..=b_end].iter().cloned().fold(f64::MIN, f64::max);
        b_dif_peak < a_dif_peak
    } else {
        let a_dif_low = macd.dif[a_start..=a_end].iter().cloned().fold(f64::MAX, f64::min);
        let b_dif_low = macd.dif[b_start..=b_end].iter().cloned().fold(f64::MAX, f64::min);
        b_dif_low > a_dif_low
    }
}

// ─── 走势级别背驰 ──────────────────────────────────────

/// 走势级别背驰检测（P1）
///
/// 缠论原文中背驰是最标准的定义在走势级别上的（1F趋势、5F趋势…），
/// 不是笔/线段级别。走势级别背驰检测将每个走势视为趋势段，其内部的中枢列表
/// 决定了它是盘整（单中枢）还是趋势（多中枢），并在此基础上检测背驰。
///
/// 与笔/线段级别背驰的区别：
/// - 笔/线段背驰：对单一构造元素序列直接检测
/// - 走势级别背驰：对递归后的走势对象进行检测，更接近缠论原文定义
pub fn detect_zoushi_beichi(
    zoushi: &[ZouShi],
    macd: &MacdData,
    xds: &[XianDuan],
) -> Vec<BeiChi> {
    let mut results = Vec::new();

    for zs_item in zoushi {
        if zs_item.zs_list.is_empty() {
            continue;
        }

        // 将当前的线段映射为 TrendSection
        let sections: Vec<TrendSection> = xds
            .iter()
            .filter(|xd| {
                xd.start_index >= zs_item.start_index && xd.end_index <= zs_item.end_index
            })
            .map(|xd| {
                let mut sec = TrendSection {
                    direction: xd.direction.clone(),
                    start_idx: xd.start_index,
                    end_idx: xd.end_index,
                    start_val: xd.start_price,
                    end_val: xd.end_price,
                    macd_area: 0.0,
                    macd_peak: 0.0,
                    dif_peak: 0.0,
                };
                calculate_section_power(&mut sec, macd);
                sec
            })
            .collect();

        if sections.len() < 4 {
            continue;
        }

        if zs_item.zs_list.len() >= 2 {
            // 走势包含 ≥2 个中枢 → 趋势背驰
            let zs_refs: Vec<&ZhongShu> = zs_item.zs_list.iter().collect();
            if let Some(bd) = check_trend_backdivergence(
                &zs_refs, &sections, macd, "zoushi_beichi",
            ) {
                results.push(bd);
            } else {
                // 37课兜底：回退到盘整背驰
                for zs_ref in &zs_item.zs_list {
                    if let Some(bd) = check_panzheng_backdivergence(
                        zs_ref, &sections, macd, "zoushi_beichi",
                    ) {
                        results.push(bd);
                    }
                }
            }
        } else {
            // 单中枢 → 盘整背驰
            let zs_ref = &zs_item.zs_list[0];
            if let Some(bd) = check_panzheng_backdivergence(
                zs_ref, &sections, macd, "zoushi_beichi",
            ) {
                results.push(bd);
            }
        }
    }

    // 去重：按 index 合并相同位置的背驰
    results.sort_by_key(|b| b.index);
    results.dedup_by(|a, b| a.index == b.index);

    results
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
            dif_peak: 0.0,
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
            dif_peak: 0.0,
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
    fn test_trend_beichi_without_third_bs_still_valid() {
        // 37课原文："c一定要创出新高...否则，就算c包含B的第三类买卖点，
        //           也可以对围绕B的次级别震荡用盘整背驰的方式进行判断。"
        // 原文意思是：三买不三买的，关键是c创新高。三买不是必要条件。
        // 此例c创新高(25>22)且力度衰减 → 应该是趋势背驰。
        let bis = vec![
            make_bi(0, "up", 10.0, 15.0, 0, 3),
            make_bi(1, "down", 15.0, 12.0, 3, 6),
            make_bi(2, "up", 12.0, 14.0, 6, 9),
            make_bi(3, "down", 14.0, 13.0, 9, 12),    // ZS1: [12,14]
            make_bi(4, "up", 13.0, 22.0, 12, 16),      // b段
            make_bi(5, "down", 22.0, 17.0, 16, 19),
            make_bi(6, "up", 17.0, 19.0, 19, 22),
            make_bi(7, "down", 19.0, 18.0, 22, 25),    // ZS2: [17,19]
            make_bi(8, "up", 16.5, 25.0, 25, 29),      // c段（创新高25>22, c.low=16.5<ZS2.zd=17不构成三买，但三买非趋势背驰必要条件）
        ];
        let zs = vec![
            make_zs("bi_zs", 3, 12, 14.0, 12.0),   // ZS1
            make_zs("bi_zs", 16, 25, 19.0, 17.0),  // ZS2: zd=17
        ];

        let mut macd_hist = vec![0.0; 30];
        for i in 12..=16 { macd_hist[i] = 5.0; }  // b段力度大
        for i in 25..=29 { macd_hist[i] = 1.0; }  // c段力度小（面积衰减）
        let macd = MacdData { dif: vec![0.0; 30], dea: vec![0.0; 30], macd_hist };

        let beichi = detect_bi_beichi(&bis, &macd, &zs);
        let trend_bc: Vec<_> = beichi.iter().filter(|b| b.bc_sub_type == "trend").collect();
        assert!(!trend_bc.is_empty(), "c创新高且力度衰减，三买不是必要条件，应构成趋势背驰(37课)");
        assert_eq!(trend_bc[0].direction, "up");
        assert!(trend_bc[0].reason.contains("面积"));
    }

    #[test]
    fn test_trend_beichi_when_c_starts_within_last_zs() {
        // 真实K线场景：c段的转折低点落在最后一个中枢内部（而非恰好在中枢结束后）
        // 旧代码要求 start_idx >= last_zs.end_index（=25），会漏掉此信号
        //
        // 结构：a + ZS1 + b + ZS2 + c
        //        c段起点(23)在ZS2区间[16,25]内部，终点(29)突破中枢
        let bis = vec![
            make_bi(0, "up", 10.0, 15.0, 0, 3),
            make_bi(1, "down", 15.0, 12.0, 3, 6),
            make_bi(2, "up", 12.0, 14.0, 6, 9),
            make_bi(3, "down", 14.0, 13.0, 9, 12),    // ZS1: [12,14]
            make_bi(4, "up", 13.0, 22.0, 12, 16),      // b段（连接ZS1→ZS2）
            make_bi(5, "down", 22.0, 17.0, 16, 19),
            make_bi(6, "up", 17.0, 19.0, 19, 22),
            make_bi(7, "down", 19.0, 18.0, 22, 25),    // ZS2: [17,19]
            make_bi(8, "up", 18.0, 25.0, 23, 29),      // c段！start_idx=23 < ZS2.end_index=25
        ];
        let zs = vec![
            make_zs("bi_zs", 3, 12, 14.0, 12.0),   // ZS1: end_index=12
            make_zs("bi_zs", 16, 25, 19.0, 17.0),  // ZS2: start_index=16, end_index=25
        ];

        let mut macd_hist = vec![0.0; 30];
        for i in 12..=16 { macd_hist[i] = 5.0; }  // b段力度大
        for i in 23..=29 { macd_hist[i] = 1.0; }  // c段力度小
        let macd = MacdData { dif: vec![0.0; 30], dea: vec![0.0; 30], macd_hist };

        let beichi = detect_bi_beichi(&bis, &macd, &zs);
        let trend_bc: Vec<_> = beichi.iter().filter(|b| b.bc_sub_type == "trend").collect();
        assert!(!trend_bc.is_empty(),
            "c段起点在中枢内部(23<25)也应该检测到趋势背驰——这是真实K线最常见的情况");
        assert_eq!(trend_bc[0].direction, "up");
    }
}
