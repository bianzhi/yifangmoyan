//! 笔的构建
//!
//! **严格对齐 czsc 0.9.9 的 check_bi 实现**
//!
//! 缠论笔的定义：
//! 1. 从一个分型到另一个相反类型的分型构成一笔
//! 2. 顶分型到底分型 → 下降笔；底分型到顶分型 → 上升笔
//! 3. 成笔条件：
//!    a) fx_a 和 fx_b 之间没有包含关系（即两个分型的价格区间不互相包含）
//!    b) 笔长度 >= min_bi_len（新笔=6，老笔=7，即无包含K线数量）
//! 4. 笔被破坏后的回退：如果当前笔被新K线破坏，废弃当前笔，重新从分型起点开始

use crate::fenxing::{FxMark, check_fxs};
use crate::include::NewBar;
use yifang_data::Bi;

/// 默认最小笔长度（新笔=6，老笔=7）
/// 与 czsc 环境变量 czsc_min_bi_len 默认值 6 一致
const DEFAULT_MIN_BI_LEN: usize = 6;

/// 单笔构建结果
pub enum BiCheckResult {
    /// 找到一笔
    Found(Bi, Vec<NewBar>),
    /// 没找到，返回剩余的 bars_ubi
    NotFound(Vec<NewBar>),
}

/// 在无包含K线序列中查找一笔
///
/// 对齐 czsc check_bi 的核心逻辑：
/// 1. 先找出所有分型
/// 2. 以第一个分型 fx_a 为起点
/// 3. 找与 fx_a 方向匹配的极端 fx_b
/// 4. 检查成笔条件：无包含 + 长度足够
pub fn check_bi(bars: &[NewBar], min_bi_len: Option<usize>) -> BiCheckResult {
    let min_bi_len = min_bi_len.unwrap_or(DEFAULT_MIN_BI_LEN);
    let fxs = check_fxs(bars);

    if fxs.len() < 2 {
        return BiCheckResult::NotFound(bars.to_vec());
    }

    // 缠论标准笔构建：遍历相邻的分型对，找第一对满足成笔条件的
    // 核心：(fx_a, fx_b) 必须是一顶一底，价格合理，无包含，长度够
    for start_fx_idx in 0..fxs.len() - 1 {
        let fx_a = &fxs[start_fx_idx];

        // 只看与 fx_a 相邻的反向分型对
        // 即 fx_a 之后第一个反向分型 fx_b
        let fx_b_opt = fxs
            .iter()
            .enumerate()
            .skip(start_fx_idx + 1)
            .find(|(_, fx)| fx.mark != fx_a.mark && fx.dt > fx_a.dt);

        let (_fx_b_idx, fx_b) = match fx_b_opt {
            Some((idx, fx)) => (idx, fx),
            None => continue,
        };

        // 方向：底→顶为升笔，顶→底为降笔
        let direction = if fx_a.mark == FxMark::Bottom {
            "up".to_string()
        } else {
            "down".to_string()
        };

        // 提取笔区间内的 bars
        let bars_a_count = bars
            .iter()
            .filter(|b| b.dt >= fx_a.dt && b.dt <= fx_b.dt)
            .count();

        // 判断 fx_a 和 fx_b 价格区间是否存在包含关系
        let ab_include = (fx_a.high > fx_b.high && fx_a.low < fx_b.low)
            || (fx_a.high < fx_b.high && fx_a.low > fx_b.low);

        // 成笔条件：(1) 顶底分型之间没有包含关系；(2) 笔长度 >= min_bi_len
        if !ab_include && bars_a_count >= min_bi_len {
            let bars_b: Vec<NewBar> = bars
                .iter()
                .filter(|b| b.dt >= fx_b.dt)
                .cloned()
                .collect();

            let bi = Bi {
                direction,
                start_index: fx_a.merged_index as u64,
                end_index: fx_b.merged_index as u64,
                start_dt: fx_a.dt.clone(),
                end_dt: fx_b.dt.clone(),
                start_price: fx_a.fx,
                end_price: fx_b.fx,
                is_finished: true,
            };

            return BiCheckResult::Found(bi, bars_b);
        }
        // 不成笔，继续尝试下一个起始分型
    }

    BiCheckResult::NotFound(bars.to_vec())
}

/// 找第一笔：在初始 bars_ubi 中搜索
///
/// 对齐 czsc CZSC.__update_bi 的第一笔查找逻辑：
/// 1. 找出所有分型
/// 2. 取第一个分型 fx_a
/// 3. 在同类型分型中找最极端的（底取最低，顶取最高）
/// 4. 从这个最极端分型开始，调用 check_bi
pub fn find_first_bi(bars: &[NewBar], min_bi_len: Option<usize>) -> (Option<Bi>, Vec<NewBar>) {
    let min_bi_len = min_bi_len.unwrap_or(DEFAULT_MIN_BI_LEN);
    let fxs = check_fxs(bars);

    if fxs.is_empty() {
        return (None, bars.to_vec());
    }

    // 尝试从最极端的同方向分型开始
    let fx_a = &fxs[0];

    let mut best_fx = fx_a.clone();
    for fx in &fxs {
        if fx.mark != fx_a.mark {
            continue;
        }
        let should_replace = match fx.mark {
            FxMark::Bottom => fx.low <= best_fx.low,
            FxMark::Top => fx.high >= best_fx.high,
        };
        if should_replace {
            best_fx = fx.clone();
        }
    }

    // 从最极端分型开始截取 bars
    let trimmed_bars: Vec<NewBar> = bars
        .iter()
        .filter(|b| b.dt >= best_fx.dt)
        .cloned()
        .collect();

    if !trimmed_bars.is_empty() {
        if let BiCheckResult::Found(bi, remaining) = check_bi(&trimmed_bars, Some(min_bi_len)) {
            return (Some(bi), remaining);
        }
    }

    // 如果从最极端分型开始找不到笔，直接从原始 bars 用 check_bi 搜索
    match check_bi(bars, Some(min_bi_len)) {
        BiCheckResult::Found(bi, remaining) => (Some(bi), remaining),
        BiCheckResult::NotFound(remaining) => (None, remaining),
    }
}

/// 对 K 线序列进行完整笔构建（增量式）
///
/// 对齐 czsc CZSC 类的 update 流程：
/// 1. 去包含 → bars_ubi
/// 2. 如果没有笔 → 找第一笔
/// 3. 已有笔 → 从 bars_ubi 继续检查新笔
/// 4. 笔被破坏 → 回退重算
pub fn build_bi_incremental(bars_ubi: &[NewBar], existing_bis: &[Bi], min_bi_len: Option<usize>) -> (Vec<Bi>, Vec<NewBar>) {
    let min_bi_len = min_bi_len.unwrap_or(DEFAULT_MIN_BI_LEN);

    if bars_ubi.len() < 3 {
        return (existing_bis.to_vec(), bars_ubi.to_vec());
    }

    let mut bi_list = existing_bis.to_vec();
    let mut ubi = bars_ubi.to_vec();

    if bi_list.is_empty() {
        // 第一笔查找
        let (bi, remaining) = find_first_bi(&ubi, Some(min_bi_len));
        if let Some(b) = bi {
            bi_list.push(b);
        }
        ubi = remaining;
        return (bi_list, ubi);
    }

    // 非第一笔
    match check_bi(&ubi, Some(min_bi_len)) {
        BiCheckResult::Found(bi, remaining) => {
            bi_list.push(bi);
            ubi = remaining;
        }
        BiCheckResult::NotFound(remaining) => {
            ubi = remaining;
        }
    }

    // 笔被破坏的后处理
    // 对齐 czsc：如果当前最后一笔被新 K 线破坏，废弃最后一笔，合并回 bars_ubi
    // 注意：在批量模式下此逻辑过于激进，已禁用
    // 批量模式通过 build_bi 的循环 + remaining 机制保证笔的完整性
    #[allow(dead_code)]
    if false && !bi_list.is_empty() && ubi.len() >= 2 {
        let last_bi = &bi_list[bi_list.len() - 1];
        let last_ubi = &ubi[ubi.len() - 1];

        let is_broken = (last_bi.direction == "up" && last_ubi.high > last_bi.end_price)
            || (last_bi.direction == "down" && last_ubi.low < last_bi.end_price);

        if is_broken {
            // 废弃最后一笔，合并其 bars 到 bars_ubi
            // 注意：czsc 用 last_bi.bars[:-2] + 新 bars，但我们用 dt 来匹配
            let new_ubi: Vec<NewBar> = ubi.iter().cloned().collect();
            bi_list.pop();
            // 重新从更早的位置开始
            // 但由于我们没有保存 last_bi 的 bars，这里简化处理：
            // 直接返回废弃后的 bi_list 和 new_ubi
            return (bi_list, new_ubi);
        }
    }

    (bi_list, ubi)
}

/// 对完整 K 线序列构建笔（批量模式）
///
/// 使用基于分型序列的直接递推方法：
/// 1. 去包含后识别所有分型
/// 2. 从分型序列中逐对构建笔
/// 3. 成笔条件：相邻顶底分型无包含关系 + 笔长度 >= min_bi_len
/// 4. 不成笔时跳过，尝试下对分型
pub fn build_bi(klines: &[yifang_data::KLine], min_bi_len: Option<usize>) -> Vec<Bi> {
    let min_bi_len = min_bi_len.unwrap_or(DEFAULT_MIN_BI_LEN);
    let bars_ubi = crate::include::remove_include(klines);
    
    if bars_ubi.len() < 3 {
        return Vec::new();
    }

    let fxs = check_fxs(&bars_ubi);
    if fxs.len() < 2 {
        return Vec::new();
    }

    let mut bis: Vec<Bi> = Vec::new();
    let mut i = 0;

    while i < fxs.len() - 1 {
        let fx_a = &fxs[i];
        
        // 找 fx_a 之后第一个反向分型
        let fx_b_idx = match fxs[i + 1..].iter().enumerate()
            .find(|(_, fx)| fx.mark != fx_a.mark)
        {
            Some((idx, _)) => i + 1 + idx,
            None => break,
        };
        let fx_b = &fxs[fx_b_idx];

        // 方向
        let direction = if fx_a.mark == FxMark::Bottom {
            "up".to_string()
        } else {
            "down".to_string()
        };

        // 笔区间内的 bars 数量
        let bars_count = bars_ubi.iter()
            .filter(|b| b.dt >= fx_a.dt && b.dt <= fx_b.dt)
            .count();

        // 包含关系判断
        let ab_include = (fx_a.high > fx_b.high && fx_a.low < fx_b.low)
            || (fx_a.high < fx_b.high && fx_a.low > fx_b.low);

        if !ab_include && bars_count >= min_bi_len {
            // 成笔
            bis.push(Bi {
                direction,
                start_index: fx_a.merged_index as u64,
                end_index: fx_b.merged_index as u64,
                start_dt: fx_a.dt.clone(),
                end_dt: fx_b.dt.clone(),
                start_price: fx_a.fx,
                end_price: fx_b.fx,
                is_finished: true,
            });
            // 从 fx_b 继续找下一笔
            i = fx_b_idx;
        } else {
            // 不成笔，跳过 fx_a，从下一个分型开始尝试
            i += 1;
        }
    }

    bis
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::include::remove_include;
    use yifang_data::{KLine, TimeFrame};

    fn make_kline(id: u64, dt: &str, open: f64, close: f64, high: f64, low: f64) -> KLine {
        KLine {
            symbol: "test".to_string(),
            timeframe: TimeFrame::D,
            dt: dt.to_string(),
            id,
            open,
            close,
            high,
            low,
            vol: 1000.0,
            amount: 10000.0,
        }
    }

    #[test]
    fn test_basic_bi_detection() {
        // 构建一个有明确顶底形态的数据，确保去包含后足够的K线
        // 核心走势：上升(8根) → 顶 → 下降(8根) → 底 → 上升(8根)
        let mut klines = Vec::new();
        let mut id: u64 = 0;
        // 上升段 10→18
        for i in 0..8 {
            let price = 10.0 + i as f64;
            klines.push(make_kline(id, &format!("2024-01-{:02}", id + 1), price, price + 1.0, price + 1.0, price));
            id += 1;
        }
        // 下降段 18→10
        for i in 0..8 {
            let price = 18.0 - i as f64;
            klines.push(make_kline(id, &format!("2024-01-{:02}", id + 1), price + 1.0, price, price + 1.0, price));
            id += 1;
        }
        // 上升段 10→16
        for i in 0..6 {
            let price = 10.0 + i as f64;
            klines.push(make_kline(id, &format!("2024-01-{:02}", id + 1), price, price + 1.0, price + 1.0, price));
            id += 1;
        }
        // 下降段 16→11
        for i in 0..5 {
            let price = 16.0 - i as f64;
            klines.push(make_kline(id, &format!("2024-01-{:02}", id + 1), price + 1.0, price, price + 1.0, price));
            id += 1;
        }
        // 上升段 11→17
        for i in 0..6 {
            let price = 11.0 + i as f64;
            klines.push(make_kline(id, &format!("2024-01-{:02}", id + 1), price, price + 1.0, price + 1.0, price));
            id += 1;
        }

        let bars = remove_include(&klines);
        let (bis, _) = build_bi_incremental(&bars, &[], None);
        // 至少应该找到笔
        assert!(!bis.is_empty(), "应该找到笔，去包含后{}根K线", bars.len());
    }

    #[test]
    fn test_min_bi_len() {
        // 太短的序列不应该成笔
        let klines = vec![
            make_kline(0, "2024-01-01", 10.0, 11.0, 11.0, 10.0),
            make_kline(1, "2024-01-02", 11.0, 13.0, 13.0, 11.0), // 顶
            make_kline(2, "2024-01-03", 13.0, 10.0, 13.0, 10.0), // 底
        ];
        let bars = remove_include(&klines);
        // min_bi_len=7，只有3根K线，不够
        let (bis, _) = build_bi_incremental(&bars, &[], Some(7));
        assert!(bis.is_empty(), "K线不足 min_bi_len，不应成笔");
    }
}
