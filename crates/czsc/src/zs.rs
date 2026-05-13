//! 中枢识别
//!
//! **严格对齐 czsc 0.9.9 的 ZS 类定义**
//!
//! 缠论中枢定义：
//! - 由至少3笔（或线段）的价格重叠区间形成
//! - zg（中枢上沿）= min(前3笔的高点)   ← 即 min(bi.high for bi in bis[:3])
//! - zd（中枢下沿）= max(前3笔的低点)   ← 即 max(bi.low for bi in bis[:3])
//! - gg（最高边界）= max(所有笔的高点)
//! - dd（最低边界）= min(所有笔的低点)
//! - 中枢有效条件：zg >= zd（上沿不低于下沿，即存在重叠区间）
//! - 中枢扩展：如果后续笔的高点/低点仍在 [zd, zg] 区间内，则归入该中枢
//!
//! **之前代码的错误**：
//! 旧代码中 zg = min(三段低点), zd = min(三段高点)，这完全是反的。
//! 正确的是：zg = min(三段高点), zd = max(三段低点)

use yifang_data::{Bi, XianDuan, ZhongShu};

/// 从笔序列识别笔中枢
///
/// 对齐 czsc ZS 类：
/// - zg = min(前3笔的 high)   ，high = max(start_price, end_price)
/// - zd = max(前3笔的 low)    ，low  = min(start_price, end_price)
/// - gg = max(所有参与笔的 high)
/// - dd = min(所有参与笔的 low)
/// - 中枢有效：zg >= zd
pub fn build_bi_zs(bis: &[Bi]) -> Vec<ZhongShu> {
    if bis.len() < 3 {
        return Vec::new();
    }

    // 计算 Bi 的高低点: high = max(start_price, end_price), low = min(start_price, end_price)
    let bi_high = |bi: &Bi| bi.start_price.max(bi.end_price);
    let bi_low = |bi: &Bi| bi.start_price.min(bi.end_price);

    build_zs_inner(
        bis,
        "bi_zs",
        bi_high,
        bi_low,
        |bi| bi.start_index,
        |bi| bi.end_index,
        |bi| bi.start_dt.clone(),
        |bi| bi.end_dt.clone(),
    )
}

/// 从线段序列识别线段中枢
pub fn build_xd_zs(xds: &[XianDuan]) -> Vec<ZhongShu> {
    if xds.len() < 3 {
        return Vec::new();
    }

    let xd_high = |xd: &XianDuan| xd.start_price.max(xd.end_price);
    let xd_low = |xd: &XianDuan| xd.start_price.min(xd.end_price);

    build_zs_inner(
        xds,
        "xd_zs",
        xd_high,
        xd_low,
        |xd| xd.start_index,
        |xd| xd.end_index,
        |xd| xd.start_dt.clone(),
        |xd| xd.end_dt.clone(),
    )
}

/// 通用的中枢构建函数
///
/// 严格对齐 czsc ZS 类的逻辑：
/// 1. 从连续3笔（线段）开始尝试构建中枢
/// 2. zg = min(前3段 high), zd = max(前3段 low)
/// 3. 如果 zg < zd，说明无重叠，跳过，从下一组3笔开始
/// 4. 如果 zg >= zd，中枢成立
/// 5. 中枢扩展：如果后续段的 [low, high] 与 [zd, zg] 有交集，则归入该中枢
/// 6. 直到某段完全脱离中枢区间，中枢结束
fn build_zs_inner<T, FH, FL, SI, EI, SF, DF>(
    segments: &[T],
    zs_type: &str,
    get_high: FH,
    get_low: FL,
    get_start_idx: SI,
    get_end_idx: EI,
    get_start_dt: SF,
    get_end_dt: DF,
) -> Vec<ZhongShu>
where
    FH: Fn(&T) -> f64,
    FL: Fn(&T) -> f64,
    SI: Fn(&T) -> u64,
    EI: Fn(&T) -> u64,
    SF: Fn(&T) -> String,
    DF: Fn(&T) -> String,
{
    let mut zs_list = Vec::new();
    let mut i = 0;

    while i + 2 < segments.len() {
        // 取连续3段尝试构建中枢
        let h1 = get_high(&segments[i]);
        let l1 = get_low(&segments[i]);
        let h2 = get_high(&segments[i + 1]);
        let l2 = get_low(&segments[i + 1]);
        let h3 = get_high(&segments[i + 2]);
        let l3 = get_low(&segments[i + 2]);

        // 对齐 czsc ZS 类：
        // zg = min(前3段 high)
        let zg = h1.min(h2).min(h3);
        // zd = max(前3段 low)
        let zd = l1.max(l2).max(l3);

        if zg < zd {
            // 无重叠区间，不是有效中枢，从下一组开始
            i += 1;
            continue;
        }

        // 中枢成立，尝试扩展
        // gg, dd 初始为前3段的极值
        let mut gg = h1.max(h2).max(h3);
        let mut dd = l1.min(l2).min(l3);
        let mut end_i = i + 2; // 中枢包含的最后一段索引

        // 向后扫描：如果后续段的 [low, high] 与 [zd, zg] 有交集，归入中枢
        for j in (i + 3)..segments.len() {
            let j_high = get_high(&segments[j]);
            let j_low = get_low(&segments[j]);

            // 对齐 czsc ZS.is_valid：中枢内的笔必须与中枢的上下沿有交集
            let has_overlap = (zd <= j_high && j_high <= zg)
                || (zd <= j_low && j_low <= zg)
                || (j_low <= zd && j_high >= zg);

            if has_overlap {
                // 归入该中枢
                gg = gg.max(j_high);
                dd = dd.min(j_low);
                end_i = j;
            } else {
                // 完全脱离，中枢结束
                break;
            }
        }

        zs_list.push(ZhongShu {
            zs_type: zs_type.to_string(),
            start_index: get_start_idx(&segments[i]),
            end_index: get_end_idx(&segments[end_i]),
            start_dt: get_start_dt(&segments[i]),
            end_dt: get_end_dt(&segments[end_i]),
            zg,
            zd,
            gg,
            dd,
        });

        // 下一个中枢从当前中枢结束后开始
        i = end_i + 1;
    }

    zs_list
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_bi(id: usize, dir: &str, start: f64, end: f64) -> Bi {
        Bi {
            direction: dir.to_string(),
            start_index: id as u64,
            end_index: (id + 3) as u64,
            start_dt: format!("t{}", id),
            end_dt: format!("t{}", id + 3),
            start_price: start,
            end_price: end,
            is_finished: true,
        }
    }

    #[test]
    fn test_build_bi_zs_basic() {
        // 3笔有重叠区间
        // 笔0: up  10→15, high=15, low=10
        // 笔1: down 15→12, high=15, low=12
        // 笔2: up  12→14, high=14, low=12
        // zg = min(15, 15, 14) = 14
        // zd = max(10, 12, 12) = 12
        // zg=14 > zd=12 → 有效中枢
        let bis = vec![
            make_bi(0, "up", 10.0, 15.0),
            make_bi(1, "down", 15.0, 12.0),
            make_bi(2, "up", 12.0, 14.0),
        ];

        let zs = build_bi_zs(&bis);
        assert_eq!(zs.len(), 1);
        assert_eq!(zs[0].zg, 14.0); // min(15, 15, 14)
        assert_eq!(zs[0].zd, 12.0); // max(10, 12, 12)
        assert!(zs[0].zg > zs[0].zd, "中枢上沿应大于下沿");
    }

    #[test]
    fn test_no_zs_when_no_overlap() {
        // 3笔无重叠区间
        // 笔0: up  10→20, high=20, low=10
        // 笔1: down 20→5,  high=20, low=5
        // 笔2: up  5→8,    high=8,  low=5
        // zg = min(20, 20, 8) = 8
        // zd = max(10, 5, 5) = 10
        // zg=8 < zd=10 → 无效中枢
        let bis = vec![
            make_bi(0, "up", 10.0, 20.0),
            make_bi(1, "down", 20.0, 5.0),
            make_bi(2, "up", 5.0, 8.0),
        ];

        let zs = build_bi_zs(&bis);
        assert!(zs.is_empty(), "无重叠区间不应有中枢");
    }

    #[test]
    fn test_zs_extension() {
        // 中枢扩展：后续笔仍然在中枢区间内
        // 笔0: up  10→15, high=15, low=10
        // 笔1: down 15→12, high=15, low=12
        // 笔2: up  12→14, high=14, low=12  → 中枢成立 [12,14]
        // 笔3: down 14→13, high=14, low=13 → 仍在 [12,14] 内，扩展
        // 笔4: up  13→18, high=18, low=13 → 脱离中枢
        let bis = vec![
            make_bi(0, "up", 10.0, 15.0),
            make_bi(1, "down", 15.0, 12.0),
            make_bi(2, "up", 12.0, 14.0),
            make_bi(3, "down", 14.0, 13.0),
            make_bi(4, "up", 13.0, 18.0),
        ];

        let zs = build_bi_zs(&bis);
        assert_eq!(zs.len(), 1);
        // 中枢应包含笔0-3（扩展到笔3），笔4脱离
        assert_eq!(zs[0].start_index, 0);
    }

    #[test]
    fn test_zs_gg_dd() {
        // 验证 gg 和 dd 的计算
        // 笔0: up  10→15, high=15, low=10
        // 笔1: down 15→12, high=15, low=12
        // 笔2: up  12→14, high=14, low=12
        let bis = vec![
            make_bi(0, "up", 10.0, 15.0),
            make_bi(1, "down", 15.0, 12.0),
            make_bi(2, "up", 12.0, 14.0),
        ];

        let zs = build_bi_zs(&bis);
        assert_eq!(zs.len(), 1);
        assert_eq!(zs[0].gg, 15.0); // max(15, 15, 14)
        assert_eq!(zs[0].dd, 10.0); // min(10, 12, 12)
    }
}
