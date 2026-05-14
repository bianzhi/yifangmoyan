//! 线段分析
//!
//! **严格遵照缠论理论的线段定义，采用一维特征值方法**
//!
//! 缠论原文（课程71-78、88）关于线段的定义：
//!
//! 1. 线段由至少3笔组成
//! 2. 线段的破坏以特征序列的分型为判定依据
//!
//! **关键洞察**：相邻的笔共享端点（bi_n.end_price == bi_{n+1}.start_price），
//! 这使得将笔直接映射为2D虚拟K线（high/low）的方法无法正常工作，
//! 因为去包含会过度合并。
//!
//! **正确方法**：使用一维特征值方法
//! 1. 每根笔有一个"特征值" = end_price（终点价格）
//!    - 上升笔：end_price = 高点
//!    - 下降笔：end_price = 低点
//! 2. 在特征值序列上做1D分型检测（局部极值识别）
//! 3. 检测到的分型就是线段的转折点
//! 4. 从分型序列中构建线段
//!
//! 这种方法的合理性：
//! - 缠论原文说"特征序列的顶分型就是线段的终点"
//! - 特征序列的本质是笔端点的1D序列
//! - 由于笔交替（up, down, up...），特征值自然交替（高点、低点、高点...）
//! - 局部极值就是特征序列中的分型转折点

use yifang_data::{Bi, XianDuan};

/// 默认最小线段长度（笔数），对齐缠论定义：至少3笔
const DEFAULT_MIN_XD_LEN: usize = 3;

/// 构建线段
pub fn build_xd(bis: &[Bi]) -> Vec<XianDuan> {
    build_xd_with_min_len(bis, None)
}

/// 带自定义最小线段长度的构建
pub fn build_xd_with_min_len(bis: &[Bi], min_xd_len: Option<usize>) -> Vec<XianDuan> {
    let min_len = min_xd_len.unwrap_or(DEFAULT_MIN_XD_LEN);
    if bis.len() < min_len {
        return Vec::new();
    }

    // Step 1: 提取特征值序列
    // 每根笔的特征值 = end_price
    // 同时记录 start_price 用于计算线段的价格范围
    let feature_values: Vec<FeatureValue> = bis.iter().enumerate().map(|(i, bi)| {
        FeatureValue {
            bi_index: i,
            value: bi.end_price,
        }
    }).collect();

    // Step 2: 在特征值序列上检测1D分型（局部极值）
    let fxs = detect_1d_fenxing(&feature_values);
    if fxs.len() < 2 {
        return Vec::new();
    }

    // Step 3: 确保分型顶底交替（去掉连续同类型的分型）
    let alt_fxs = ensure_1d_alternating(fxs);
    if alt_fxs.len() < 2 {
        return Vec::new();
    }

    // Step 4: 从分型序列构建线段
    build_xd_from_fenxing(&alt_fxs, bis, min_len)
}

/// 特征值
#[derive(Debug, Clone)]
struct FeatureValue {
    /// 对应原始笔的索引
    bi_index: usize,
    /// 特征值 = end_price
    value: f64,
}

/// 1D分型
#[derive(Debug, Clone)]
struct FenXing1D {
    /// 分型类型
    mark: FxMark1D,
    /// 分型值
    value: f64,
    /// 对应的笔索引
    bi_index: usize,
}

/// 分型类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FxMark1D {
    /// 顶分型（局部极大值）
    Top,
    /// 底分型（局部极小值）
    Bottom,
}

/// 在特征值序列上检测1D分型
///
/// 简单的局部极值检测：
/// - 顶分型：value[i] > value[i-1] 且 value[i] > value[i+1]
/// - 底分型：value[i] < value[i-1] 且 value[i] < value[i+1]
fn detect_1d_fenxing(fvs: &[FeatureValue]) -> Vec<FenXing1D> {
    if fvs.len() < 3 {
        return Vec::new();
    }

    let mut fxs = Vec::new();
    for i in 1..fvs.len() - 1 {
        let prev = fvs[i - 1].value;
        let cur = fvs[i].value;
        let next = fvs[i + 1].value;

        if cur > prev && cur > next {
            fxs.push(FenXing1D {
                mark: FxMark1D::Top,
                value: cur,
                bi_index: fvs[i].bi_index,
            });
        } else if cur < prev && cur < next {
            fxs.push(FenXing1D {
                mark: FxMark1D::Bottom,
                value: cur,
                bi_index: fvs[i].bi_index,
            });
        }
    }
    fxs
}

/// 确保1D分型序列顶底交替
///
/// 如果出现连续同类型分型，保留更极端的那个
fn ensure_1d_alternating(fxs: Vec<FenXing1D>) -> Vec<FenXing1D> {
    if fxs.is_empty() {
        return Vec::new();
    }

    let mut result = vec![fxs[0].clone()];

    for fx in &fxs[1..] {
        let last = result.last().unwrap();

        if fx.mark == last.mark {
            // 同类型：保留更极端的
            let should_replace = match fx.mark {
                FxMark1D::Top => fx.value > last.value,
                FxMark1D::Bottom => fx.value < last.value,
            };
            if should_replace {
                let last = result.last_mut().unwrap();
                *last = fx.clone();
            }
        } else {
            // 不同类型：顶的 fx 值必须高于底的 fx 值
            let valid = match fx.mark {
                FxMark1D::Top => fx.value > last.value,
                FxMark1D::Bottom => fx.value < last.value,
            };
            if valid {
                result.push(fx.clone());
            } else {
                // 不满足约束：保留更极端的
                let should_replace = match fx.mark {
                    FxMark1D::Top => fx.value > last.value,
                    FxMark1D::Bottom => fx.value < last.value,
                };
                if should_replace {
                    let last = result.last_mut().unwrap();
                    *last = fx.clone();
                }
            }
        }
    }

    result
}

/// 从分型序列构建线段
///
/// 将分型序列中相邻的顶底分型配对，形成线段：
/// - 底→顶：上升线段
/// - 顶→底：下降线段
///
/// 对齐 Python CZSC.__update_bi 的增量逻辑：
/// - 从分型序列的第一个分型开始
/// - 找最极端的反向分型作为 fx_b（与 check_bi 对齐）
/// - 后续线段：从上一条线段的终点继续找
fn build_xd_from_fenxing(fxs: &[FenXing1D], bis: &[Bi], min_len: usize) -> Vec<XianDuan> {
    let mut xd_list: Vec<XianDuan> = Vec::new();

    if fxs.len() < 2 {
        return Vec::new();
    }

    // 找第一条线段：从第一个分型开始
    // 与 Python CZSC.__update_bi 不同的是，增量过程从第一个出现
    // 的分型开始找笔。但我们的1D分型已经是局部极值，
    // 所以直接从第一个分型出发。
    //
    // 对齐 Python CZSC 的做法：先找同方向最极端的分型作为 fx_a，
    // 然后找最极端的反向分型作为 fx_b。
    //
    // 但"最极端的 fx_a"只应该在已经找到的同方向分型中选择，
    // 即所有相同类型中 price 最极端的那个，这与 Python __update_bi 
    // 中找"第一笔"的逻辑一致。
    let first_mark = fxs[0].mark;
    let mut fx_a_idx = 0;
    let mut fx_a_value = fxs[0].value;

    for (i, fx) in fxs.iter().enumerate() {
        if fx.mark != first_mark {
            continue;
        }
        let is_better = match fx.mark {
            FxMark1D::Top => fx.value >= fx_a_value,
            FxMark1D::Bottom => fx.value <= fx_a_value,
        };
        if is_better {
            fx_a_value = fx.value;
            fx_a_idx = i;
        }
    }

    // 从 fx_a 出发，找最极端的反向分型作为 fx_b
    if let Some(fx_b_idx) = find_xd_endpoint(fxs, fx_a_idx, min_len, bis) {
        let fx_a = &fxs[fx_a_idx];
        let fx_b = &fxs[fx_b_idx];

        let direction = if fx_a.mark == FxMark1D::Bottom {
            "up".to_string()
        } else {
            "down".to_string()
        };

        let start_bi = fx_a.bi_index;
        let end_bi = fx_b.bi_index;

        let xd = XianDuan {
            direction,
            start_index: start_bi as u64,
            end_index: end_bi as u64,
            start_dt: bis[start_bi].end_dt.clone(),
            end_dt: bis[end_bi].end_dt.clone(),
            start_price: fx_a.value,
            end_price: fx_b.value,
            is_finished: true,
        };

        xd_list.push(xd);

        // 增量构建后续线段
        let mut i = fx_b_idx;
        while i + 1 < fxs.len() {
            let fx_a_ref = &fxs[i];
            let fx_b = find_xd_endpoint(fxs, i, min_len, bis);

            match fx_b {
                Some(j) => {
                    let fx_b_ref = &fxs[j];

                    let direction = if fx_a_ref.mark == FxMark1D::Bottom {
                        "up".to_string()
                    } else {
                        "down".to_string()
                    };

                    let start_bi = fx_a_ref.bi_index;
                    let end_bi = fx_b_ref.bi_index;

                    let xd = XianDuan {
                        direction,
                        start_index: start_bi as u64,
                        end_index: end_bi as u64,
                        start_dt: bis[start_bi].end_dt.clone(),
                        end_dt: bis[end_bi].end_dt.clone(),
                        start_price: fx_a_ref.value,
                        end_price: fx_b_ref.value,
                        is_finished: true,
                    };

                    xd_list.push(xd);
                    i = j;
                }
                None => break,
            }
            i += 1;
        }
    }

    xd_list
}

/// 找到线段的终点分型
///
/// 对齐 Python check_bi 逻辑：
/// - 从 fx_a 出发，找方向相反且满足条件的分型
/// - 上升线段：找最高顶分型（fx > fx_a.fx）
/// - 下降线段：找最低底分型（fx < fx_a.fx）
/// - 保证线段包含至少 min_len 根笔
///
/// bi_count 计算：对齐 Python check_bi 中 bars_a_count 的计算方式
/// fx_a 的"元素"范围是 [fx_a.bi_index-1, fx_a.bi_index+1]
/// fx_b 的"元素"范围是 [fx_b.bi_index-1, fx_b.bi_index+1]
/// bars_a 从 fx_a 第一个元素到 fx_b 最后一个元素
fn find_xd_endpoint(fxs: &[FenXing1D], start_idx: usize, min_len: usize, bis: &[Bi]) -> Option<usize> {
    let fx_a = &fxs[start_idx];

    match fx_a.mark {
        FxMark1D::Bottom => {
            // 找上升线段的终点：最高顶分型
            let mut best_idx: Option<usize> = None;
            let mut best_high = f64::NEG_INFINITY;

            for j in (start_idx + 1)..fxs.len() {
                let fx_b = &fxs[j];
                if fx_b.mark != FxMark1D::Top {
                    continue;
                }
                // fx_b.value 必须高于 fx_a.value（上升线段）
                if fx_b.value <= fx_a.value {
                    continue;
                }
                // 检查线段长度：对齐 Python bars_a_count
                // Python: bars_a = [x for x in bars if fx_a.elements[0].dt <= x.dt <= fx_b.elements[2].dt]
                // 1D fenxing 的 elements: fx_a 的范围是 [bi_index-1, bi_index+1]
                //                       fx_b 的范围是 [bi_index-1, bi_index+1]
                let a_start = fx_a.bi_index.saturating_sub(1);
                let b_end = (fx_b.bi_index + 1).min(bis.len() - 1);
                let bi_count = b_end.saturating_sub(a_start) + 1;
                if bi_count < min_len {
                    continue;
                }
                if fx_b.value > best_high {
                    best_high = fx_b.value;
                    best_idx = Some(j);
                }
            }
            best_idx
        }
        FxMark1D::Top => {
            // 找下降线段的终点：最低底分型
            let mut best_idx: Option<usize> = None;
            let mut best_low = f64::INFINITY;

            for j in (start_idx + 1)..fxs.len() {
                let fx_b = &fxs[j];
                if fx_b.mark != FxMark1D::Bottom {
                    continue;
                }
                // fx_b.value 必须低于 fx_a.value（下降线段）
                if fx_b.value >= fx_a.value {
                    continue;
                }
                // 检查线段长度
                let a_start = fx_a.bi_index.saturating_sub(1);
                let b_end = (fx_b.bi_index + 1).min(bis.len() - 1);
                let bi_count = b_end.saturating_sub(a_start) + 1;
                if bi_count < min_len {
                    continue;
                }
                if fx_b.value < best_low {
                    best_low = fx_b.value;
                    best_idx = Some(j);
                }
            }
            best_idx
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_bi(id: usize, dir: &str, start_price: f64, end_price: f64, start_idx: u64, end_idx: u64) -> Bi {
        Bi {
            direction: dir.to_string(),
            start_index: start_idx,
            end_index: end_idx,
            start_dt: format!("t{}", id),
            end_dt: format!("t{}", id + 1),
            start_price,
            end_price,
            is_finished: true,
        }
    }

    #[test]
    fn test_xd_min_3_bi() {
        let bis = vec![
            make_bi(0, "up", 10.0, 20.0, 0, 5),
            make_bi(1, "down", 20.0, 15.0, 5, 10),
        ];
        let xds = build_xd(&bis);
        assert!(xds.is_empty(), "2笔不应形成线段");
    }

    #[test]
    fn test_xd_with_sufficient_diversity() {
        let mut bis = Vec::new();
        let mut idx = 0u64;

        bis.push(make_bi(0, "up", 10.0, 25.0, idx, idx + 6)); idx += 6;
        bis.push(make_bi(1, "down", 25.0, 17.0, idx, idx + 6)); idx += 6;
        bis.push(make_bi(2, "up", 17.0, 30.0, idx, idx + 6)); idx += 6;
        bis.push(make_bi(3, "down", 30.0, 14.0, idx, idx + 6)); idx += 6;
        bis.push(make_bi(4, "up", 14.0, 28.0, idx, idx + 6)); idx += 6;
        bis.push(make_bi(5, "down", 28.0, 8.0, idx, idx + 6)); idx += 6;

        let xds = build_xd(&bis);
        assert!(!xds.is_empty(), "足够多样性的笔模式应产生线段，实际产生 {} 条", xds.len());
    }

    #[test]
    fn test_xd_direction_consistency() {
        let mut bis = Vec::new();
        let mut idx = 0u64;

        bis.push(make_bi(0, "up", 10.0, 30.0, idx, idx + 5)); idx += 5;
        bis.push(make_bi(1, "down", 30.0, 18.0, idx, idx + 5)); idx += 5;
        bis.push(make_bi(2, "up", 18.0, 35.0, idx, idx + 5)); idx += 5;
        bis.push(make_bi(3, "down", 35.0, 20.0, idx, idx + 5)); idx += 5;
        bis.push(make_bi(4, "up", 20.0, 28.0, idx, idx + 5)); idx += 5;
        bis.push(make_bi(5, "down", 28.0, 8.0, idx, idx + 5)); idx += 5;
        bis.push(make_bi(6, "up", 8.0, 15.0, idx, idx + 5)); idx += 5;
        bis.push(make_bi(7, "down", 15.0, 5.0, idx, idx + 5)); idx += 5;

        let xds = build_xd(&bis);
        for xd in &xds {
            if xd.direction == "up" {
                assert!(xd.end_price > xd.start_price,
                    "上升线段 end_price({}) 应 > start_price({})", xd.end_price, xd.start_price);
            } else {
                assert!(xd.end_price < xd.start_price,
                    "下降线段 end_price({}) 应 < start_price({})", xd.end_price, xd.start_price);
            }
        }
    }

    #[test]
    fn test_xd_alternating_direction() {
        let mut bis = Vec::new();
        let mut idx = 0u64;

        for round in 0..4 {
            let base = 10.0 + round as f64 * 20.0;
            bis.push(make_bi(bis.len(), "up", base, base + 20.0, idx, idx + 5)); idx += 5;
            bis.push(make_bi(bis.len(), "down", base + 20.0, base + 8.0, idx, idx + 5)); idx += 5;
            bis.push(make_bi(bis.len(), "up", base + 8.0, base + 25.0, idx, idx + 5)); idx += 5;
            bis.push(make_bi(bis.len(), "down", base + 25.0, base + 5.0, idx, idx + 5)); idx += 5;
            bis.push(make_bi(bis.len(), "up", base + 5.0, base + 18.0, idx, idx + 5)); idx += 5;
        }

        let xds = build_xd(&bis);
        for i in 1..xds.len() {
            assert_ne!(xds[i].direction, xds[i - 1].direction, "相邻线段方向必须交替");
        }
    }

    #[test]
    fn test_xd_complex_pattern() {
        let mut bis = Vec::new();
        let mut idx = 0u64;

        bis.push(make_bi(0, "up", 100.0, 120.0, idx, idx + 5)); idx += 5;
        bis.push(make_bi(1, "down", 120.0, 108.0, idx, idx + 5)); idx += 5;
        bis.push(make_bi(2, "up", 108.0, 125.0, idx, idx + 5)); idx += 5;
        bis.push(make_bi(3, "down", 125.0, 110.0, idx, idx + 5)); idx += 5;
        bis.push(make_bi(4, "up", 110.0, 130.0, idx, idx + 5)); idx += 5;
        bis.push(make_bi(5, "down", 130.0, 95.0, idx, idx + 5)); idx += 5;
        bis.push(make_bi(6, "up", 95.0, 105.0, idx, idx + 5)); idx += 5;
        bis.push(make_bi(7, "down", 105.0, 88.0, idx, idx + 5)); idx += 5;
        bis.push(make_bi(8, "up", 88.0, 98.0, idx, idx + 5)); idx += 5;
        bis.push(make_bi(9, "down", 98.0, 80.0, idx, idx + 5)); idx += 5;

        let xds = build_xd(&bis);
        assert!(xds.len() <= bis.len() / 3 + 1, "线段数量不应超过笔数/3+1");
    }

    #[test]
    fn test_1d_fenxing_detection() {
        // 简单特征值序列：10, 25, 17, 30, 14, 28, 8
        // 局部极值：25(top), 17(bottom), 30(top), 14(bottom), 28(top)
        let fvs: Vec<FeatureValue> = vec![
            10.0, 25.0, 17.0, 30.0, 14.0, 28.0, 8.0
        ].into_iter().enumerate().map(|(i, v)| FeatureValue {
            bi_index: i,
            value: v,
        }).collect();

        let fxs = detect_1d_fenxing(&fvs);
        assert_eq!(fxs.len(), 5, "应检测到5个分型");
        assert!(matches!(fxs[0].mark, FxMark1D::Top)); // 25
        assert!(matches!(fxs[1].mark, FxMark1D::Bottom)); // 17
        assert!(matches!(fxs[2].mark, FxMark1D::Top)); // 30
        assert!(matches!(fxs[3].mark, FxMark1D::Bottom)); // 14
        assert!(matches!(fxs[4].mark, FxMark1D::Top)); // 28
    }
}
