//! 威科夫阶段识别
//!
//! 基于价格趋势和成交量特征，识别当前市场所处的威科夫阶段。

use yifang_data::KLine;
use super::types::WyckoffPhase;

/// 识别 K 线序列的威科夫阶段
pub fn identify_phases(klines: &[KLine]) -> Vec<(usize, WyckoffPhase)> {
    if klines.len() < 20 {
        return Vec::new();
    }

    let mut phases = Vec::new();

    // 简化的阶段识别逻辑：
    // 1. 计算移动平均线趋势
    // 2. 计算成交量趋势
    // 3. 综合判断阶段

    let window = 20usize;
    for i in window..klines.len() {
        let slice = &klines[i - window..i];
        let phase = classify_phase(slice);
        phases.push((i, phase));
    }

    phases
}

/// 根据近期 K 线和成交量判断当前阶段
fn classify_phase(klines: &[KLine]) -> WyckoffPhase {
    if klines.len() < 10 {
        return WyckoffPhase::Accumulation;
    }

    let half = klines.len() / 2;
    let first_half = &klines[..half];
    let second_half = &klines[half..];

    // 价格趋势
    let first_avg_close: f64 = first_half.iter().map(|k| k.close).sum::<f64>() / first_half.len() as f64;
    let second_avg_close: f64 = second_half.iter().map(|k| k.close).sum::<f64>() / second_half.len() as f64;
    let price_trend = second_avg_close - first_avg_close;

    // 成交量趋势
    let first_avg_vol: f64 = first_half.iter().map(|k| k.vol).sum::<f64>() / first_half.len() as f64;
    let second_avg_vol: f64 = second_half.iter().map(|k| k.vol).sum::<f64>() / second_half.len() as f64;
    let vol_trend = second_avg_vol - first_avg_vol;

    // 波动率
    let avg_body: f64 = klines.iter().map(|k| k.body()).sum::<f64>() / klines.len() as f64;
    let avg_close: f64 = klines.iter().map(|k| k.close).sum::<f64>() / klines.len() as f64;
    let _volatility = avg_body / avg_close;

    // 简化规则
    if price_trend > 0.0 && vol_trend > 0.0 {
        WyckoffPhase::Markup
    } else if price_trend > 0.0 && vol_trend <= 0.0 {
        WyckoffPhase::Accumulation
    } else if price_trend <= 0.0 && vol_trend > 0.0 {
        WyckoffPhase::Distribution
    } else {
        WyckoffPhase::Markdown
    }
}
