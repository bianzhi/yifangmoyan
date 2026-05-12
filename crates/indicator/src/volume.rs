//! 成交量分析

use yifang_data::KLine;

/// 成交量分析结果
#[derive(Debug, Clone)]
pub struct VolumeAnalysis {
    /// 量价配合度
    pub vp_score: Vec<f64>,
    /// OBV (On Balance Volume)
    pub obv: Vec<f64>,
}

impl VolumeAnalysis {
    /// 计算成交量分析
    pub fn analyze(klines: &[KLine]) -> Self {
        let vp_score = calc_vp_score(klines);
        let obv = calc_obv(klines);
        Self { vp_score, obv }
    }
}

/// 量价配合度
///
/// 正值: 量价同向 (涨放量/跌缩量)
/// 负值: 量价背离 (涨缩量/跌放量)
fn calc_vp_score(klines: &[KLine]) -> Vec<f64> {
    if klines.is_empty() {
        return Vec::new();
    }

    let avg_vol: f64 = klines.iter().map(|k| k.vol).sum::<f64>() / klines.len() as f64;

    klines
        .iter()
        .map(|k| {
            let price_dir = if k.close > k.open { 1.0 } else { -1.0 };
            let vol_ratio = k.vol / avg_vol;
            price_dir * vol_ratio
        })
        .collect()
}

/// OBV (On Balance Volume)
fn calc_obv(klines: &[KLine]) -> Vec<f64> {
    if klines.is_empty() {
        return Vec::new();
    }

    let mut obv = vec![0.0; klines.len()];

    for i in 1..klines.len() {
        let delta = if klines[i].close > klines[i - 1].close {
            klines[i].vol
        } else if klines[i].close < klines[i - 1].close {
            -klines[i].vol
        } else {
            0.0
        };
        obv[i] = obv[i - 1] + delta;
    }

    obv
}
