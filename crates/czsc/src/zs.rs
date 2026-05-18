//! 中枢识别
//!
//! **对齐 czsc Python 的 get_zs_seq + ZS.is_valid 逻辑**
//!
//! 缠论中枢定义（对齐 czsc Python 库）：
//! - 逐笔遍历，将笔归入"当前中枢"
//! - 脱离条件：向上笔 high < zd 或 向下笔 low > zg
//! - 不脱离时归入当前中枢
//! - 脱离时新开中枢
//! - 最终用 is_valid 过滤无效中枢：
//!   1. 包含至少3笔
//!   2. zg >= zd（上沿不低于下沿）
//!   3. 中枢内每笔都必须与 [zd, zg] 有交集
//! - zg = min(前3笔 high), zd = max(前3笔 low)
//! - gg = max(所有笔 high), dd = min(所有笔 low)
//!
//! **与旧算法的关键区别**：
//! 旧算法每次取3笔尝试构建中枢再扩展，新算法逐笔归入，
//! 只在脱离时新开中枢，这更符合缠论原始定义和 czsc 实现。

use yifang_data::{Bi, XianDuan, ZhongShu};

/// 从笔序列识别笔中枢
pub fn build_bi_zs(bis: &[Bi]) -> Vec<ZhongShu> {
    if bis.len() < 3 {
        return Vec::new();
    }

    let bi_high = |bi: &Bi| bi.start_price.max(bi.end_price);
    let bi_low = |bi: &Bi| bi.start_price.min(bi.end_price);

    build_zs_inner(
        bis,
        "bi_zs",
        bi_high,
        bi_low,
        |bi| &bi.direction,
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
        |xd: &XianDuan| &xd.direction,
        |xd| xd.start_index,
        |xd| xd.end_index,
        |xd| xd.start_dt.clone(),
        |xd| xd.end_dt.clone(),
    )
}

/// 检查一段的 [low, high] 是否与中枢 [zd, zg] 有交集
///
/// 对齐 czsc ZS.is_valid 中的逻辑
fn has_overlap_with_zs(high: f64, low: f64, zd: f64, zg: f64) -> bool {
    (zd <= high && high <= zg)
        || (zd <= low && low <= zg)
        || (high >= zg && zg > zd && zd >= low)
}

/// 通用的中枢构建函数
///
/// 对齐 czsc get_zs_seq 算法：
/// 1. 逐段遍历
/// 2. 每段归入当前中枢
/// 3. 脱离条件：向上段 high < zd（向下脱离）或 向下段 low > zg（向上脱离）
/// 4. 脱离时新开中枢
/// 5. 最后用 is_valid 过滤无效中枢（>=3段、zg>=zd、每笔与中枢有交集）
fn build_zs_inner<T, FH, FL, GD, SI, EI, SF, DF>(
    segments: &[T],
    zs_type: &str,
    get_high: FH,
    get_low: FL,
    get_direction: GD,
    get_start_idx: SI,
    get_end_idx: EI,
    get_start_dt: SF,
    get_end_dt: DF,
) -> Vec<ZhongShu>
where
    FH: Fn(&T) -> f64,
    FL: Fn(&T) -> f64,
    GD: Fn(&T) -> &str,
    SI: Fn(&T) -> u64,
    EI: Fn(&T) -> u64,
    SF: Fn(&T) -> String,
    DF: Fn(&T) -> String,
{
    // 第一阶段：按脱离条件分组（对齐 czsc get_zs_seq）
    struct ZsCandidate {
        start: usize,
        end: usize,
    }

    let mut candidates: Vec<ZsCandidate> = Vec::new();

    for idx in 0..segments.len() {
        let dir = get_direction(&segments[idx]);

        if candidates.is_empty() {
            candidates.push(ZsCandidate {
                start: idx,
                end: idx,
            });
            continue;
        }

        let cur = candidates.last_mut().unwrap();

        // 当前中枢候选至少有3段时，才能计算 zg/zd 做脱离判断
        if cur.end - cur.start + 1 >= 3 {
            let zg = (cur.start..cur.start + 3)
                .map(|i| get_high(&segments[i]))
                .fold(f64::INFINITY, f64::min);
            let zd = (cur.start..cur.start + 3)
                .map(|i| get_low(&segments[i]))
                .fold(f64::NEG_INFINITY, f64::max);

            // 脱离条件（对齐 czsc get_zs_seq）：
            // 向上段且 high < zd → 向下脱离
            // 向下段且 low > zg → 向上脱离
            let is_breakaway = (dir == "up" && get_high(&segments[idx]) < zd)
                || (dir == "down" && get_low(&segments[idx]) > zg);

            if is_breakaway {
                candidates.push(ZsCandidate {
                    start: idx,
                    end: idx,
                });
            } else {
                cur.end = idx;
            }
        } else {
            // 不足3段，无法判断，直接归入
            cur.end = idx;
        }
    }

    // 第二阶段：转换为 ZhongShu，并过滤无效中枢
    let mut zs_list = Vec::new();

    for cand in &candidates {
        let seg_count = cand.end - cand.start + 1;

        // 至少需要3段
        if seg_count < 3 {
            continue;
        }

        // zg = min(前3段 high), zd = max(前3段 low)
        let zg = (cand.start..cand.start + 3)
            .map(|i| get_high(&segments[i]))
            .fold(f64::INFINITY, f64::min);
        let zd = (cand.start..cand.start + 3)
            .map(|i| get_low(&segments[i]))
            .fold(f64::NEG_INFINITY, f64::max);

        // gg = max(所有段 high), dd = min(所有段 low)
        let gg = (cand.start..=cand.end)
            .map(|i| get_high(&segments[i]))
            .fold(f64::NEG_INFINITY, f64::max);
        let dd = (cand.start..=cand.end)
            .map(|i| get_low(&segments[i]))
            .fold(f64::INFINITY, f64::min);

        // is_valid 检查 1: zg >= zd
        if zg < zd {
            continue;
        }

        // is_valid 检查 2: 每段都必须与 [zd, zg] 有交集
        let mut valid = true;
        for i in cand.start..=cand.end {
            let h = get_high(&segments[i]);
            let l = get_low(&segments[i]);
            if !has_overlap_with_zs(h, l, zd, zg) {
                valid = false;
                break;
            }
        }
        if !valid {
            continue;
        }

        zs_list.push(ZhongShu {
            zs_type: zs_type.to_string(),
            start_index: get_start_idx(&segments[cand.start]),
            end_index: get_end_idx(&segments[cand.end]),
            start_dt: get_start_dt(&segments[cand.start]),
            end_dt: get_end_dt(&segments[cand.end]),
            zg,
            zd,
            gg,
            dd,
        });
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
        assert_eq!(zs[0].zg, 14.0);
        assert_eq!(zs[0].zd, 12.0);
        assert!(zs[0].zg > zs[0].zd, "中枢上沿应大于下沿");
    }

    #[test]
    fn test_no_zs_when_no_overlap() {
        // 3笔无重叠区间
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
        // 中枢扩展：后续笔仍在中枢区间内
        // 笔0-2: 中枢 [12,14]
        // 笔3: down, low=13 → 不脱离（low=13 < zg=14，不满足 low > zg）
        // 笔4: up, high=18 → 不脱离（high=18 > zd=12，不满足 high < zd）
        // 但 is_valid: 笔4 [13,18] 与 [12,14] → (12<=13 && 18>=14) → 有交集，OK
        let bis = vec![
            make_bi(0, "up", 10.0, 15.0),
            make_bi(1, "down", 15.0, 12.0),
            make_bi(2, "up", 12.0, 14.0),
            make_bi(3, "down", 14.0, 13.0),
            make_bi(4, "up", 13.0, 18.0),
        ];

        let zs = build_bi_zs(&bis);
        assert_eq!(zs.len(), 1);
        assert_eq!(zs[0].start_index, 0);
    }

    #[test]
    fn test_zs_gg_dd() {
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

    #[test]
    fn test_zs_breakaway() {
        // 笔0-2: 中枢 [12,14]
        // 笔3: down, low=8 → down 且 low=8 < zg=14，不满足 low > zg，不脱离
        //   [8,14] 与 [12,14] 有交集 → is_valid OK
        // 笔4: up, high=11 < zd=12 → 向上笔 high < zd，脱离！新开中枢
        //   新中枢候选只有1笔（笔4），不够3笔
        let bis = vec![
            make_bi(0, "up", 10.0, 15.0),
            make_bi(1, "down", 15.0, 12.0),
            make_bi(2, "up", 12.0, 14.0),
            make_bi(3, "down", 14.0, 8.0),
            make_bi(4, "up", 8.0, 11.0),
        ];

        let zs = build_bi_zs(&bis);
        // 一个有效中枢包含笔0-3
        assert_eq!(zs.len(), 1);
        // 笔3 的 end_index = (3+3) = 6
        assert_eq!(zs[0].end_index, 6);
    }

    #[test]
    fn test_two_zhongshu() {
        // 笔0-2: 中枢 [12,14]
        // 笔3: down 20→14 → down, low=14, high=20; low=14 不 > zg=14, 不脱离
        //   但是 [14,20] 与 [12,14] → 14<=14<=14 → overlap ✓
        // 笔4: up 14→5 → 笔方向=up 但价格下跌，这不符合缠论
        //
        // 重新设计：用合理的笔方向
        // 笔0: up   10→15, high=15, low=10
        // 笔1: down 15→12, high=15, low=12
        // 笔2: up   12→14, high=14, low=12  ZS1: zg=14, zd=12
        // 笔3: down 14→11, high=14, low=11  up笔 high=14 >= zd=12, 不脱离
        //   但 [11,14] 与 [12,14] → 14<=14 → overlap ✓
        // 笔4: up   11→8  → up 且 high=8 < zd=12 → 脱离！
        //   新中枢候选 [4,...]
        // 但笔方向=up 价格从11降到8不合理。换一种：
        //
        // 正确的脱离方式：向上脱离中枢
        // 笔3: down 14→6  high=14, low=6  down, low=6 > zg=14? No, 不脱离
        //   [6,14] 与 [12,14] → (6<=12 && 14>=14) → overlap ✓
        // 笔4: up   6→20   high=20, low=6  up, high=20 < zd=12? No, 不脱离
        //   [6,20] 与 [12,14] → (6<=12 && 20>=14) → overlap ✓
        // 笔5: down 20→16  high=20, low=16 down, low=16 > zg=14 → YES! 向上脱离
        // 新候选从笔5开始 [5,6]，不足3段
        let bis = vec![
            make_bi(0, "up", 10.0, 15.0),
            make_bi(1, "down", 15.0, 12.0),
            make_bi(2, "up", 12.0, 14.0),
            make_bi(3, "down", 14.0, 6.0),
            make_bi(4, "up", 6.0, 20.0),
            make_bi(5, "down", 20.0, 16.0),  // 脱离
            make_bi(6, "up", 16.0, 25.0),
        ];

        let zs = build_bi_zs(&bis);
        // 一个有效中枢包含笔0-4
        assert_eq!(zs.len(), 1);
        assert_eq!(zs[0].zg, 14.0);
        assert_eq!(zs[0].zd, 12.0);
    }

    #[test]
    fn test_two_valid_zhongshu() {
        // 构造真正有两个有效中枢的序列
        // 笔0-4: 第一个中枢 [12,14]，扩展到笔4
        // 笔5: down 18→16 → down, low=16 > zg=14 → 脱离
        // 笔5-7: 第二个中枢
        //   bi[5]: down 18→16, high=18, low=16
        //   bi[6]: up   16→20, high=20, low=16
        //   bi[7]: down 20→17, high=20, low=17
        //   zg=min(18,20,20)=18, zd=max(16,16,17)=17
        //   zg=18 > zd=17 → 有效！
        let bis = vec![
            make_bi(0, "up", 10.0, 15.0),   // high=15, low=10
            make_bi(1, "down", 15.0, 12.0),  // high=15, low=12
            make_bi(2, "up", 12.0, 14.0),    // high=14, low=12  ZS1: zg=14, zd=12
            make_bi(3, "down", 14.0, 13.0),  // high=14, low=13  在[12,14]内
            make_bi(4, "up", 13.0, 18.0),    // high=18, low=13  [13,18]与[12,14]有交集
            make_bi(5, "down", 18.0, 16.0),  // down, low=16 > zg=14 → 脱离
            make_bi(6, "up", 16.0, 20.0),    // high=20, low=16
            make_bi(7, "down", 20.0, 17.0),  // high=20, low=17  ZS2: zg=18, zd=17
        ];

        let zs = build_bi_zs(&bis);
        assert_eq!(zs.len(), 2, "应该有2个有效中枢");
        assert_eq!(zs[0].zg, 14.0);
        assert_eq!(zs[0].zd, 12.0);
        assert_eq!(zs[1].zg, 18.0);
        assert_eq!(zs[1].zd, 17.0);
    }
}
