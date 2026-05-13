//! 分型识别
//!
//! **严格对齐 czsc 0.9.9 的 check_fx / check_fxs 实现**
//!
//! 顶分型：k2 的高点和低点都高于 k1 和 k3
//! 底分型：k2 的高点和低点都低于 k1 和 k3
//!
//! 关键约束：check_fxs 返回的分型序列**必须顶底交替**。
//! 如果出现连续两个同类型分型，只保留更极端的那个。

use crate::include::NewBar;
use yifang_data::FenXing;

/// 分型标记
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FxMark {
    /// 顶分型
    Top,
    /// 底分型
    Bottom,
}

/// 分型识别结果（内部使用，包含完整的结构信息）
#[derive(Debug, Clone)]
pub struct FxResult {
    pub mark: FxMark,
    /// 构成分型中间那根合并K线在 bars_ubi 中的索引位置
    pub bar_index: usize,
    /// 构成分型中间那根合并K线对应的原始K线索引（用于映射回K线序列）
    pub merged_index: usize,
    pub dt: String,
    pub high: f64,
    pub low: f64,
    /// 分型的特征价格：顶分型取 high，底分型取 low
    pub fx: f64,
    /// 构成分型的3根合并K线的 bar_index
    pub bars: [usize; 3],
}

/// 检查三根合并 K 线是否构成分型
///
/// 对齐 czsc check_fx:
/// - 顶分型: k1.high < k2.high > k3.high AND k1.low < k2.low > k3.low
/// - 底分型: k1.low > k2.low < k3.low AND k1.high > k2.high < k3.high
pub fn check_fx(k1: &NewBar, k2: &NewBar, k3: &NewBar, k2_idx: usize, k1_idx: usize) -> Option<FxResult> {
    // 顶分型：k2 的高低点都高于 k1 和 k3
    if k1.high < k2.high && k2.high > k3.high && k1.low < k2.low && k2.low > k3.low {
        return Some(FxResult {
            mark: FxMark::Top,
            bar_index: k2_idx,
            merged_index: k2.id as usize,
            dt: k2.dt.clone(),
            high: k2.high,
            low: k2.low,
            fx: k2.high,
            bars: [k1_idx, k2_idx, k2_idx + 1],
        });
    }

    // 底分型：k2 的高低点都低于 k1 和 k3
    if k1.low > k2.low && k2.low < k3.low && k1.high > k2.high && k2.high < k3.high {
        return Some(FxResult {
            mark: FxMark::Bottom,
            bar_index: k2_idx,
            merged_index: k2.id as usize,
            dt: k2.dt.clone(),
            high: k2.high,
            low: k2.low,
            fx: k2.low,
            bars: [k1_idx, k2_idx, k2_idx + 1],
        });
    }

    None
}

/// 对合并后的 K 线序列识别所有分型
///
/// 对齐 czsc check_fxs:
/// 遍历 bars，对每三根连续K线调用 check_fx。
/// **关键约束：返回的分型序列必须顶底交替**。
/// 如果出现连续同类型分型，保留更极端的那个（顶取更高的，底取更低的）。
pub fn check_fxs(bars: &[NewBar]) -> Vec<FxResult> {
    if bars.len() < 3 {
        return Vec::new();
    }

    let raw_fxs: Vec<FxResult> = (1..bars.len() - 1)
        .filter_map(|i| check_fx(&bars[i - 1], &bars[i], &bars[i + 1], i, i - 1))
        .collect();

    // 强制顶底交替：如果连续同类型，保留更极端的
    ensure_alternating(raw_fxs)
}

/// 确保分型序列顶底交替
///
/// 规则：遇到连续同类型分型时：
/// - 连续顶分型：保留高点更高的
/// - 连续底分型：保留低点更低的
fn ensure_alternating(fxs: Vec<FxResult>) -> Vec<FxResult> {
    if fxs.is_empty() {
        return Vec::new();
    }

    let mut result = vec![fxs[0].clone()];

    for fx in &fxs[1..] {
        let last = result.last().unwrap();

        if fx.mark == last.mark {
            // 同类型：保留更极端的
            let should_replace = match fx.mark {
                FxMark::Top => fx.fx > last.fx,     // 顶分型取更高的
                FxMark::Bottom => fx.fx < last.fx,   // 底分型取更低的
            };

            if should_replace {
                let last = result.last_mut().unwrap();
                *last = fx.clone();
            }
        } else {
            // 不同类型：
            // 额外校验：顶分型 fx 值必须高于前一个底分型 fx 值，
            // 底分型 fx 值必须低于前一个顶分型 fx 值。
            // 这是缠论的基本逻辑约束——顶一定高于底。
            let valid = match fx.mark {
                FxMark::Top => fx.fx > last.fx,
                FxMark::Bottom => fx.fx < last.fx,
            };

            if valid {
                result.push(fx.clone());
            } else {
                // 不满足约束时，保留更极端的
                // 例如：前一个底分型 low=10，当前顶分型 high=9 → 不合理，需保留更极端的
                // valid=false implies no replacement needed; discard
                // 这里 valid=false 意味着 should_replace 也为 false,
                // 所以直接跳过这个分型
                // 但这也可能意味着前一个分型需要被更新
                // 保守处理：丢弃不合理的分型。
                // 实际数据中极少出现，如果频繁出现说明去包含逻辑有误。
                #[cfg(debug_assertions)]
                eprintln!(
                    "WARN: 分型不合理被丢弃: {:?} fx={} 在 {:?} fx={} 之后",
                    fx.mark, fx.fx, last.mark, last.fx
                );
            }
        }
    }

    result
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
            price: fx.fx,
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
        // 构建一个确保去包含后仍有3根K线的数据
        // k0: 上升   high=11, low=10
        // k1: 继续上升 high=14, low=12  ← 顶分型中间
        // k2: 下降   high=13, low=9     ← k2.low=9 < k1.low=12, 不被包含
        let klines = vec![
            make_kline(0, "2024-01-01", 10.0, 11.0, 11.0, 10.0),   // k0
            make_kline(1, "2024-01-02", 12.0, 14.0, 14.0, 12.0),   // k1 (顶)
            make_kline(2, "2024-01-03", 13.0, 9.0,  13.0, 9.0),    // k2 (k2.high=13 < k1.high=14, 不被包含)
        ];
        let merged = remove_include(&klines);
        assert!(merged.len() >= 3, "去包含后应有3根K线, 实际{}", merged.len());
        let fxs = check_fxs(&merged);
        assert_eq!(fxs.len(), 1);
        assert_eq!(fxs[0].mark, FxMark::Top);
        assert_eq!(fxs[0].fx, 14.0); // 顶分型取 high
    }

    #[test]
    fn test_bottom_fenxing() {
        // 构建确保去包含后有3根K线的底分型数据
        // k0: 下降   high=13, low=11
        // k1: 继续下降 high=11, low=8   ← 底分型中间
        // k2: 上升   high=12, low=9     ← 不被包含
        let klines = vec![
            make_kline(0, "2024-01-01", 13.0, 11.0, 13.0, 11.0),   // k0
            make_kline(1, "2024-01-02", 11.0, 8.0,  11.0, 8.0),    // k1 (底)
            make_kline(2, "2024-01-03", 9.0,  12.0, 12.0, 9.0),    // k2
        ];
        let merged = remove_include(&klines);
        assert!(merged.len() >= 3, "去包含后应有3根K线, 实际{}", merged.len());
        let fxs = check_fxs(&merged);
        assert_eq!(fxs.len(), 1);
        assert_eq!(fxs[0].mark, FxMark::Bottom);
        assert_eq!(fxs[0].fx, 8.0); // 底分型取 low
    }

    #[test]
    fn test_alternating_fxs() {
        // 测试分型交替约束
        // 构建一个序列，产生两个连续顶分型
        let klines = vec![
            make_kline(0, "2024-01-01", 10.0, 11.0, 11.0, 10.0),
            make_kline(1, "2024-01-02", 11.0, 13.0, 13.0, 11.0), // 顶1
            make_kline(2, "2024-01-03", 13.0, 12.0, 13.0, 11.5),
            make_kline(3, "2024-01-04", 12.0, 14.0, 14.0, 11.5), // 顶2（更高）
            make_kline(4, "2024-01-05", 14.0, 10.0, 14.0, 10.0),
        ];
        let merged = remove_include(&klines);
        let fxs = check_fxs(&merged);
        // 连续顶分型应只保留更高的那个
        for i in 1..fxs.len() {
            assert_ne!(fxs[i].mark, fxs[i - 1].mark, "分型必须顶底交替");
        }
    }
}
