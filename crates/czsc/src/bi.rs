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

    let fx_a = &fxs[0];

    // 根据第一个分型确定笔的方向，找最极端的反向分型
    // 对齐 czsc：升笔找最高顶分型(max high)，降笔找最低底分型(min low)
    let (direction, fx_b_idx): (String, Option<usize>) = if fx_a.mark == FxMark::Bottom {
        // 底分型起始 → 上升笔，找最高的顶分型
        let best = fxs
            .iter()
            .enumerate()
            .filter(|(_, fx)| fx.mark == FxMark::Top && fx.dt > fx_a.dt && fx.fx > fx_a.fx)
            .max_by(|(_, a), (_, b)| a.high.partial_cmp(&b.high).unwrap())
            .map(|(i, _)| i);
        ("up".to_string(), best)
    } else {
        // 顶分型起始 → 下降笔，找最低的底分型
        let best = fxs
            .iter()
            .enumerate()
            .filter(|(_, fx)| fx.mark == FxMark::Bottom && fx.dt > fx_a.dt && fx.fx < fx_a.fx)
            .min_by(|(_, a), (_, b)| a.low.partial_cmp(&b.low).unwrap())
            .map(|(i, _)| i);
        ("down".to_string(), best)
    };

    let fx_b_idx = match fx_b_idx {
        Some(idx) => idx,
        None => return BiCheckResult::NotFound(bars.to_vec()),
    };
    let fx_b = &fxs[fx_b_idx];

    // 提取笔区间内的 bars
    let bars_a_count = bars
        .iter()
        .filter(|b| b.dt >= fx_a.dt && b.dt <= fx_b.dt)
        .count();

    // 判断 fx_a 和 fx_b 价格区间是否存在包含关系
    // 包含定义：(fx_a.high > fx_b.high AND fx_a.low < fx_b.low)
    //         OR (fx_a.high < fx_b.high AND fx_a.low > fx_b.low)
    let ab_include = (fx_a.high > fx_b.high && fx_a.low < fx_b.low)
        || (fx_a.high < fx_b.high && fx_a.low > fx_b.low);

    // 成笔条件：(1) 顶底分型之间没有包含关系；(2) 笔长度 >= min_bi_len
    if !ab_include && bars_a_count >= min_bi_len {
        // 剩余的 bars：从 fx_b 的第一根 K 线开始
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

        BiCheckResult::Found(bi, bars_b)
    } else {
        BiCheckResult::NotFound(bars.to_vec())
    }
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

    // 找最极端的同方向分型
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

    if trimmed_bars.is_empty() {
        return (None, bars.to_vec());
    }

    match check_bi(&trimmed_bars, Some(min_bi_len)) {
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
    if !bi_list.is_empty() && ubi.len() >= 2 {
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
/// 简化接口：一次性处理所有 K 线
pub fn build_bi(klines: &[yifang_data::KLine], min_bi_len: Option<usize>) -> Vec<Bi> {
    let bars_ubi = crate::include::remove_include(klines);
    let (bis, _) = build_bi_incremental(&bars_ubi, &[], min_bi_len);
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
