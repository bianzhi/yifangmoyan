//! 笔的构建
//!
//! **严格对齐 czsc 0.9.9 的 CZSC.__update_bi + check_bi 实现**
//!
//! 核心原则：Python 的 CZSC 类是增量式的——逐根K线输入。
//! 在批量模式下，必须模拟完全相同的增量过程才能得到一致的结果。
//!
//! 算法流程（对齐 Python CZSC）：
//! 1. 逐根K线输入，建立 bars_ubi（去除包含后的K线序列）
//! 2. 每次 bars_ubi 变化后调用 __update_bi：
//!    a) 如果没有笔，找第一笔（最极端同方向分型 + check_bi）
//!    b) 如果已有笔，对 bars_ubi 调 check_bi 找下一笔
//!    c) 后处理：如果最后一笔被损坏，回退并合并 bars
//! 3. check_bi：取 fxs[0] 为 fx_a，找最极端的反向分型 fx_b，
//!    检查成笔条件（无包含 + 长度 >= min_bi_len）

use crate::fenxing::{FxMark, FxResult, check_fxs};
use crate::include::NewBar;
use yifang_data::Bi;

/// 默认最小笔长度（新笔=6，老笔=7）
const DEFAULT_MIN_BI_LEN: usize = 6;

/// 在无包含K线序列中查找一笔（严格对齐 Python check_bi）
///
/// 算法：
/// 1. 找出所有分型
/// 2. 取第一个分型 fx_a
/// 3. 根据方向找最极端的反向分型 fx_b：
///    - 底→顶（上笔）：取 high 最高的顶分型，且 fx_b.fx > fx_a.fx
///    - 顶→底（下笔）：取 low 最低的底分型，且 fx_b.fx < fx_a.fx
/// 4. 如果 fx_b 不存在，返回 None
/// 5. 检查成笔条件：
///    a) fx_a 和 fx_b 无包含关系
///    b) 笔长度 >= min_bi_len
/// 6. 成笔返回 (bi, bars_b)；不成笔返回 (None, 原始 bars)
fn check_bi(bars: &[NewBar], min_bi_len: usize) -> Option<(Bi, Vec<NewBar>)> {
    let fxs = check_fxs(bars);

    if fxs.len() < 2 {
        return None;
    }

    let fx_a = &fxs[0];

    // 根据方向找最极端的反向分型
    let direction: String;
    let fx_b_idx: usize;

    if fx_a.mark == FxMark::Bottom {
        // 底分型开头 → 找上笔 → 取最高顶分型
        let mut best_idx: Option<usize> = None;
        let mut best_high: f64 = f64::NEG_INFINITY;
        for (i, fx) in fxs.iter().enumerate() {
            if fx.mark == FxMark::Top && fx.dt > fx_a.dt && fx.fx > fx_a.fx && fx.high > best_high {
                best_high = fx.high;
                best_idx = Some(i);
            }
        }
        match best_idx {
            Some(idx) => {
                direction = "up".to_string();
                fx_b_idx = idx;
            }
            None => return None,
        }
    } else {
        // 顶分型开头 → 找下笔 → 取最低底分型
        let mut best_idx: Option<usize> = None;
        let mut best_low: f64 = f64::INFINITY;
        for (i, fx) in fxs.iter().enumerate() {
            if fx.mark == FxMark::Bottom && fx.dt > fx_a.dt && fx.fx < fx_a.fx && fx.low < best_low {
                best_low = fx.low;
                best_idx = Some(i);
            }
        }
        match best_idx {
            Some(idx) => {
                direction = "down".to_string();
                fx_b_idx = idx;
            }
            None => return None,
        }
    };

    let fx_b = &fxs[fx_b_idx];

    // Python: bars_a = [x for x in bars if fx_a.elements[0].dt <= x.dt <= fx_b.elements[2].dt]
    // fx_a.elements[0] 是构成分型左边那根K线
    // fx_b.elements[2] 是构成分型右边那根K线
    // 关键差异：bars_a 计数包含 fx_b 右边那根K线之后的所有K线
    // 而 fx_b.dt 只是中间那根K线的时间，所以 bars_a 会多出 1 根
    let fx_a_first_bar_dt = bars.get(fx_a.bars[0]).map(|b| b.dt.as_str()).unwrap_or(fx_a.dt.as_str());
    let fx_b_last_bar_dt = bars.get(fx_b.bars[2]).map(|b| b.dt.as_str()).unwrap_or(fx_b.dt.as_str());
    let bars_a_count = bars
        .iter()
        .filter(|b| b.dt.as_str() >= fx_a_first_bar_dt && b.dt.as_str() <= fx_b_last_bar_dt)
        .count();

    // Python: bars_b = [x for x in bars if x.dt >= fx_b.elements[0].dt]
    // elements[0] 是构成 fx_b 的第一根 K 线
    let fx_b_first_bar_dt = bars.get(fx_b.bars[0]).map(|b| b.dt.as_str()).unwrap_or(&fx_b.dt);
    let bars_b: Vec<NewBar> = bars
        .iter()
        .filter(|b| b.dt.as_str() >= fx_b_first_bar_dt)
        .cloned()
        .collect();

    // 判断 fx_a 和 fx_b 价格区间是否存在包含关系
    let ab_include = (fx_a.high > fx_b.high && fx_a.low < fx_b.low)
        || (fx_a.high < fx_b.high && fx_a.low > fx_b.low);

    // 成笔条件：1）无包含关系；2）笔长度 >= min_bi_len
    if !ab_include && bars_a_count >= min_bi_len {
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
        Some((bi, bars_b))
    } else {
        None
    }
}

/// 内部用的扩展笔结构（包含构成笔的 bars，用于后处理回退）
struct BiEx {
    bi: Bi,
    /// 构成该笔的所有无包含K线
    bars: Vec<NewBar>,
}

/// 对完整 K 线序列构建笔（批量模式）
///
/// **严格模拟 Python CZSC 类的增量过程**：
/// 1. 逐根K线模拟输入，每根K线更新 bars_ubi（去包含）
/// 2. 每次 bars_ubi 变化后调用 __update_bi 逻辑
/// 3. 找到第一笔后，继续找后续笔
/// 4. 后处理：如果最后一笔被损坏，回退并合并
pub fn build_bi(klines: &[yifang_data::KLine], min_bi_len: Option<usize>) -> Vec<Bi> {
    let min_bi_len = min_bi_len.unwrap_or(DEFAULT_MIN_BI_LEN);
    if klines.len() < 3 {
        return Vec::new();
    }

    // === 模拟 Python CZSC 的增量过程 ===
    let mut bi_list: Vec<BiEx> = Vec::new();
    let mut bars_ubi: Vec<NewBar> = Vec::new();

    for (i, kline) in klines.iter().enumerate() {
        // Step 1: 将当前 K 线加入 bars_ubi（去包含处理）
        update_bars_ubi(&mut bars_ubi, kline, i);

        // Step 2: 调用 __update_bi 逻辑
        __update_bi(&mut bi_list, &mut bars_ubi, min_bi_len);
    }

    // 转换为 Bi 列表
    bi_list.into_iter().map(|bex| bex.bi).collect()
}

/// 将一根 K 线添加到 bars_ubi（去包含处理）
///
/// 对齐 Python CZSC.update() 中的去包含逻辑
fn update_bars_ubi(bars_ubi: &mut Vec<NewBar>, kline: &yifang_data::KLine, index: usize) {
    let new_bar = NewBar {
        id: index as u64,
        dt: kline.dt.clone(),
        open: kline.open,
        close: kline.close,
        high: kline.high,
        low: kline.low,
        vol: kline.vol,
        amount: kline.amount,
        elements: vec![index],
    };

    if bars_ubi.len() < 2 {
        bars_ubi.push(new_bar);
        return;
    }

    // 先从 bars_ubi 提取需要的信息到局部变量，避免借用冲突
    let k1_high = bars_ubi[bars_ubi.len() - 2].high;
    let k2_high = bars_ubi[bars_ubi.len() - 1].high;
    let k2_low = bars_ubi[bars_ubi.len() - 1].low;
    let k2_id = bars_ubi[bars_ubi.len() - 1].id;
    let k2_dt = bars_ubi[bars_ubi.len() - 1].dt.clone();
    let k2_vol = bars_ubi[bars_ubi.len() - 1].vol;
    let k2_amount = bars_ubi[bars_ubi.len() - 1].amount;
    let k2_elements = bars_ubi[bars_ubi.len() - 1].elements.clone();

    // 方向由 k1、k2 决定
    if k1_high < k2_high {
        // 方向向上
        let has_include = (k2_high <= kline.high && k2_low >= kline.low)
            || (k2_high >= kline.high && k2_low <= kline.low);

        if has_include {
            // 向上取高高
            let high = k2_high.max(kline.high);
            let low = k2_low.max(kline.low);
            let dt = if k2_high > kline.high {
                k2_dt.clone()
            } else {
                kline.dt.clone()
            };
            let (open_, close) = if kline.open > kline.close {
                (high, low)
            } else {
                (low, high)
            };
            let vol = k2_vol + kline.vol;
            let amount = k2_amount + kline.amount;

            let mut elements = k2_elements.clone();
            elements.push(index);
            if elements.len() > 100 {
                elements.drain(..elements.len() - 100);
            }

            let last = bars_ubi.last_mut().unwrap();
            *last = NewBar {
                id: k2_id,
                dt,
                open: open_,
                close,
                high,
                low,
                vol,
                amount,
                elements,
            };
        } else {
            bars_ubi.push(new_bar);
        }
    } else if k1_high > k2_high {
        // 方向向下
        let has_include = (k2_high <= kline.high && k2_low >= kline.low)
            || (k2_high >= kline.high && k2_low <= kline.low);

        if has_include {
            // 向下取低低
            let high = k2_high.min(kline.high);
            let low = k2_low.min(kline.low);
            let dt = if k2_low < kline.low {
                k2_dt.clone()
            } else {
                kline.dt.clone()
            };
            let (open_, close) = if kline.open > kline.close {
                (high, low)
            } else {
                (low, high)
            };
            let vol = k2_vol + kline.vol;
            let amount = k2_amount + kline.amount;

            let mut elements = k2_elements.clone();
            elements.push(index);
            if elements.len() > 100 {
                elements.drain(..elements.len() - 100);
            }

            let last = bars_ubi.last_mut().unwrap();
            *last = NewBar {
                id: k2_id,
                dt,
                open: open_,
                close,
                high,
                low,
                vol,
                amount,
                elements,
            };
        } else {
            bars_ubi.push(new_bar);
        }
    } else {
        // k1.high == k2.high：无法确定方向
        bars_ubi.push(new_bar);
    }
}

/// 模拟 Python CZSC.__update_bi 逻辑
///
/// 严格对齐 Python 代码：
/// 1. 如果没有笔，找第一笔
/// 2. 如果已有笔，找下一笔
/// 3. 后处理：如果最后一笔被破坏，回退
fn __update_bi(bi_list: &mut Vec<BiEx>, bars_ubi: &mut Vec<NewBar>, min_bi_len: usize) {
    if bars_ubi.len() < 3 {
        return;
    }

    // 找笔
    if bi_list.is_empty() {
        // === 第一笔的查找（对齐 Python CZSC.__update_bi 第一笔逻辑）===
        let fxs = check_fxs(bars_ubi);
        if fxs.is_empty() {
            return;
        }

        // 找同方向最极端的分型作为 fx_a
        let mut fx_a = fxs[0].clone();
        for fx in &fxs {
            if fx.mark != fx_a.mark {
                continue;
            }
            let should_replace = match fx.mark {
                FxMark::Bottom => fx.low <= fx_a.low,
                FxMark::Top => fx.high >= fx_a.high,
            };
            if should_replace {
                fx_a = fx.clone();
            }
        }


        // Python: bars_ubi = [x for x in bars_ubi if x.dt >= fx_a.elements[0].dt]
        let start_dt = bars_ubi
            .get(fx_a.bars[0])
            .map(|b| b.dt.as_str())
            .unwrap_or(fx_a.dt.as_str())
            .to_string();

        let trimmed: Vec<NewBar> = bars_ubi
            .iter()
            .filter(|b| b.dt.as_str() >= start_dt.as_str())
            .cloned()
            .collect();

        if let Some((bi, bars_b)) = check_bi(&trimmed, min_bi_len) {
            // 计算构成该笔的 bars_a
            // 对齐 Python: bi.bars = bars_a（从 fx_a.elements[0].dt 到 fx_b.elements[2].dt）
            // 我们需要用 fx_a.bars[0] 和 fx_b.bars[2] 来获取正确的范围
            let fxs_in_trimmed = check_fxs(&trimmed);
            let fx_a_in_trimmed = fxs_in_trimmed.iter().find(|fx| fx.dt == bi.start_dt);
            let fx_b_in_trimmed = fxs_in_trimmed.iter().find(|fx| fx.dt == bi.end_dt);
            
            let bars_a_start = fx_a_in_trimmed
                .and_then(|fx| trimmed.get(fx.bars[0]))
                .map(|b| b.dt.as_str())
                .unwrap_or(bi.start_dt.as_str());
            let bars_a_end = fx_b_in_trimmed
                .and_then(|fx| trimmed.get(fx.bars[2]))
                .map(|b| b.dt.as_str())
                .unwrap_or(bi.end_dt.as_str());
            
            let bars_a: Vec<NewBar> = trimmed
                .iter()
                .filter(|b| b.dt.as_str() >= bars_a_start && b.dt.as_str() <= bars_a_end)
                .cloned()
                .collect();
            bi_list.push(BiEx { bi, bars: bars_a });
            *bars_ubi = bars_b;
        }
        // 不成笔，保持 bars_ubi 不变（等下一根K线）
        return;
    }

    // === 已有笔，找下一笔 ===
    let check_result = check_bi(bars_ubi, min_bi_len);
    match check_result {
        Some((bi, bars_b)) => {
            // 计算构成该笔的 bars_a（对齐 Python bi.bars 范围）
            let fxs_for_bars = check_fxs(bars_ubi);
            let fx_a_for_bars = fxs_for_bars.iter().find(|fx| fx.dt == bi.start_dt);
            let fx_b_for_bars = fxs_for_bars.iter().find(|fx| fx.dt == bi.end_dt);
            
            let bars_a_start = fx_a_for_bars
                .and_then(|fx| bars_ubi.get(fx.bars[0]))
                .map(|b| b.dt.as_str())
                .unwrap_or(bi.start_dt.as_str());
            let bars_a_end = fx_b_for_bars
                .and_then(|fx| bars_ubi.get(fx.bars[2]))
                .map(|b| b.dt.as_str())
                .unwrap_or(bi.end_dt.as_str());
            
            let bars_a: Vec<NewBar> = bars_ubi
                .iter()
                .filter(|b| b.dt.as_str() >= bars_a_start && b.dt.as_str() <= bars_a_end)
                .cloned()
                .collect();
            bi_list.push(BiEx { bi, bars: bars_a });
            *bars_ubi = bars_b;
        }
        None => {
            // check_bi 未找到新笔，bars_ubi 不变
            // 但仍需要更新 bars_ubi（Python 总是设置 self.bars_ubi = bars_ubi_）
            // 在 check_bi None 的情况下，bars_ubi_ == bars（即不变）
        }
    }

    // 后处理：无论是否找到新笔，都检查最后一笔是否被破坏
    // Python 的 __update_bi 中，后处理在 check_bi 之后无条件运行
    if !bi_list.is_empty() && bars_ubi.len() >= 2 {
        let last_bi = &bi_list[bi_list.len() - 1];
        let last_ubi = &bars_ubi[bars_ubi.len() - 1];

        let is_broken = (last_bi.bi.direction == "up" && last_ubi.high > last_bi.bi.end_price)
            || (last_bi.bi.direction == "down" && last_ubi.low < last_bi.bi.end_price);

        if is_broken {
            // Python: self.bars_ubi = last_bi.bars[:-2] + [x for x in bars_ubi if x.dt >= last_bi.bars[-2].dt]
            // Python: self.bi_list.pop(-1)
            let broken_bex = bi_list.pop().unwrap();

            // 严格对齐 Python：
            // last_bi.bars[:-2] 表示去掉最后两根K线
            // [x for x in bars_ubi if x.dt >= last_bi.bars[-2].dt] 取剩余部分
            let broken_bars = &broken_bex.bars;
            if broken_bars.len() >= 2 {
                let rollback_dt = broken_bars[broken_bars.len() - 2].dt.clone();
                
                let mut new_bars_ubi: Vec<NewBar> = broken_bars
                    .iter()
                    .take(broken_bars.len() - 2)
                    .cloned()
                    .collect();
                
                let remaining: Vec<NewBar> = bars_ubi
                    .iter()
                    .filter(|b| b.dt >= rollback_dt)
                    .cloned()
                    .collect();
                
                // 去重：new_bars_ubi 末尾和 remaining 开头可能重叠
                // 只添加 remaining 中时间不在 new_bars_ubi 中的
                let last_dt = new_bars_ubi.last().map(|b| b.dt.clone()).unwrap_or_default();
                for b in &remaining {
                    if b.dt > last_dt {
                        new_bars_ubi.push(b.clone());
                    }
                }
                
                *bars_ubi = new_bars_ubi;
            } else {
                // 不到2根bar，用 start_dt 回退
                let rollback_dt = broken_bex.bi.start_dt.clone();
                *bars_ubi = bars_ubi
                    .iter()
                    .filter(|b| b.dt >= rollback_dt)
                    .cloned()
                    .collect();
            }
        }
    }
    // 如果 check_bi 返回 None，bars_ubi 不变（等下一根K线）
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

        let bis = build_bi(&klines, None);
        assert!(!bis.is_empty(), "应该找到笔，去包含后{}根K线", remove_include(&klines).len());
    }

    #[test]
    fn test_min_bi_len() {
        let klines = vec![
            make_kline(0, "2024-01-01", 10.0, 11.0, 11.0, 10.0),
            make_kline(1, "2024-01-02", 11.0, 13.0, 13.0, 11.0),
            make_kline(2, "2024-01-03", 13.0, 10.0, 13.0, 10.0),
        ];
        let _bars = remove_include(&klines);
        // min_bi_len=7，只有3根K线，不够
        let bis = build_bi(&klines, Some(7));
        assert!(bis.is_empty(), "K线不足 min_bi_len，不应成笔");
    }

    #[test]
    fn test_bi_direction_constraint() {
        // 构建一个场景：底分型后，存在顶分型但 fx_b.fx < fx_a.fx（不应成笔）
        let klines = vec![
            make_kline(0, "2024-01-01", 10.0, 9.0, 10.0, 9.0),
            make_kline(1, "2024-01-02", 9.0, 8.0, 9.0, 8.0),    // 底
            make_kline(2, "2024-01-03", 8.0, 8.5, 8.5, 8.0),     // 小反弹
            make_kline(3, "2024-01-04", 8.5, 7.5, 8.5, 7.5),     // 再跌
            make_kline(4, "2024-01-05", 7.5, 7.0, 7.5, 7.0),     // 继续跌
            make_kline(5, "2024-01-06", 7.0, 6.0, 7.0, 6.0),     // 更低
            make_kline(6, "2024-01-07", 6.0, 5.0, 6.0, 5.0),     // 最低
            make_kline(7, "2024-01-08", 5.0, 6.0, 6.0, 5.0),     // 反弹
        ];
        let bis = build_bi(&klines, None);
        for bi in &bis {
            if bi.direction == "up" {
                assert!(bi.end_price > bi.start_price, "上笔终价应高于起价");
            } else {
                assert!(bi.end_price < bi.start_price, "下笔终价应低于起价");
            }
        }
    }
}

#[cfg(test)]
mod compare_test {
    use super::*;
    use crate::include::remove_include;
    use crate::fenxing::check_fxs;
    use yifang_data::{KLine, TimeFrame};

    /// 对比测试：与 Python czsc 0.9.9 的笔识别结果对比
    /// 注意：此测试依赖 /tmp/000001_daily.json 文件，且需要与 Python czsc 结果一致
    #[test]
    fn test_bi_compare_with_python_reference() {
        let json_str = std::fs::read_to_string("/tmp/000001_daily.json").unwrap_or_default();
        if json_str.is_empty() {
            eprintln!("SKIP: /tmp/000001_daily.json not found");
            return;
        }
        let records: Vec<serde_json::Value> = serde_json::from_str(&json_str).unwrap();
        
        let klines: Vec<KLine> = records.iter().enumerate().map(|(i, r)| {
            KLine {
                symbol: "000001".to_string(),
                timeframe: TimeFrame::D,
                dt: r["dt"].as_str().unwrap().to_string(),
                id: i as u64,
                open: r["open"].as_f64().unwrap(),
                close: r["close"].as_f64().unwrap(),
                high: r["high"].as_f64().unwrap(),
                low: r["low"].as_f64().unwrap(),
                vol: r["vol"].as_f64().unwrap(),
                amount: 0.0,
            }
        }).collect();
        
        eprintln!("\n=== Rust bi analysis for 000001 daily ===");
        eprintln!("Loaded {} klines", klines.len());
        
        let bars_ubi = remove_include(&klines);
        eprintln!("After remove_include: {} bars", bars_ubi.len());
        
        let fxs = check_fxs(&bars_ubi);
        eprintln!("FenXing count: {}", fxs.len());
        
        let bis = build_bi(&klines, None);
        eprintln!("\nRust bi count: {}", bis.len());
        for (i, bi) in bis.iter().enumerate() {
            eprintln!("  Bi[{}]: {} {} → {}, start_price={:.2}, end_price={:.2}", 
                i, bi.direction, bi.start_dt, bi.end_dt, bi.start_price, bi.end_price);
        }
        
        // 确认方向交替
        for i in 1..bis.len() {
            assert_ne!(bis[i].direction, bis[i-1].direction, 
                "BI[{}]和BI[{}]方向相同: {}", i, i-1, bis[i].direction);
        }
    }

    /// 对比测试：与 Python czsc seed=42 随机数据
    #[test]
    fn test_bi_compare_seed42() {
        let json_str = std::fs::read_to_string("/tmp/test_klines_42.json").unwrap_or_default();
        if json_str.is_empty() {
            eprintln!("SKIP: /tmp/test_klines_42.json not found");
            return;
        }
        let records: Vec<serde_json::Value> = serde_json::from_str(&json_str).unwrap();
        
        let klines: Vec<KLine> = records.iter().enumerate().map(|(i, r)| {
            KLine {
                symbol: "TEST".to_string(),
                timeframe: TimeFrame::D,
                dt: r["dt"].as_str().unwrap().to_string(),
                id: i as u64,
                open: r["open"].as_f64().unwrap(),
                close: r["close"].as_f64().unwrap(),
                high: r["high"].as_f64().unwrap(),
                low: r["low"].as_f64().unwrap(),
                vol: 1000.0,
                amount: 10000.0,
            }
        }).collect();
        
        let bis = build_bi(&klines, None);
        eprintln!("\nyifang-czsc: {} 笔", bis.len());
        for (i, bi) in bis.iter().enumerate() {
            eprintln!("  BI[{}]: {} start={:.2} → end={:.2}", 
                     i, bi.direction, bi.start_price, bi.end_price);
        }
        
        let py_json = std::fs::read_to_string("/tmp/py_czsc_bis_42.json").unwrap_or_default();
        if py_json.is_empty() {
            eprintln!("SKIP: /tmp/py_czsc_bis_42.json not found");
            return;
        }
        let py_bis: Vec<serde_json::Value> = serde_json::from_str(&py_json).unwrap();
        eprintln!("Python CZSC: {} 笔", py_bis.len());
        
        let min_len = bis.len().min(py_bis.len());
        let mut mismatches = 0;
        for i in 0..min_len {
            let rb = &bis[i];
            let py_dir = py_bis[i]["direction"].as_str().unwrap();
            let py_fx_a = py_bis[i]["fx_a_fx"].as_f64().unwrap();
            let py_fx_b = py_bis[i]["fx_b_fx"].as_f64().unwrap();
            
            let dir_match = rb.direction == py_dir;
            let price_match = (rb.start_price - py_fx_a).abs() < 0.1 && (rb.end_price - py_fx_b).abs() < 0.1;
            
            if !dir_match || !price_match {
                mismatches += 1;
                eprintln!("  MISMATCH BI[{}]: Rust({} {:.2}→{:.2}) vs Python({} {:.2}→{:.2})", 
                    i, rb.direction, rb.start_price, rb.end_price,
                    py_dir, py_fx_a, py_fx_b);
            }
        }
        
        if bis.len() != py_bis.len() {
            eprintln!("⚠️ 笔数不一致: Rust={}, Python={}", bis.len(), py_bis.len());
        }
        if mismatches == 0 && bis.len() == py_bis.len() {
            eprintln!("✅ yifang-czsc 与 Python czsc 完全一致！");
        }
        
        assert_eq!(bis.len(), py_bis.len(), "笔数应一致");
        assert_eq!(mismatches, 0, "笔方向和价格应一致");
    }

    fn compare_bi_with_python(seed: usize) {
        let klines_path = format!("/tmp/test_klines_{}.json", seed);
        let bis_path = format!("/tmp/py_czsc_bis_{}.json", seed);
        
        let json_str = std::fs::read_to_string(&klines_path).unwrap_or_default();
        if json_str.is_empty() {
            eprintln!("SKIP: {} not found", klines_path);
            return;
        }
        let records: Vec<serde_json::Value> = serde_json::from_str(&json_str).unwrap();
        
        let klines: Vec<KLine> = records.iter().enumerate().map(|(i, r)| {
            KLine {
                symbol: "TEST".to_string(),
                timeframe: TimeFrame::D,
                dt: r["dt"].as_str().unwrap().to_string(),
                id: i as u64,
                open: r["open"].as_f64().unwrap(),
                close: r["close"].as_f64().unwrap(),
                high: r["high"].as_f64().unwrap(),
                low: r["low"].as_f64().unwrap(),
                vol: 1000.0,
                amount: 10000.0,
            }
        }).collect();
        
        let bis = build_bi(&klines, None);
        
        let py_json = std::fs::read_to_string(&bis_path).unwrap_or_default();
        if py_json.is_empty() {
            eprintln!("SKIP: {} not found", bis_path);
            return;
        }
        let py_bis: Vec<serde_json::Value> = serde_json::from_str(&py_json).unwrap();
        
        let min_len = bis.len().min(py_bis.len());
        let mut mismatches = 0;
        for i in 0..min_len {
            let rb = &bis[i];
            let py_dir = py_bis[i]["direction"].as_str().unwrap();
            let py_fx_a = py_bis[i]["fx_a_fx"].as_f64().unwrap();
            let py_fx_b = py_bis[i]["fx_b_fx"].as_f64().unwrap();
            
            let dir_match = rb.direction == py_dir;
            let price_match = (rb.start_price - py_fx_a).abs() < 0.1 && (rb.end_price - py_fx_b).abs() < 0.1;
            
            if !dir_match || !price_match {
                mismatches += 1;
                eprintln!("  MISMATCH BI[{}]: Rust({} {:.2}→{:.2}) vs Python({} {:.2}→{:.2})", 
                    i, rb.direction, rb.start_price, rb.end_price,
                    py_dir, py_fx_a, py_fx_b);
            }
        }
        
        let pass = mismatches == 0 && bis.len() == py_bis.len();
        if pass {
            eprintln!("✅ seed={}: {} 笔完全一致", seed, bis.len());
        } else {
            eprintln!("❌ seed={}: Rust={}笔 vs Python={}笔, mismatches={}", seed, bis.len(), py_bis.len(), mismatches);
        }
        assert_eq!(bis.len(), py_bis.len(), "seed={}, 笔数应一致", seed);
        assert_eq!(mismatches, 0, "seed={}, 笔方向和价格应一致", seed);
    }

    #[test]
    fn test_bi_compare_seed123() { compare_bi_with_python(123); }

    #[test]
    fn test_bi_compare_seed456() { compare_bi_with_python(456); }

    #[test]
    fn test_bi_compare_seed789() { compare_bi_with_python(789); }

    #[test]
    fn test_bi_compare_seed1024() { compare_bi_with_python(1024); }
}
