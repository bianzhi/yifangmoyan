//! 威科夫关键形态识别
//!
//! SC (Selling Climax): 卖出高潮
//! AR (Automatic Rally): 自动反弹
//! ST (Secondary Test): 二次测试
//! Spring: 弹簧效应
//! UTAD (Upthrust After Distribution): 派发后冲高
//! JOC (Jump Over Creek): 跳过小溪
//! LPS (Last Point of Support): 最后支撑点
//! SOS (Sign of Strength): 强势信号
//! SOW (Sign of Weakness): 弱势信号
//! ICE (Ice Line): 冰线

use yifang_data::{KLine, WyckoffEvent, TradingRange};

/// 识别威科夫事件
pub fn detect_events(klines: &[KLine]) -> Vec<WyckoffEvent> {
    if klines.len() < 20 {
        return Vec::new();
    }

    let mut events = Vec::new();

    // 计算一些基础统计
    let avg_vol: f64 = klines.iter().map(|k| k.vol).sum::<f64>() / klines.len() as f64;
    let vol_threshold = avg_vol * 1.5; // 放量阈值

    for i in 5..klines.len() - 5 {
        let k = &klines[i];

        // SC (卖出高潮): 大幅下跌 + 巨量
        if k.close < k.open && k.vol > vol_threshold {
            let price_drop = (k.open - k.close) / k.open;
            if price_drop > 0.03 {
                events.push(WyckoffEvent {
                    event_type: "SC".to_string(),
                    index: i as u64,
                    dt: k.dt.clone(),
                    price: k.low,
                    description: "卖出高潮: 放量下跌".to_string(),
                });
            }
        }

        // AR (自动反弹): SC 之后的放量反弹
        if k.close > k.open && k.vol > vol_threshold * 0.8 {
            // 检查前面是否有 SC
            let has_sc_before = events.iter().any(|e| {
                e.event_type == "SC" && (i as u64 - e.index) < 5
            });
            if has_sc_before {
                events.push(WyckoffEvent {
                    event_type: "AR".to_string(),
                    index: i as u64,
                    dt: k.dt.clone(),
                    price: k.high,
                    description: "自动反弹: SC后放量上涨".to_string(),
                });
            }
        }

        // Spring: 价格跌破支撑后快速回升
        if i >= 10 {
            let support = klines[i - 10..i]
                .iter()
                .map(|k| k.low)
                .fold(f64::MAX, f64::min);
            if k.low < support && k.close > support {
                events.push(WyckoffEvent {
                    event_type: "Spring".to_string(),
                    index: i as u64,
                    dt: k.dt.clone(),
                    price: k.low,
                    description: "弹簧效应: 跌破支撑后收回".to_string(),
                });
            }
        }

        // JOC (跳过小溪): 放量突破阻力位
        if i >= 10 && k.close > k.open && k.vol > vol_threshold {
            let resistance = klines[i - 10..i]
                .iter()
                .map(|k| k.high)
                .fold(f64::MIN, f64::max);
            if k.close > resistance {
                events.push(WyckoffEvent {
                    event_type: "JOC".to_string(),
                    index: i as u64,
                    dt: k.dt.clone(),
                    price: k.high,
                    description: "跳过小溪: 放量突破阻力".to_string(),
                });
            }
        }

        // LPS (最后支撑点): 回踩支撑位
        if i >= 15 {
            let recent_highs: Vec<f64> = klines[i - 15..i]
                .windows(3)
                .filter(|w| w[1].high > w[0].high && w[1].high > w[2].high)
                .map(|w| w[1].high)
                .collect();
            if !recent_highs.is_empty() {
                let support_level = recent_highs.iter().cloned().fold(f64::MIN, f64::max);
                let price_near_support = (k.low - support_level).abs() / support_level < 0.02;
                if price_near_support && k.close > k.open {
                    events.push(WyckoffEvent {
                        event_type: "LPS".to_string(),
                        index: i as u64,
                        dt: k.dt.clone(),
                        price: k.low,
                        description: "最后支撑点: 回踩前高获支撑".to_string(),
                    });
                }
            }
        }
    }

    events
}

/// 识别交易区间 (Trading Range)
pub fn detect_trading_ranges(klines: &[KLine]) -> Vec<TradingRange> {
    if klines.len() < 30 {
        return Vec::new();
    }

    let mut ranges = Vec::new();
    let window = 30;
    let step = 10;

    let mut i = 0;
    while i + window <= klines.len() {
        let slice = &klines[i..i + window];
        let high = slice.iter().map(|k| k.high).fold(f64::MIN, f64::max);
        let low = slice.iter().map(|k| k.low).fold(f64::MAX, f64::min);
        let range_pct = (high - low) / low;

        // 横盘区间：波幅较小
        if range_pct < 0.15 {
            let ice_line = (high + low) / 2.0; // 冰线取中位
            ranges.push(TradingRange {
                start_index: i as u64,
                end_index: (i + window - 1) as u64,
                upper: high,
                lower: low,
                ice_line,
            });
        }

        i += step;
    }

    ranges
}
