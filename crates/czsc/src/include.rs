//! 去除 K 线包含关系
//!
//! 参考 czsc 的 remove_include 逻辑：
//! - 向上趋势中，取高高（高点取大，低点取大）
//! - 向下趋势中，取低低（高点取小，低点取小）

use yifang_data::KLine;

/// 去除包含关系后的合并 K 线
#[derive(Debug, Clone)]
pub struct MergedBar {
    /// 序号
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

/// 合并方向
#[derive(Debug, Clone, Copy, PartialEq)]
enum Direction {
    Up,
    Down,
}

/// 对 K 线序列去除包含关系，返回合并后的 K 线列表
pub fn remove_include(klines: &[KLine]) -> Vec<MergedBar> {
    if klines.is_empty() {
        return Vec::new();
    }
    if klines.len() == 1 {
        return vec![MergedBar {
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

    let mut merged = Vec::new();
    let mut direction = Direction::Up; // 初始方向

    // 第一根 K 线直接加入
    merged.push(MergedBar {
        id: 0,
        dt: klines[0].dt.clone(),
        open: klines[0].open,
        close: klines[0].close,
        high: klines[0].high,
        low: klines[0].low,
        vol: klines[0].vol,
        amount: klines[0].amount,
        elements: vec![0],
    });

    for i in 1..klines.len() {
        let curr = &klines[i];

        // 先从 merged 末尾提取所需数据，避免借用冲突
        let prev_high = merged.last().unwrap().high;
        let prev_low = merged.last().unwrap().low;
        let prev_vol = merged.last().unwrap().vol;
        let prev_amount = merged.last().unwrap().amount;
        let prev_dt = merged.last().unwrap().dt.clone();
        let prev_id = merged.last().unwrap().id;

        // 确定方向：如果当前高点 > 前一根高点，方向向上；否则向下
        if curr.high > prev_high {
            direction = Direction::Up;
        } else if curr.high < prev_high {
            direction = Direction::Down;
        }
        // 如果高点相等，保持当前方向

        // 判断是否包含关系
        let is_contain = (prev_high >= curr.high && prev_low <= curr.low)
            || (prev_high <= curr.high && prev_low >= curr.low);

        if is_contain {
            // 合并处理
            let (new_high, new_low, new_dt) = match direction {
                Direction::Up => {
                    let high = prev_high.max(curr.high);
                    let low = prev_low.max(curr.low);
                    let dt = if prev_high > curr.high {
                        prev_dt.clone()
                    } else {
                        curr.dt.clone()
                    };
                    (high, low, dt)
                }
                Direction::Down => {
                    let high = prev_high.min(curr.high);
                    let low = prev_low.min(curr.low);
                    let dt = if prev_low < curr.low {
                        prev_dt.clone()
                    } else {
                        curr.dt.clone()
                    };
                    (high, low, dt)
                }
            };

            let open = if curr.open > curr.close {
                new_high
            } else {
                new_low
            };
            let close = if curr.open > curr.close {
                new_low
            } else {
                new_high
            };

            let total_vol = prev_vol + curr.vol;
            let total_amount = prev_amount + curr.amount;

            // 更新最后一根合并 K 线
            let last = merged.last_mut().unwrap();
            let mut elements = std::mem::take(&mut last.elements);
            elements.push(i);
            // 限制 elements 长度，防止极端情况
            if elements.len() > 100 {
                elements.drain(..elements.len() - 100);
            }

            *last = MergedBar {
                id: prev_id,
                dt: new_dt,
                open,
                close,
                high: new_high,
                low: new_low,
                vol: total_vol,
                amount: total_amount,
                elements,
            };
        } else {
            // 无包含关系，新增合并 K 线
            merged.push(MergedBar {
                id: i as u64,
                dt: curr.dt.clone(),
                open: curr.open,
                close: curr.close,
                high: curr.high,
                low: curr.low,
                vol: curr.vol,
                amount: curr.amount,
                elements: vec![i],
            });
        }
    }

    merged
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
        // 上升序列，无包含关系
        let klines = vec![
            make_kline(0, "2024-01-01", 10.0, 11.0, 11.5, 9.5),
            make_kline(1, "2024-01-02", 11.0, 12.0, 12.5, 10.5),
            make_kline(2, "2024-01-03", 12.0, 13.0, 13.5, 11.5),
        ];
        let result = remove_include(&klines);
        assert_eq!(result.len(), 3);
    }

    #[test]
    fn test_with_include() {
        // 第二根包含第一根
        let klines = vec![
            make_kline(0, "2024-01-01", 10.0, 11.0, 11.0, 10.0),
            make_kline(1, "2024-01-02", 10.5, 12.0, 12.0, 9.5), // 包含第一根
            make_kline(2, "2024-01-03", 12.0, 13.0, 13.0, 11.5),
        ];
        let result = remove_include(&klines);
        // 第一根和第二根合并
        assert_eq!(result.len(), 2);
        assert!(result[0].elements.len() == 2);
    }
}
