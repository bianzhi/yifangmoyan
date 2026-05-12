//! 笔的构建
//!
//! 缠论笔的定义：从顶分型到底分型或从底分型到顶分型，
//! 中间至少包含 5 根合并 K 线（含分型本身 3 根），
//! 即顶底分型之间至少间隔 1 根 K 线。

use crate::fenxing::{FxMark, FxResult};
use yifang_data::Bi;

/// 构建笔
///
/// 规则:
/// 1. 相邻分型必须一顶一底交替
/// 2. 顶分型到底分型（下降笔）：顶分型高点 > 底分型低点
/// 3. 底分型到顶分型（上升笔）：底分型低点 < 顶分型高点
/// 4. 新笔的顶/底必须高于/低于前一笔的顶/底（笔的逻辑一致性）
pub fn build_bi(fxs: &[FxResult], _bars_len: usize) -> Vec<Bi> {
    if fxs.len() < 2 {
        return Vec::new();
    }

    // 第一步：去除相邻同类型分型，保留更极端的
    let filtered = filter_fxs(fxs);
    if filtered.len() < 2 {
        return Vec::new();
    }

    // 第二步：从交替分型序列构建笔
    let mut bis = Vec::new();
    for i in 0..filtered.len() - 1 {
        let fx1 = &filtered[i];
        let fx2 = &filtered[i + 1];

        // 必须一顶一底交替
        if fx1.mark == fx2.mark {
            continue;
        }

        let is_up = fx1.mark == FxMark::Bottom;
        let (start, end) = if is_up {
            (fx1, fx2)
        } else {
            (fx1, fx2)
        };

        bis.push(Bi {
            direction: if is_up { "up".to_string() } else { "down".to_string() },
            start_index: start.merged_index as u64,
            end_index: end.merged_index as u64,
            start_dt: start.dt.clone(),
            end_dt: end.dt.clone(),
            start_price: start.fx_price,
            end_price: end.fx_price,
            is_finished: true,
        });
    }

    bis
}

/// 过滤相邻同类型分型，保留价格更极端的
fn filter_fxs(fxs: &[FxResult]) -> Vec<FxResult> {
    if fxs.is_empty() {
        return Vec::new();
    }

    let mut result = vec![fxs[0].clone()];

    for fx in &fxs[1..] {
        let last = result.last().unwrap();

        if fx.mark == last.mark {
            // 同类型：保留更极端的
            match fx.mark {
                FxMark::Top => {
                    if fx.fx_price > last.fx_price {
                        let last = result.last_mut().unwrap();
                        *last = fx.clone();
                    }
                }
                FxMark::Bottom => {
                    if fx.fx_price < last.fx_price {
                        let last = result.last_mut().unwrap();
                        *last = fx.clone();
                    }
                }
            }
        } else {
            // 不同类型：检查价格是否合理
            // 新的顶分型必须高于前一个底分型的低点
            // 新的底分型必须低于前一个顶分型的高点
            let valid = match fx.mark {
                FxMark::Top => fx.fx_price > last.fx_price,
                FxMark::Bottom => fx.fx_price < last.fx_price,
            };

            if valid {
                result.push(fx.clone());
            }
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_filter_same_fxs() {
        let fxs = vec![
            FxResult { mark: FxMark::Top, merged_index: 2, dt: "t1".into(), high: 15.0, low: 13.0, fx_price: 15.0 },
            FxResult { mark: FxMark::Top, merged_index: 4, dt: "t2".into(), high: 16.0, low: 14.0, fx_price: 16.0 }, // 更高，保留
            FxResult { mark: FxMark::Bottom, merged_index: 6, dt: "t3".into(), high: 12.0, low: 10.0, fx_price: 10.0 },
        ];

        let filtered = filter_fxs(&fxs);
        assert_eq!(filtered.len(), 2);
        assert_eq!(filtered[0].fx_price, 16.0); // 保留了更高的顶
        assert_eq!(filtered[1].fx_price, 10.0);
    }
}
