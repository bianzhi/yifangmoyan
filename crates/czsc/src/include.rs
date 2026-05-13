//! 去除 K 线包含关系
//!
//! **严格对齐 czsc 0.9.9 的 remove_include 实现**
//!
//! 缠论原文定义：
//! - 向上趋势中（k1.high < k2.high），取高高（高点取大，低点取大）
//! - 向下趋势中（k1.high > k2.high），取低低（高点取小，低点取小）
//! - 方向由 k1 和 k2 的高低关系决定（不是 k2 和 k3）
//!
//! 关键点：方向由 **已确认的两根无包含K线** k1、k2 决定，
//! 处理的是 k2 和 k3 之间的包含关系。

use yifang_data::KLine;

/// 去除包含关系后的合并 K 线
#[derive(Debug, Clone)]
pub struct NewBar {
    /// 原始 K 线中的序号（取合并组第一根的 id）
    pub id: u64,
    /// 时间（取决定高低点的那根 K 线的时间）
    pub dt: String,
    /// 开盘价
    pub open: f64,
    /// 收盘价
    pub close: f64,
    /// 最高价
    pub high: f64,
    /// 最低价
    pub low: f64,
    /// 成交量（合并）
    pub vol: f64,
    /// 成交额（合并）
    pub amount: f64,
    /// 包含的原始 K 线索引列表
    pub elements: Vec<usize>,
}

/// 对 K 线序列去除包含关系，返回合并后的 K 线列表
///
/// 逐根处理：每来一根新 K 线 k3，与已确认的最后两根无包含 K 线 k1、k2 比较：
/// - 方向由 k1、k2 决定（k1.high < k2.high → Up，k1.high > k2.high → Down）
/// - 判断 k2、k3 是否存在包含关系
/// - 存在包含则合并，不存在则 k3 作为新的一根加入
pub fn remove_include(klines: &[KLine]) -> Vec<NewBar> {
    if klines.is_empty() {
        return Vec::new();
    }
    if klines.len() == 1 {
        return vec![NewBar {
            id: 0,
            dt: klines[0].dt.clone(),
            open: klines[0].open,
            close: klines[0].close,
            high: klines[0].high,
            low: klines[0].low,
            vol: klines[0].vol,
            amount: klines[0].amount,
            elements: vec![0],
        }];
    }

    let mut bars_ubi: Vec<NewBar> = Vec::new();

    // 前两根直接加入
    for (i, k) in klines.iter().enumerate().take(2) {
        bars_ubi.push(NewBar {
            id: i as u64,
            dt: k.dt.clone(),
            open: k.open,
            close: k.close,
            high: k.high,
            low: k.low,
            vol: k.vol,
            amount: k.amount,
            elements: vec![i],
        });
    }

    // 从第三根开始逐根处理
    for i in 2..klines.len() {
        let k3 = &klines[i];

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
            let has_include = (k2_high <= k3.high && k2_low >= k3.low)
                || (k2_high >= k3.high && k2_low <= k3.low);

            if has_include {
                // 向上取高高
                let high = k2_high.max(k3.high);
                let low = k2_low.max(k3.low);
                let dt = if k2_high > k3.high {
                    k2_dt.clone()
                } else {
                    k3.dt.clone()
                };
                let (open_, close) = if k3.open > k3.close {
                    (high, low)
                } else {
                    (low, high)
                };
                let vol = k2_vol + k3.vol;
                let amount = k2_amount + k3.amount;

                let mut elements = k2_elements.clone();
                elements.push(i);
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
                bars_ubi.push(NewBar {
                    id: i as u64,
                    dt: k3.dt.clone(),
                    open: k3.open,
                    close: k3.close,
                    high: k3.high,
                    low: k3.low,
                    vol: k3.vol,
                    amount: k3.amount,
                    elements: vec![i],
                });
            }
        } else if k1_high > k2_high {
            // 方向向下
            let has_include = (k2_high <= k3.high && k2_low >= k3.low)
                || (k2_high >= k3.high && k2_low <= k3.low);

            if has_include {
                // 向下取低低
                let high = k2_high.min(k3.high);
                let low = k2_low.min(k3.low);
                let dt = if k2_low < k3.low {
                    k2_dt.clone()
                } else {
                    k3.dt.clone()
                };
                let (open_, close) = if k3.open > k3.close {
                    (high, low)
                } else {
                    (low, high)
                };
                let vol = k2_vol + k3.vol;
                let amount = k2_amount + k3.amount;

                let mut elements = k2_elements.clone();
                elements.push(i);
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
                bars_ubi.push(NewBar {
                    id: i as u64,
                    dt: k3.dt.clone(),
                    open: k3.open,
                    close: k3.close,
                    high: k3.high,
                    low: k3.low,
                    vol: k3.vol,
                    amount: k3.amount,
                    elements: vec![i],
                });
            }
        } else {
            // k1.high == k2.high：无法确定方向，k3 直接作为新 K 线
            bars_ubi.push(NewBar {
                id: i as u64,
                dt: k3.dt.clone(),
                open: k3.open,
                close: k3.close,
                high: k3.high,
                low: k3.low,
                vol: k3.vol,
                amount: k3.amount,
                elements: vec![i],
            });
        }
    }

    bars_ubi
}

#[cfg(test)]
mod tests {
    use super::*;
    use yifang_data::TimeFrame;

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
    fn test_no_include() {
        let klines = vec![
            make_kline(0, "2024-01-01", 10.0, 11.0, 11.5, 9.5),
            make_kline(1, "2024-01-02", 11.0, 12.0, 12.5, 10.5),
            make_kline(2, "2024-01-03", 12.0, 13.0, 13.5, 11.5),
        ];
        let result = remove_include(&klines);
        assert_eq!(result.len(), 3);
    }

    #[test]
    fn test_up_include() {
        let klines = vec![
            make_kline(0, "2024-01-01", 10.0, 11.0, 11.0, 10.0),
            make_kline(1, "2024-01-02", 10.5, 12.0, 12.0, 9.5),
            make_kline(2, "2024-01-03", 11.0, 11.5, 11.5, 10.5),
        ];
        let result = remove_include(&klines);
        assert_eq!(result.len(), 2);
        assert_eq!(result[1].high, 12.0);
        assert_eq!(result[1].low, 10.5);
    }

    #[test]
    fn test_down_include() {
        let klines = vec![
            make_kline(0, "2024-01-01", 13.0, 12.0, 13.0, 11.5),
            make_kline(1, "2024-01-02", 12.0, 10.0, 12.0, 9.5),
            make_kline(2, "2024-01-03", 10.5, 9.8, 11.0, 10.0),
        ];
        let result = remove_include(&klines);
        assert_eq!(result.len(), 2);
        assert_eq!(result[1].high, 11.0);
        assert_eq!(result[1].low, 9.5);
    }

    #[test]
    fn test_equal_highs() {
        let klines = vec![
            make_kline(0, "2024-01-01", 10.0, 12.0, 12.0, 9.5),
            make_kline(1, "2024-01-02", 11.0, 12.0, 12.0, 10.5),
            make_kline(2, "2024-01-03", 11.5, 13.0, 13.0, 11.0),
        ];
        let result = remove_include(&klines);
        assert_eq!(result.len(), 3);
    }
}
