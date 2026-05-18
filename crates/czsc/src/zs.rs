//! 中枢识别
//!
//! **严格遵循缠论原始定义**：
//!
//! 中枢 = 连续三段（或N段）的公共重叠区间。
//! - zg（中枢高）= 所有段 high 的最小值
//! - zd（中枢低）= 所有段 low 的最大值
//! - zg 和 zd 随新段加入**动态重算**，始终取所有段的交集
//! - 脱离条件：新段与当前 [zd, zg] **完全无交集**
//!   - `段.low > zg` → 向上脱离（整段在中枢上方）
//!   - `段.high < zd` → 向下脱离（整段在中枢下方）
//!
//! gg（高高）= 所有段 high 的最大值（段能到达的最高点）
//! dd（低低）= 所有段 low 的最小值（段能到达的最低点）

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
        |xd: &XianDuan| xd.start_index,
        |xd| xd.end_index,
        |xd| xd.start_dt.clone(),
        |xd| xd.end_dt.clone(),
    )
}

/// 检查两区间是否重叠
#[inline]
fn intervals_overlap(low: f64, high: f64, zd: f64, zg: f64) -> bool {
    low <= zg && high >= zd
}

/// 通用的中枢构建函数
///
/// ### 算法
///
/// 1. 顺序遍历所有段
/// 2. 收集够3段后，计算它们的交集 [zd, zg]；若无交集则候选无效
/// 3. 后续段：
///    - 与 [zd, zg] 有交集 → 归入中枢，**重算**所有段的交集作为新 [zd, zg]
///    - 与 [zd, zg] 无交集 → 脱离，创建新候选
/// 4. 最终过滤：>=3段、zg >= zd、每段与 [zd, zg] 都有交集（自动满足，验证用）
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
    // 第一阶段：按是否重叠依次分组
    struct ZsCandidate {
        start: usize,
        end: usize,
        zg: f64, // 当前所有段的交集上沿（min high）
        zd: f64, // 当前所有段的交集下沿（max low）
    }

    let mut candidates: Vec<ZsCandidate> = Vec::new();

    for idx in 0..segments.len() {
        let h = get_high(&segments[idx]);
        let l = get_low(&segments[idx]);

        if candidates.is_empty() {
            candidates.push(ZsCandidate {
                start: idx,
                end: idx,
                zg: h,
                zd: l,
            });
            continue;
        }

        let cur = candidates.last().unwrap();
        let seg_count = cur.end - cur.start + 1;

        if seg_count < 3 {
            // 不足3段，无法形成有效中枢
            // 但需要收集段以便后续判断是否构成中枢
            let cur = candidates.last_mut().unwrap();
            cur.end = idx;
            cur.zg = cur.zg.min(h);
            cur.zd = cur.zd.max(l);
        } else {
            // >=3段，已有有效的交集 [zd, zg]
            let cur_zg = cur.zg;
            let cur_zd = cur.zd;

            // 检查当前段是否与中枢交集重叠
            if intervals_overlap(l, h, cur_zd, cur_zg) {
                // 归入中枢，重算所有段的交集
                let cur = candidates.last_mut().unwrap();
                cur.end = idx;

                // 重算 [zd, zg] = 所有段的交集
                let mut new_zg = f64::INFINITY;
                let mut new_zd = f64::NEG_INFINITY;
                for j in cur.start..=cur.end {
                    let hj = get_high(&segments[j]);
                    let lj = get_low(&segments[j]);
                    new_zg = new_zg.min(hj);
                    new_zd = new_zd.max(lj);
                }
                cur.zg = new_zg;
                cur.zd = new_zd;
            } else {
                // 脱离中枢，新开候选
                candidates.push(ZsCandidate {
                    start: idx,
                    end: idx,
                    zg: h,
                    zd: l,
                });
            }
        }
    }

    // 第二阶段：过滤无效中枢 + 计算 gg/dd
    let mut zs_list = Vec::new();

    for cand in &candidates {
        let seg_count = cand.end - cand.start + 1;

        // 至少需要3段
        if seg_count < 3 {
            continue;
        }

        let zg = cand.zg;
        let zd = cand.zd;

        // zg >= zd 才有效（前3段必须有交集）
        if zg < zd {
            continue;
        }

        // gg = max(所有段 high), dd = min(所有段 low)
        let gg = (cand.start..=cand.end)
            .map(|i| get_high(&segments[i]))
            .fold(f64::NEG_INFINITY, f64::max);
        let dd = (cand.start..=cand.end)
            .map(|i| get_low(&segments[i]))
            .fold(f64::INFINITY, f64::min);

        // 验证：每段都必须与 [zd, zg] 有交集（按构造应自动满足）
        let mut valid = true;
        for i in cand.start..=cand.end {
            let h = get_high(&segments[i]);
            let l = get_low(&segments[i]);
            if !intervals_overlap(l, h, zd, zg) {
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
            start_price: start,
            end_price: end,
            start_index: (id * 3) as u64,
            end_index: (id * 3 + 3) as u64,
            start_dt: format!("2024-01-{:02}", id + 1),
            end_dt: format!("2024-01-{:02}", id + 2),
            is_finished: true,
        }
    }

    #[test]
    fn test_build_bi_zs_basic() {
        // 3笔有重叠区间
        // 笔0: up  10→15, high=15, low=10
        // 笔1: down 15→12, high=15, low=12
        // 笔2: up  12→14, high=14, low=12
        // 交集: zd=max(10,12,12)=12, zg=min(15,15,14)=14 → [12,14]
        let bis = vec![
            make_bi(0, "up", 10.0, 15.0),
            make_bi(1, "down", 15.0, 12.0),
            make_bi(2, "up", 12.0, 14.0),
        ];

        let zs = build_bi_zs(&bis);
        assert_eq!(zs.len(), 1);
        assert_eq!(zs[0].zg, 14.0);
        assert_eq!(zs[0].zd, 12.0);
    }

    #[test]
    fn test_no_zs_when_no_overlap() {
        // 3笔无重叠
        // 笔0: up   10→15, high=15, low=10
        // 笔1: down 15→20, high=20, low=15  → 与笔0无重叠
        // 笔2: up   20→25, high=25, low=20  → 与笔1无重叠
        let bis = vec![
            make_bi(0, "up", 10.0, 15.0),
            make_bi(1, "down", 15.0, 20.0),
            make_bi(2, "up", 20.0, 25.0),
        ];

        let zs = build_bi_zs(&bis);
        assert!(zs.is_empty(), "无重叠区间不应有中枢");
    }

    #[test]
    fn test_zs_extension() {
        // 中枢延伸测试
        // 笔0: up   10→15, high=15, low=10
        // 笔1: down 15→12, high=15, low=12
        // 笔2: up   12→14, high=14, low=12  前3笔交: [12,14]
        // 笔3: down 14→13, high=14, low=13  在[12,14]内 → 归入
        //   重算 4笔交: max(10,12,12,13)=13, min(15,15,14,14)=14 → [13,14]
        // 笔4: up   13→18, high=18, low=13  与[13,14]有交集 → 归入
        //   重算 5笔交: max(10,12,12,13,13)=13, min(15,15,14,14,18)=14 → [13,14]
        let bis = vec![
            make_bi(0, "up", 10.0, 15.0),   // high=15, low=10
            make_bi(1, "down", 15.0, 12.0),  // high=15, low=12
            make_bi(2, "up", 12.0, 14.0),    // high=14, low=12
            make_bi(3, "down", 14.0, 13.0),  // high=14, low=13
            make_bi(4, "up", 13.0, 18.0),    // high=18, low=13
        ];

        let zs = build_bi_zs(&bis);
        assert_eq!(zs.len(), 1);
        assert_eq!(zs[0].start_index, 0);
        assert_eq!(zs[0].zg, 14.0, "动态zg: 5笔min high=min(15,15,14,14,18)=14");
        assert_eq!(zs[0].zd, 13.0, "动态zd: 5笔max low=max(10,12,12,13,13)=13");
    }

    #[test]
    fn test_zs_gg_dd() {
        // gg/dd 计算测试
        let bis = vec![
            make_bi(0, "up", 10.0, 15.0),   // high=15, low=10
            make_bi(1, "down", 15.0, 12.0),  // high=15, low=12
            make_bi(2, "up", 12.0, 14.0),    // high=14, low=12
        ];

        let zs = build_bi_zs(&bis);
        assert_eq!(zs.len(), 1);
        assert_eq!(zs[0].gg, 15.0); // max(15, 15, 14)
        assert_eq!(zs[0].dd, 10.0); // min(10, 12, 12)
    }

    #[test]
    fn test_zs_breakaway() {
        // 脱离测试（严格缠论定义：段与中枢完全无交集）
        // 笔0: up   10→15, high=15, low=10
        // 笔1: down 15→12, high=15, low=12
        // 笔2: up   12→14, high=14, low=12  前3笔交: [12,14]
        // 笔3: down 14→8,  high=14, low=8   与[12,14]有交集 → 归入
        //   重算: zd=max(10,12,12,8)=12, zg=min(15,15,14,14)=14 → [12,14]
        // 笔4: up   8→20,  high=20, low=8   与[12,14]有交集 → 归入
        //   重算: zd=max(10,12,12,8,8)=12, zg=min(15,15,14,14,20)=14 → [12,14]
        // 笔5: down 20→16, high=20, low=16  low=16 > zg=14 → 脱离！
        // 新候选从笔5开始 [5,6]，不足3段
        let bis = vec![
            make_bi(0, "up", 10.0, 15.0),
            make_bi(1, "down", 15.0, 12.0),
            make_bi(2, "up", 12.0, 14.0),
            make_bi(3, "down", 14.0, 8.0),
            make_bi(4, "up", 8.0, 20.0),
            make_bi(5, "down", 20.0, 16.0),  // 脱离
            make_bi(6, "up", 16.0, 25.0),
        ];

        let zs = build_bi_zs(&bis);
        // 一个有效中枢包含笔0-4（笔5脱离，笔5-6不足3段）
        assert_eq!(zs.len(), 1);
        assert_eq!(zs[0].end_index, (4 * 3 + 3) as u64); // 笔4的 end_index
    }

    #[test]
    fn test_two_zhongshu() {
        // 两个中枢测试（笔5向上脱离后形成第二个中枢）
        // 笔0: up   10→15, high=15, low=10
        // 笔1: down 15→12, high=15, low=12
        // 笔2: up   12→14, high=14, low=12  ZS1: 前3笔交 [12,14]
        // 笔3: down 14→6,  high=14, low=6   与[12,14]有交集 → 归入
        //   重算: zd=max(10,12,12,6)=12, zg=min(15,15,14,14)=14 → [12,14]
        // 笔4: up   6→20,  high=20, low=6   与[12,14]有交集 → 归入
        //   重算: zd=12, zg=14 → [12,14]
        // 笔5: down 20→16, high=20, low=16  low=16 > zg=14 → 脱离！
        // 笔6: up   16→25, high=25, low=16  新候选 [5,6]，不足3段
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
        // 一个有效中枢（笔5-6不足3段没有第二个中枢）
        assert_eq!(zs.len(), 1);
        assert_eq!(zs[0].zg, 14.0);
        assert_eq!(zs[0].zd, 12.0);
    }

    #[test]
    fn test_two_valid_zhongshu() {
        // 构造有两个有效中枢的序列
        // 笔0: up   10→15, high=15, low=10
        // 笔1: down 15→12, high=15, low=12
        // 笔2: up   12→14, high=14, low=12  前3笔交: [12,14]
        // 笔3: down 14→13, high=14, low=13  与[12,14]有交集 → 归入
        //   重算: zd=13, zg=14 → [13,14]
        // 笔4: up   13→18, high=18, low=13  与[13,14]有交集 → 归入
        //   重算: zd=13, zg=14 → [13,14]
        // 笔5: down 18→16, high=18, low=16  low=16 > zg=14 → 脱离！
        // 笔6: up   16→20, high=20, low=16
        // 笔7: down 20→17, high=20, low=17  ZS2: 前3笔交 [17,18]
        let bis = vec![
            make_bi(0, "up", 10.0, 15.0),   // high=15, low=10
            make_bi(1, "down", 15.0, 12.0),  // high=15, low=12
            make_bi(2, "up", 12.0, 14.0),    // high=14, low=12  ZS1: [12,14]
            make_bi(3, "down", 14.0, 13.0),  // high=14, low=13  归入 → [13,14]
            make_bi(4, "up", 13.0, 18.0),    // high=18, low=13  归入 → [13,14]
            make_bi(5, "down", 18.0, 16.0),  // 脱离（low=16 > zg=14）
            make_bi(6, "up", 16.0, 20.0),    // high=20, low=16
            make_bi(7, "down", 20.0, 17.0),  // high=20, low=17  ZS2: [17,18]
        ];

        let zs = build_bi_zs(&bis);
        assert_eq!(zs.len(), 2, "应该有2个有效中枢");
        // ZS1: 笔0-4的动态交集
        assert_eq!(zs[0].zg, 14.0, "ZS1 zg = min(15,15,14,14,18) = 14");
        assert_eq!(zs[0].zd, 13.0, "ZS1 zd = max(10,12,12,13,13) = 13");
        // ZS2: 笔5-7的交集
        assert_eq!(zs[1].zg, 18.0, "ZS2 zg = min(18,20,20) = 18");
        assert_eq!(zs[1].zd, 17.0, "ZS2 zd = max(16,16,17) = 17");
    }

    #[test]
    fn test_zs_zone_tightens_on_entry() {
        // 测试中枢区间动态收紧
        // 笔0: up   10→20, high=20, low=10
        // 笔1: down 20→15, high=20, low=15
        // 笔2: up   15→18, high=18, low=15  前3笔交: [15,18]
        // 笔3: down 18→16, high=18, low=16  在[15,18]内
        //   重算: zd=max(10,15,15,16)=16, zg=min(20,20,18,18)=18 → [16,18]
        // 笔4: up   16→17, high=17, low=16  在[16,18]内
        //   重算: zd=16, zg=min(20,20,18,18,17)=17 → [16,17]
        // 笔5: down 17→16.5, high=17, low=16.5  在[16,17]内
        //   重算: zd=16.5, zg=17 → [16.5, 17]
        let bis = vec![
            make_bi(0, "up", 10.0, 20.0),
            make_bi(1, "down", 20.0, 15.0),
            make_bi(2, "up", 15.0, 18.0),
            make_bi(3, "down", 18.0, 16.0),
            make_bi(4, "up", 16.0, 17.0),
            make_bi(5, "down", 17.0, 16.5),
        ];

        let zs = build_bi_zs(&bis);
        assert_eq!(zs.len(), 1);
        // 动态交集: 6笔
        // zg = min(20,20,18,18,17,17) = 17
        // zd = max(10,15,15,16,16,16.5) = 16.5
        assert_eq!(zs[0].zg, 17.0);
        assert_eq!(zs[0].zd, 16.5);
    }

    #[test]
    fn test_no_zs_mid_break_and_continue() {
        // 前3笔无交集，但后续3笔有交集
        // 笔0: up   10→20, high=20, low=10
        // 笔1: down 20→25, high=25, low=20  → 与笔0无交集
        // 笔2: up   25→30, high=30, low=25  → 前3笔完全无交集
        // 笔3: down 30→28, high=30, low=28
        // 笔4: up   28→32, high=32, low=28
        // 笔5: down 32→29, high=32, low=29  → 笔3-5交 [29,30]
        let bis = vec![
            make_bi(0, "up", 10.0, 20.0),
            make_bi(1, "down", 20.0, 25.0),
            make_bi(2, "up", 25.0, 30.0),
            make_bi(3, "down", 30.0, 28.0),
            make_bi(4, "up", 28.0, 32.0),
            make_bi(5, "down", 32.0, 29.0),
        ];

        let zs = build_bi_zs(&bis);
        // 前3笔无交集 → 候选无效
        // 笔3-5形成第二个候选 → 有效
        assert_eq!(zs.len(), 1);
        assert_eq!(zs[0].start_index, (3 * 3) as u64); // 笔3
        assert_eq!(zs[0].end_index, (5 * 3 + 3) as u64); // 笔5
        assert_eq!(zs[0].zg, 30.0); // min(30,32,32)=30
        assert_eq!(zs[0].zd, 29.0); // max(28,28,29)=29
    }
}
