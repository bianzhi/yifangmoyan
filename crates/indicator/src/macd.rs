//! MACD 指标计算
//!
//! MACD = EMA(close, fast) - EMA(close, slow)
//! Signal = EMA(MACD, signal_period)
//! Histogram = MACD - Signal

use yifang_data::{KLine, MacdData};

/// 计算MACD指标
///
/// 参数:
/// - klines: K 线数据
/// - fast_period: 快速 EMA 周期 (默认 12)
/// - slow_period: 慢速 EMA 周期 (默认 26)
/// - signal_period: 信号线周期 (默认 9)
pub fn calc_macd(
    klines: &[KLine],
    fast_period: usize,
    slow_period: usize,
    signal_period: usize,
) -> MacdData {
    if klines.len() < slow_period {
        return MacdData::default();
    }

    let closes: Vec<f64> = klines.iter().map(|k| k.close).collect();

    // 计算 EMA
    let fast_ema = calc_ema(&closes, fast_period);
    let slow_ema = calc_ema(&closes, slow_period);

    // DIF = 快速 EMA - 慢速 EMA
    let dif: Vec<f64> = fast_ema
        .iter()
        .zip(slow_ema.iter())
        .map(|(f, s)| f - s)
        .collect();

    // DEA = EMA(DIF, signal_period)
    let dea = calc_ema(&dif, signal_period);

    // MACD Histogram = 2 * (DIF - DEA)
    let macd_hist: Vec<f64> = dif
        .iter()
        .zip(dea.iter())
        .map(|(d, e)| 2.0 * (d - e))
        .collect();

    MacdData {
        dif,
        dea,
        macd_hist,
    }
}

/// 计算 EMA (Exponential Moving Average)
fn calc_ema(data: &[f64], period: usize) -> Vec<f64> {
    if data.is_empty() || period == 0 {
        return vec![0.0; data.len()];
    }

    let multiplier = 2.0 / (period as f64 + 1.0);
    let mut ema = Vec::with_capacity(data.len());

    // 第一根使用 SMA
    let mut sum = 0.0;
    for (i, &val) in data.iter().enumerate() {
        if i < period {
            sum += val;
            if i == period - 1 {
                ema.push(sum / period as f64);
            } else {
                ema.push(0.0);
            }
        } else {
            let prev = ema[i - 1];
            let current = (val - prev) * multiplier + prev;
            ema.push(current);
        }
    }

    ema
}

#[cfg(test)]
mod tests {
    use super::*;
    use yifang_data::TimeFrame;

    fn make_kline(close: f64) -> KLine {
        KLine {
            symbol: "test".to_string(),
            timeframe: TimeFrame::D,
            dt: "".to_string(),
            id: 0,
            open: close,
            close,
            high: close * 1.01,
            low: close * 0.99,
            vol: 1000.0,
            amount: 10000.0,
        }
    }

    #[test]
    fn test_macd_calc() {
        let klines: Vec<KLine> = (0..50)
            .map(|i| make_kline(10.0 + (i as f64).sin() * 2.0))
            .collect();

        let macd = calc_macd(&klines, 12, 26, 9);
        assert_eq!(macd.dif.len(), 50);
        assert_eq!(macd.dea.len(), 50);
        assert_eq!(macd.macd_hist.len(), 50);
    }
}
