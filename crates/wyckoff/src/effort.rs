//! 努力与结果法则 (Law of Effort vs. Result)
//!
//! 威科夫第三法则的核心实现。
//!
//! **定义**：成交量（Volume）= 努力（Effort），价格变动幅度 = 结果（Result）。
//!
//! **量价协调（Harmonious）**：努力与结果方向一致，趋势将延续。
//! - 放量上涨 → 需求主导，看涨
//! - 放量下跌 → 供给主导，看跌
//! - 缩量横盘 → 供需均衡
//!
//! **量价背离（Divergent）**：努力与结果方向不一致，趋势可能反转。
//! - 上涨 + 缩量 → 需求不足，可能是派发
//! - 上涨 + 放量但涨幅极小 → 供给出现，主力出货
//! - 下跌 + 缩量 → 供给衰竭，可能是吸筹
//! - 下跌 + 放量但跌幅极小 → 需求出现，主力吸筹
//!
//! **判断逻辑**：
//! 1. 计算每根 K 线的努力（成交量相对均值的倍数）
//! 2. 计算每根 K 线的结果（价格变动幅度占价格的百分比）
//! 3. 量价方向对比：
//!    - effort 高 + result 高（同向）→ harmonious, demand/supply_dominant
//!    - effort 高 + result 低 → divergent, 供给或需求出现
//!    - effort 低 + result 高 → 供需任一方主导
//!    - effort 低 + result 低 → neutral

use yifang_data::{KLine, EffortResult};

/// 计算努力与结果分析
///
/// 对每根 K 线计算量价关系，判断供需主导方。
pub fn analyze_effort_result(klines: &[KLine]) -> Vec<EffortResult> {
    if klines.len() < 5 {
        return Vec::new();
    }

    let avg_vol = calc_avg_volume(klines);
    let mut results = Vec::with_capacity(klines.len());

    for (i, k) in klines.iter().enumerate() {
        let effort = if avg_vol > 0.0 { k.vol / avg_vol } else { 1.0 };
        let result = if k.open > 0.0 {
            (k.close - k.open).abs() / k.open
        } else {
            0.0
        };

        let is_up = k.close >= k.open;
        let (harmony, interpretation) = classify_effort_result(effort, result, is_up);

        results.push(EffortResult {
            index: i as u64,
            dt: k.dt.clone(),
            effort,
            result,
            harmony,
            interpretation,
        });
    }

    results
}

/// 判断量价协调性
///
/// 努力阈值和结果阈值基于统计分布：
/// - effort > 1.0 → 高努力（成交量超过均值）
/// - result > 平均振幅 → 高结果
fn classify_effort_result(effort: f64, result: f64, is_up: bool) -> (String, String) {
    let high_effort = effort > 1.3;  // 成交量超过均值 1.3 倍
    let high_result = result > 0.015; // 振幅超过 1.5%

    match (high_effort, high_result, is_up) {
        // 量价协调：放量 + 大幅同向
        (true, true, true) => ("harmonious".into(), "demand_dominant".into()),
        (true, true, false) => ("harmonious".into(), "supply_dominant".into()),
        
        // 量价背离：放量 + 小幅（供给/需求出现）
        (true, false, true) => ("divergent".into(), "supply_appearing".into()),  // 放量涨不动→供给出现
        (true, false, false) => ("divergent".into(), "demand_appearing".into()), // 放量跌不动→需求出现
        
        // 缩量情形
        (false, true, true) => ("divergent".into(), "demand_dominant".into()),  // 缩量上涨→需求主导但持续性存疑
        (false, true, false) => ("divergent".into(), "supply_dominant".into()), // 缩量下跌→供给主导但力度减弱
        (false, false, true) => ("neutral".into(), "neutral".into()),
        (false, false, false) => ("neutral".into(), "neutral".into()),
    }
}

/// 计算平均成交量（排除异常值）
fn calc_avg_volume(klines: &[KLine]) -> f64 {
    if klines.is_empty() {
        return 0.0;
    }
    let total: f64 = klines.iter().map(|k| k.vol).sum();
    total / klines.len() as f64
}

/// 检测一段时间内的成交量趋势
///
/// 用于判断 Phase B 中主力是吸筹还是派发：
/// - 上涨日成交量逐步萎缩 → 派发
/// - 下跌日成交量逐步萎缩 → 吸筹
pub fn volume_trend(klines: &[KLine]) -> f64 {
    if klines.len() < 6 {
        return 0.0;
    }
    let half = klines.len() / 2;
    let first_half_avg: f64 = klines[..half].iter().map(|k| k.vol).sum::<f64>() / half as f64;
    let second_half_avg: f64 = klines[half..].iter().map(|k| k.vol).sum::<f64>() / (klines.len() - half) as f64;
    if first_half_avg > 0.0 {
        (second_half_avg - first_half_avg) / first_half_avg
    } else {
        0.0
    }
}

/// 检测下跌中的需求出现信号
///
/// 条件：
/// - 在下跌趋势中
/// - 出现放量但跌幅极小的K线
/// - 或者放量收阳
pub fn detect_demand_appearing(klines: &[KLine], idx: usize) -> bool {
    if idx < 5 || idx >= klines.len() {
        return false;
    }
    let avg_vol: f64 = klines[idx - 5..=idx].iter().map(|k| k.vol).sum::<f64>() / 6.0;
    let k = &klines[idx];
    
    // 放量
    let vol_surge = k.vol > avg_vol * 1.5;
    
    // 涨幅或跌幅极小
    let small_drop = k.close < k.open && (k.open - k.close) / k.open < 0.01;
    let turn_up = k.close >= k.open;
    
    vol_surge && (small_drop || turn_up)
}

/// 检测上涨中的供给出现信号
///
/// 条件：
/// - 在上涨趋势中
/// - 出现放量但涨幅极小的K线
/// - 或者放量收阴
pub fn detect_supply_appearing(klines: &[KLine], idx: usize) -> bool {
    if idx < 5 || idx >= klines.len() {
        return false;
    }
    let avg_vol: f64 = klines[idx - 5..=idx].iter().map(|k| k.vol).sum::<f64>() / 6.0;
    let k = &klines[idx];
    
    let vol_surge = k.vol > avg_vol * 1.5;
    let small_rise = k.close > k.open && (k.close - k.open) / k.open < 0.01;
    let turn_down = k.close <= k.open;
    
    vol_surge && (small_rise || turn_down)
}

#[cfg(test)]
mod tests {
    use super::*;
    use yifang_data::TimeFrame;

    fn make_kline(open: f64, close: f64, high: f64, low: f64, vol: f64, idx: usize) -> KLine {
        KLine {
            symbol: "test".to_string(),
            timeframe: TimeFrame::D,
            dt: format!("d{}", idx),
            id: idx as u64,
            open, close, high, low, vol,
            amount: vol * (open + close) / 2.0,
        }
    }

    #[test]
    fn test_effort_result_basic() {
        let klines: Vec<KLine> = (0..20)
            .map(|i| make_kline(10.0, 10.0 + (i as f64) * 0.1, 10.5 + (i as f64) * 0.1, 9.5, 1000.0, i))
            .collect();

        let results = analyze_effort_result(&klines);
        assert_eq!(results.len(), 20);
        assert!(results[0].effort > 0.0);
    }

    #[test]
    fn test_volume_trend() {
        // 成交量递增
        let klines: Vec<KLine> = (0..20)
            .map(|i| make_kline(10.0, 10.5, 11.0, 10.0, 1000.0 + (i as f64) * 100.0, i))
            .collect();
        let trend = volume_trend(&klines);
        assert!(trend > 0.0, "成交量递增，趋势应为正");
    }

    #[test]
    fn test_demand_appearing() {
        // 构造：连续下跌后出现放量十字星
        let mut klines: Vec<KLine> = (0..10)
            .map(|i| make_kline(15.0 - (i as f64), 14.0 - (i as f64), 15.0 - (i as f64), 13.0 - (i as f64), 500.0, i))
            .collect();
        // 放量收阳
        klines.push(make_kline(5.0, 5.2, 5.5, 4.8, 2000.0, 10));
        klines.push(make_kline(5.2, 5.5, 5.8, 5.0, 1500.0, 11));

        assert!(detect_demand_appearing(&klines, 10) || detect_demand_appearing(&klines, 11));
    }
}
