//! 线段分析
//!
//! 线段的定义：由至少 3 笔组成的走势，且满足特征序列破坏规则。
//! 线段被破坏的条件：出现与线段方向相反的笔，且该笔的终点破了前一笔的起点。

use yifang_data::{Bi, XianDuan};

/// 构建线段
///
/// 基于笔序列，使用特征序列法判断线段：
/// 1. 上升线段至少包含 3 笔（上-下-上）
/// 2. 下降线段至少包含 3 笔（下-上-下）
/// 3. 线段终结：反向笔的终点超过前一同向笔的起点
pub fn build_xd(bis: &[Bi]) -> Vec<XianDuan> {
    if bis.len() < 3 {
        return Vec::new();
    }

    let mut xds = Vec::new();
    let mut xd_start_idx = 0;

    for i in 2..bis.len() {
        let bi_curr = &bis[i];
        let bi_prev = &bis[i - 1];

        // 检查是否与前一笔方向相反（这是基本交替条件，已由笔的构建保证）
        // 检查线段终结条件
        if i >= 3 {
            let xd_direction = bis[xd_start_idx].direction.as_str();

            // 判断当前笔是否形成线段破坏
            let is_break = match xd_direction {
                "up" => {
                    // 上升线段被破坏：出现下降笔且终点低于前一个上升笔的起点
                    bi_curr.direction == "down"
                        && bi_curr.end_price < bi_prev.start_price
                }
                "down" => {
                    // 下降线段被破坏：出现上升笔且终点高于前一个下降笔的起点
                    bi_curr.direction == "up"
                        && bi_curr.end_price > bi_prev.end_price
                }
                _ => false,
            };

            if is_break && i - xd_start_idx >= 2 {
                // 至少3笔形成一段
                let start_bi = &bis[xd_start_idx];
                let end_bi = &bis[i - 1];

                xds.push(XianDuan {
                    direction: start_bi.direction.clone(),
                    start_index: start_bi.start_index,
                    end_index: end_bi.end_index,
                    start_dt: start_bi.start_dt.clone(),
                    end_dt: end_bi.end_dt.clone(),
                    start_price: start_bi.start_price,
                    end_price: end_bi.end_price,
                    is_finished: true,
                });

                xd_start_idx = i - 1;
            }
        }
    }

    // 处理最后一段未完成的线段
    if xd_start_idx < bis.len() - 2 {
        let start_bi = &bis[xd_start_idx];
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
}
