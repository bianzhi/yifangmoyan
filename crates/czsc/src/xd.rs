//! 线段分析
//!
//! **严格对齐缠论原文定义：特征序列法**
//!
//! 线段的定义与判断规则（缠论第71课、第77课）：
//!
//! 1. 线段由至少3笔组成
//! 2. 特征序列：将笔序列中方向相同的笔提取出来，构成"特征序列元素"
//!    - 上升线段中，取所有下降笔构成特征序列
//!    - 下降线段中，取所有上升笔构成特征序列
//! 3. 特征序列的包含处理：同去包含K线的方法，对特征序列元素做包含处理
//! 4. 线段终结的条件（特征序列分型破坏）：
//!    - 特征序列出现顶分型 → 上升线段终结
//!    - 特征序列出现底分型 → 下降线段终结
//! 5. 也可以参考 czsc 的简化判断：
//!    - 线段至少3笔
//!    - 反向笔超过前一同向笔的极值点 → 线段终结

use yifang_data::{Bi, XianDuan};

/// 构建线段
///
/// 基于笔序列，使用特征序列分型破坏法判断线段：
/// 1. 至少3笔才能构成线段
/// 2. 特征序列出现分型 → 线段终结
/// 3. 对齐 czsc 的简化逻辑：反向笔超过前一同向笔极值 → 线段被破坏
pub fn build_xd(bis: &[Bi]) -> Vec<XianDuan> {
    if bis.len() < 3 {
        return Vec::new();
    }

    // 使用特征序列法构建线段
    build_xd_by_feature_sequence(bis)
}

/// 特征序列法构建线段
///
/// 步骤：
/// 1. 对笔序列构造特征序列
/// 2. 对特征序列做包含处理
/// 3. 在特征序列上找分型，分型处即为线段端点
fn build_xd_by_feature_sequence(bis: &[Bi]) -> Vec<XianDuan> {
    let mut xds = Vec::new();
    let mut start_idx = 0; // 当前线段起始笔的索引

    while start_idx + 2 < bis.len() {
        // 当前线段方向由第一笔决定
        let xd_direction = bis[start_idx].direction.as_str();

        // 提取特征序列：与线段方向相反的笔
        let feature_indices: Vec<usize> = (start_idx..bis.len())
            .filter(|&i| bis[i].direction != xd_direction)
            .collect();

        if feature_indices.len() < 3 {
            // 特征序列不足3个元素，无法形成分型，线段延续到末尾
            break;
        }

        // 对特征序列做包含处理
        let (contained_features, _original_indices) = contain_feature_sequence(bis, &feature_indices);

        // 在包含处理后的特征序列上找分型
        let mut found_break = false;
        for i in 1..contained_features.len() - 1 {
            let prev = &contained_features[i - 1];
            let curr = &contained_features[i];
            let next = &contained_features[i + 1];

            let is_top = prev.high < curr.high && curr.high > next.high
                && prev.low < curr.low && curr.low > next.low;
            let is_bottom = prev.low > curr.low && curr.low < next.low
                && prev.high > curr.high && curr.high < next.high;

            // 上升线段被顶分型破坏，下降线段被底分型破坏
            let is_break = match xd_direction {
                "up" => is_top,
                "down" => is_bottom,
                _ => false,
            };

            if is_break {
                // 线段在 curr 处终结
                // 找到 curr 对应的原始笔索引
                let end_bi_idx = curr.bi_index;

                // 线段从 start_idx 到 end_bi_idx
                if end_bi_idx > start_idx + 1 {
                    // 至少3笔
                    let start_bi = &bis[start_idx];
                    let end_bi = &bis[end_bi_idx];

                    xds.push(XianDuan {
                        direction: xd_direction.to_string(),
                        start_index: start_bi.start_index,
                        end_index: end_bi.end_index,
                        start_dt: start_bi.start_dt.clone(),
                        end_dt: end_bi.end_dt.clone(),
                        start_price: start_bi.start_price,
                        end_price: end_bi.end_price,
                        is_finished: true,
                    });

                    start_idx = end_bi_idx + 1;
                    found_break = true;
                    break;
                }
            }
        }

        if !found_break {
            // 没有找到破坏分型，线段延续到末尾
            break;
        }
    }

    // 处理最后一段未完成的线段
    if start_idx + 2 < bis.len() {
        let start_bi = &bis[start_idx];
        let end_bi = &bis[bis.len() - 1];
        xds.push(XianDuan {
            direction: start_bi.direction.clone(),
            start_index: start_bi.start_index,
            end_index: end_bi.end_index,
            start_dt: start_bi.start_dt.clone(),
            end_dt: end_bi.end_dt.clone(),
            start_price: start_bi.start_price,
            end_price: end_bi.end_price,
            is_finished: false,
        });
    }

    xds
}

/// 特征序列元素
struct FeatureElement {
    /// high = 这笔的 max(起点价, 终点价)
    high: f64,
    /// low = 这笔的 min(起点价, 终点价)
    low: f64,
    /// 对应原始笔序列中的索引
    bi_index: usize,
}

/// 对特征序列做包含处理
///
/// 与 K 线去包含逻辑相同：
/// - 向上序列中取高高
/// - 向下序列中取低低
fn contain_feature_sequence(
    bis: &[Bi],
    feature_indices: &[usize],
) -> (Vec<FeatureElement>, Vec<usize>) {
    if feature_indices.is_empty() {
        return (Vec::new(), Vec::new());
    }

    let to_feature = |idx: usize| -> FeatureElement {
        let bi = &bis[idx];
        let high = bi.start_price.max(bi.end_price);
        let low = bi.start_price.min(bi.end_price);
        FeatureElement {
            high,
            low,
            bi_index: idx,
        }
    };

    let mut result = vec![to_feature(feature_indices[0])];
    let mut original_map = vec![feature_indices[0]];

    for &fi in &feature_indices[1..] {
        let k3 = to_feature(fi);

        if result.len() < 2 {
            result.push(k3);
            original_map.push(fi);
            continue;
        }

        let k1 = &result[result.len() - 2];
        let k2 = &result[result.len() - 1];

        // 判断方向
        let has_include = (k2.high <= k3.high && k2.low >= k3.low)
            || (k2.high >= k3.high && k2.low <= k3.low);

        if has_include {
            // 有包含，根据方向合并
            if k1.high < k2.high {
                // 向上：取高高
                let high = k2.high.max(k3.high);
                let low = k2.low.max(k3.low);
                let last = result.last_mut().unwrap();
                last.high = high;
                last.low = low;
                // bi_index 保留 k2 的（更早的那个）
            } else if k1.high > k2.high {
                // 向下：取低低
                let high = k2.high.min(k3.high);
                let low = k2.low.min(k3.low);
                let last = result.last_mut().unwrap();
                last.high = high;
                last.low = low;
            }
            // k1.high == k2.high 时不做包含处理
        } else {
            result.push(k3);
            original_map.push(fi);
        }
    }

    (result, original_map)
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
    fn test_build_xd_basic() {
        // 上-下-上-下 形成一段上升线段
        let bis = vec![
            make_bi(0, "up", 10.0, 15.0, 0, 3),
            make_bi(1, "down", 15.0, 12.0, 3, 6),
            make_bi(2, "up", 12.0, 18.0, 6, 9),
            make_bi(3, "down", 18.0, 11.0, 9, 12), // 破坏前一个上升笔起点
        ];

        let xds = build_xd(&bis);
        assert!(!xds.is_empty());
    }

    #[test]
    fn test_xd_min_3_bi() {
        // 少于3笔不成线段
        let bis = vec![
            make_bi(0, "up", 10.0, 15.0, 0, 3),
            make_bi(1, "down", 15.0, 12.0, 3, 6),
        ];
        let xds = build_xd(&bis);
        assert!(xds.is_empty(), "少于3笔不应有线段");
    }
}
