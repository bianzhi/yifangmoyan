//! 分型识别
//!
//! 顶分型：第二根 K 线的高点和低点都高于相邻 K 线
//! 底分型：第二根 K 线的高点和低点都低于相邻 K 线

use crate::include::MergedBar;
use yifang_data::FenXing;

/// 分型标记
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FxMark {
    /// 顶分型
    Top,
    /// 底分型
    Bottom,
}

/// 识别结果
#[derive(Debug, Clone)]
pub struct FxResult {
    pub mark: FxMark,
    pub merged_index: usize,
    pub dt: String,
    pub high: f64,
    pub low: f64,
    pub fx_price: f64,
}

/// 检查三根合并 K 线是否构成分型
pub fn check_fx(k1: &MergedBar, k2: &MergedBar, k3: &MergedBar) -> Option<FxResult> {
    // 顶分型：k2 高点和低点都高于 k1 和 k3
    if k2.high > k1.high && k2.high > k3.high && k2.low > k1.low && k2.low > k3.low {
        return Some(FxResult {
            mark: FxMark::Top,
            merged_index: k2.id as usize,
            dt: k2.dt.clone(),
            high: k2.high,
            low: k2.low,
            fx_price: k2.high,
        });
    }

    // 底分型：k2 高点和低点都低于 k1 和 k3
    if k2.low < k1.low && k2.low < k3.low && k2.high < k1.high && k2.high < k3.high {
        return Some(FxResult {
            mark: FxMark::Bottom,
            merged_index: k2.id as usize,
            dt: k2.dt.clone(),
            high: k2.high,
            low: k2.low,
            fx_price: k2.low,
        });
    }

    None
}

/// 对合并后的 K 线序列识别所有分型
pub fn check_fxs(bars: &[MergedBar]) -> Vec<FxResult> {
    if bars.len() < 3 {
        return Vec::new();
    }

    let mut fxs = Vec::new();
    for i in 1..bars.len() - 1 {
        if let Some(fx) = check_fx(&bars[i - 1], &bars[i], &bars[i + 1]) {
            fxs.push(fx);
        }
    }
    fxs
}

/// 将 FxResult 转换为前端的 FenXing 类型
pub fn to_fenxing(fxs: &[FxResult]) -> Vec<FenXing> {
    fxs.iter()
        .map(|fx| FenXing {
            fx_type: match fx.mark {
                FxMark::Top => "top".to_string(),
                FxMark::Bottom => "bottom".to_string(),
            },
            index: fx.merged_index as u64,
            dt: fx.dt.clone(),
            price: fx.fx_price,
        })
        .collect()
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
    fn test_top_fenxing() {
        let klines = vec![
            make_kline(0, "2024-01-01", 10.0, 11.0, 11.0, 10.0),
            make_kline(1, "2024-01-02", 11.0, 13.0, 13.0, 11.0), // 顶
            make_kline(2, "2024-01-03", 13.0, 10.5, 13.0, 10.0),
        ];
        let merged = remove_include(&klines);
        let fxs = check_fxs(&merged);
        assert_eq!(fxs.len(), 1);
        assert_eq!(fxs[0].mark, FxMark::Top);
    }

    #[test]
    fn test_bottom_fenxing() {
        let klines = vec![
            make_kline(0, "2024-01-01", 13.0, 11.0, 13.0, 11.0),
            make_kline(1, "2024-01-02", 11.0, 9.0, 11.0, 9.0), // 底
            make_kline(2, "2024-01-03", 9.0, 12.0, 12.0, 9.0),
        ];
        let merged = remove_include(&klines);
        let fxs = check_fxs(&merged);
        assert_eq!(fxs.len(), 1);
        assert_eq!(fxs[0].mark, FxMark::Bottom);
    }
}
