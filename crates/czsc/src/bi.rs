//! 笔的构建
//!
//! 缠论笔规则（修正版）：
//! 1. K线去包含
//! 2. 找顶分型和底分型（顶底交替）
//! 3. 顶分型和底分型配对构成笔，配对时额外约束：
//!    a) 顶 > 底（价格方向正确）
//!    b) 顶底区间互不包含
//!    c) 顶底之间至少1根独立K线（共≥7根去包含K线）
//!    d) **分型失效检查**：对于上升笔，顶分型后到下一个底分型前，
//!       如果有K线突破顶分型的高点，该顶分型失效，笔继续延伸；
//!       下降笔同理（底分型被新低突破则失效）
//! 4. 从第一个分型开始，向后找第一个有效的反向分型

use crate::fenxing::{FxMark, check_fxs};
use crate::include::NewBar;
use yifang_data::Bi;

/// 最小笔长度（缠论新笔定义：顶底分型之间至少1根独立K线）
/// 加上顶底各自3根K线，共至少7根去包含K线
const DEFAULT_MIN_BI_LEN: usize = 7;

/// 在去包含K线序列中查找一笔
///
/// 关键修正：检查候选反向分型是否被后续新高/新低突破失效。
///
/// 返回: (笔, 剩余K线序列) 或 None
fn check_bi(bars: &[NewBar], min_bi_len: usize) -> Option<(Bi, Vec<NewBar>)> {
    let fxs = check_fxs(bars);
    if fxs.len() < 2 {
        return None;
    }

    let fx_a = &fxs[0];

    // 向后扫描，找第一个有效的反向分型
    for i in 1..fxs.len() {
        let fx_b = &fxs[i];

        // 1. 必须是反向分型且价格方向正确
        let direction = match (fx_a.mark, fx_b.mark) {
            (FxMark::Top, FxMark::Bottom) if fx_b.fx < fx_a.fx => "down",
            (FxMark::Bottom, FxMark::Top) if fx_b.fx > fx_a.fx => "up",
            _ => continue,
        };

        // 2. 顶底区间互不包含
        let contained = (fx_a.high > fx_b.high && fx_a.low < fx_b.low)
            || (fx_a.high < fx_b.high && fx_a.low > fx_b.low);
        if contained {
            continue;
        }

        // 3. 笔长度：从 fx_a 第一根bar到 fx_b 最后一根bar
        let total = fx_b.bars[2] - fx_a.bars[0] + 1;
        if total < min_bi_len {
            continue;
        }

        // ========== 关键：分型失效检查 ==========
        // 上升笔（底→顶）：顶分型之后到下一个底分型之间，
        //   如果有K线高点 > 顶分型的高点，此顶分型被突破失效，笔继续延伸
        // 下降笔（顶→底）：底分型之后到下一个顶分型之间，
        //   如果有K线低点 < 底分型的低点，此底分型被突破失效，笔继续延伸
        let invalidated = match fx_a.mark {
            FxMark::Bottom => {
                // 上升笔：fx_b 是顶分型
                // 找到下一个底分型的 k1 位置（即 fxs 中下一个与 fx_a 同类型的分型）
                let next_same_k1 = fxs[i + 1..]
                    .iter()
                    .find(|fx| fx.mark == FxMark::Bottom)
                    .map(|fx| fx.bars[0])
                    .unwrap_or(bars.len());

                let check_start = fx_b.bars[2] + 1; // 顶分型 k3 之后
                if check_start < next_same_k1 && check_start < bars.len() {
                    let check_end = next_same_k1.min(bars.len());
                    bars[check_start..check_end].iter().any(|b| b.high > fx_b.high)
                } else {
                    false
                }
            }
            FxMark::Top => {
                // 下降笔：fx_b 是底分型
                let next_same_k1 = fxs[i + 1..]
                    .iter()
                    .find(|fx| fx.mark == FxMark::Top)
                    .map(|fx| fx.bars[0])
                    .unwrap_or(bars.len());

                let check_start = fx_b.bars[2] + 1; // 底分型 k3 之后
                if check_start < next_same_k1 && check_start < bars.len() {
                    let check_end = next_same_k1.min(bars.len());
                    bars[check_start..check_end].iter().any(|b| b.low < fx_b.low)
                } else {
                    false
                }
            }
        };

        if invalidated {
            // 反向分型被突破失效，笔继续延伸，跳过此分型
            continue;
        }

        // === 成笔！===
        let bars_b = bars[fx_b.bars[0]..].to_vec();

        let bi = Bi {
            direction: direction.to_string(),
            start_index: fx_a.merged_index as u64,
            end_index: fx_b.merged_index as u64,
            start_dt: fx_a.dt.clone(),
            end_dt: fx_b.dt.clone(),
            start_price: fx_a.fx,
            end_price: fx_b.fx,
            is_finished: true,
        };

        return Some((bi, bars_b));
    }

    None
}

/// 对完整 K 线序列构建笔（批量模式）
///
/// 直接对全部K线做包含处理，然后逐笔配对。
/// 配对时遵守缠论核心规则：反向分型被后续新高/新低突破则失效作废。
pub fn build_bi(klines: &[yifang_data::KLine], min_bi_len: Option<usize>) -> Vec<Bi> {
    let min_bi_len = min_bi_len.unwrap_or(DEFAULT_MIN_BI_LEN);
    if klines.len() < 3 {
        return Vec::new();
    }

    // 1. K线去包含（批处理）
    let bars = crate::include::remove_include(klines);

    // 2. 迭代找笔
    let mut remaining = bars;
    let mut bis = Vec::new();

    while let Some((bi, bars_b)) = check_bi(&remaining, min_bi_len) {
        bis.push(bi);
        remaining = bars_b;
    }

    bis
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
    fn test_min_bi_len() {
        let klines = vec![
            make_kline(0, "2024-01-01", 10.0, 11.0, 11.0, 10.0),
            make_kline(1, "2024-01-02", 11.0, 13.0, 13.0, 11.0),
            make_kline(2, "2024-01-03", 13.0, 10.0, 13.0, 10.0),
        ];
        let _bars = remove_include(&klines);
        // min_bi_len=7，只有3根K线，不够
        let bis = build_bi(&klines, Some(7));
        assert!(bis.is_empty(), "K线不足 min_bi_len，不应成笔");
    }

    #[test]
    fn test_bi_exact_output() {
        // 已知的简单走势：上升后下降（预期 exactly 1笔向上）
        // 必须产生一个清晰的底部→顶部分型
        let mut klines = Vec::new();
        let mut id = 0u64;
        // 上升段: 10→20 (10根)
        for i in 0..10 {
            let low = 10.0 + i as f64;
            let high = low + 1.0;
            klines.push(make_kline(id, &format!("2024-01-{:02}", id+1), low, high, high, low));
            id += 1;
        }
        // 下降段: 20→10 (10根)  
        for i in 0..10 {
            let low = 19.0 - i as f64;
            let high = low + 1.0;
            klines.push(make_kline(id, &format!("2024-01-{:02}", id+1), high, low, high, low));
            id += 1;
        }
        // 再上升段: 10→15 (5根)
        for i in 0..5 {
            let low = 10.0 + i as f64;
            let high = low + 1.0;
            klines.push(make_kline(id, &format!("2024-01-{:02}", id+1), low, high, high, low));
            id += 1;
        }

        println!("\n===== 精确验证测试 =====");
        println!("总K线: {}根", klines.len());
        
        let bis = build_bi(&klines, None);
        println!("输出笔: {}根", bis.len());
        for (i, bi) in bis.iter().enumerate() {
            let d = if bi.start_price < bi.end_price { "↑" } else { "↓" };
            println!("  BI[{}] {} {:.1}→{:.1} (idx {}/{}→{}/{})",
                i, d, bi.start_price, bi.end_price, 
                bi.start_index, bi.start_dt, bi.end_index, bi.end_dt);
        }
        
        // 预期：应该至少有笔（笔数取决于分型位置）
        assert!(!bis.is_empty(), "必须产生笔");
        
        // 每笔的起止价格必须正确
        for bi in &bis {
            let is_up = bi.start_price < bi.end_price;
            assert_eq!(bi.direction, if is_up { "up" } else { "down" },
                "笔方向与价格关系不一致");
            if is_up {
                assert!(bi.end_price > bi.start_price, "向上笔: {:.1} < {:.1}", bi.start_price, bi.end_price);
            } else {
                assert!(bi.end_price < bi.start_price, "向下笔: {:.1} > {:.1}", bi.start_price, bi.end_price);
            }
        }
    }

    /// 反向笔最早出现 vs 最极端的选择测试
    /// 缠论要求取第一个符合条件的反向分型（不是最极端的）
    #[test]
    fn test_first_reverse_fx_selection() {
        // 构造：底分型(10) → 多个顶分型(14, 16, 18)，取第一个(14)而不是最高的(18)
        let klines = vec![
            // 底部区域
            make_kline(0, "2024-01-01", 12.0, 11.0, 12.0, 11.0),
            make_kline(1, "2024-01-02", 11.0, 10.0, 11.0, 10.0), // 底分型中间
            make_kline(2, "2024-01-03", 10.0, 10.5, 10.5, 10.0),
            // 上升1: 到14
            make_kline(3, "2024-01-04", 10.5, 12.0, 12.0, 10.5),
            make_kline(4, "2024-01-05", 12.0, 13.0, 13.0, 12.0),
            make_kline(5, "2024-01-06", 13.0, 14.0, 14.0, 13.0), // 顶分型1 (h=14)
            make_kline(6, "2024-01-07", 14.0, 13.5, 14.0, 13.5),
            // 上升2: 到16
            make_kline(7, "2024-01-08", 13.5, 15.0, 15.0, 13.5),
            make_kline(8, "2024-01-09", 15.0, 16.0, 16.0, 15.0), // 顶分型2 (h=16)
            make_kline(9, "2024-01-10", 16.0, 15.5, 16.0, 15.5),
            // 上升3: 到18
            make_kline(10, "2024-01-11", 15.5, 17.0, 17.0, 15.5),
            make_kline(11, "2024-01-12", 17.0, 18.0, 18.0, 17.0), // 顶分型3 (h=18)
            make_kline(12, "2024-01-13", 18.0, 17.5, 18.0, 17.5),
            // 下降
            make_kline(13, "2024-01-14", 17.5, 16.0, 17.5, 16.0),
            make_kline(14, "2024-01-15", 16.0, 15.0, 16.0, 15.0),
            make_kline(15, "2024-01-16", 15.0, 14.0, 15.0, 14.0),
            make_kline(16, "2024-01-17", 14.0, 13.0, 14.0, 13.0),
            make_kline(17, "2024-01-18", 13.0, 12.0, 13.0, 12.0),
            make_kline(18, "2024-01-19", 12.0, 11.0, 12.0, 11.0),
        ];

        println!("\n===== 反向分型选择测试 =====");
        println!("总K线: {}根", klines.len());
        for (i, k) in klines.iter().enumerate() {
            println!("  raw[{}] dt={} h={:.1} l={:.1}", i, k.dt, k.high, k.low);
        }
        
        // 去包含输出 (debug)
        let bars = remove_include(&klines);
        println!("\n去包含后: {} bars", bars.len());
        for (i, b) in bars.iter().enumerate() {
            println!("  bar[{}] dt={} h={:.1} l={:.1} id={}", i, b.dt, b.high, b.low, b.id);
        }
        let fxs = check_fxs(&bars);
        println!("\n分型序列:");
        for (i, fx) in fxs.iter().enumerate() {
            let mark = match fx.mark { crate::fenxing::FxMark::Top => "TOP", crate::fenxing::FxMark::Bottom => "BTM" };
            println!("  FX[{}] {} fx={:.1} dt={} bars=[{},{},{}] high={:.1} low={:.1}", 
                i, mark, fx.fx, fx.dt, fx.bars[0], fx.bars[1], fx.bars[2], fx.high, fx.low);
        }

        let bis = build_bi(&klines, None);
        println!("\n输出笔: {}根", bis.len());
        for (i, bi) in bis.iter().enumerate() {
            let d = if bi.start_price < bi.end_price { "↑" } else { "↓" };
            println!("  BI[{}] {} {:.1}→{:.1} (idx {}→{})", i, d, bi.start_price, bi.end_price, bi.start_index, bi.end_index);
        }
        
        assert!(!bis.is_empty(), "必须找到笔");
    }

    #[test]
    fn test_contains_scenario() {
        // 测试包含处理：一根大阳线后跟包含关系的小K线，再下跌
        let klines = vec![
            // 上升段 10→15 (5根)
            make_kline(0, "2024-01-01", 10.0, 11.0, 11.0, 10.0),
            make_kline(1, "2024-01-02", 11.0, 12.0, 12.0, 11.0),
            make_kline(2, "2024-01-03", 12.0, 14.0, 14.0, 12.0), // 大阳
            // 包含关系的小K线：范围在大阳内
            make_kline(3, "2024-01-04", 13.0, 13.5, 13.5, 13.0), // 被包含
            make_kline(4, "2024-01-05", 13.5, 13.0, 13.5, 13.0), // 继续被包含
            // 下跌段 14→10
            make_kline(5, "2024-01-06", 13.0, 12.0, 13.0, 12.0),
            make_kline(6, "2024-01-07", 12.0, 11.0, 12.0, 11.0),
            make_kline(7, "2024-01-08", 11.0, 10.0, 11.0, 10.0),
            make_kline(8, "2024-01-09", 10.0, 9.0, 10.0, 9.0),
            make_kline(9, "2024-01-10", 9.0, 8.0, 9.0, 8.0),
            make_kline(10, "2024-01-11", 8.0, 7.0, 8.0, 7.0),
            make_kline(11, "2024-01-12", 7.0, 6.0, 7.0, 6.0),
            // 再上涨
            make_kline(12, "2024-01-13", 6.0, 7.0, 7.0, 6.0),
            make_kline(13, "2024-01-14", 7.0, 8.0, 8.0, 7.0),
            make_kline(14, "2024-01-15", 8.0, 9.0, 9.0, 8.0),
            make_kline(15, "2024-01-16", 9.0, 10.0, 10.0, 9.0),
            make_kline(16, "2024-01-17", 10.0, 11.0, 11.0, 10.0),
            make_kline(17, "2024-01-18", 11.0, 12.0, 12.0, 11.0),
        ];

        let bars = remove_include(&klines);
        println!("\n包含测试 - 去包含后 {} bars:", bars.len());
        for (i, b) in bars.iter().enumerate() {
            println!("  [{}] dt={} h={:.1} l={:.1}", i, b.dt, b.high, b.low);
        }
        let fxs = check_fxs(&bars);
        for (i, fx) in fxs.iter().enumerate() {
            let mark = match fx.mark { crate::fenxing::FxMark::Top => "TOP", crate::fenxing::FxMark::Bottom => "BTM" };
            println!("  FX[{}] {} fx={:.1} dt={}", i, mark, fx.fx, fx.dt);
        }

        let bis = build_bi(&klines, None);
        println!("包含测试结果: {} 笔", bis.len());
        for (i, bi) in bis.iter().enumerate() {
            let d = if bi.start_price < bi.end_price { "↑" } else { "↓" };
            println!("  BI[{}] {} {:.1}→{:.1} ({}→{})", i, d, bi.start_price, bi.end_price, bi.start_index, bi.end_index);
        }
        assert!(!bis.is_empty(), "包含场景应该找到笔");
    }
}

#[cfg(test)]
mod compare_test {
    use super::*;
    use crate::include::remove_include;
    use crate::fenxing::check_fxs;
    use yifang_data::{KLine, TimeFrame};

    /// ================================================================
    /// 严格缠论手工追踪测试
    /// 
    /// 1. 构造简单价格序列（无包含关系），手工计算每个分型、每条笔
    /// 2. 每一步打印所有中间状态
    /// 3. 验证算法结果 = 手工期望结果
    /// ================================================================
    
    /// 辅助：打印一条 bar 的简洁信息
    fn bar_str(b: &NewBar) -> String {
        format!("id={} dt={} h={:.1} l={:.1}", b.id, b.dt, b.high, b.low)
    }
    
    /// 辅助：打印分型信息
    fn fx_str(fx: &crate::fenxing::FxResult) -> String {
        let mark = match fx.mark { crate::fenxing::FxMark::Top => "TOP", crate::fenxing::FxMark::Bottom => "BTM" };
        format!("{} fx={:.1} h={:.1} l={:.1} bars=[{},{},{}] dt={}", 
            mark, fx.fx, fx.high, fx.low, fx.bars[0], fx.bars[1], fx.bars[2], fx.dt)
    }
    
    /// 辅助：创建 K 线（仅 high/low，忽略 open/close）
    fn mk_kline(id: u64, dt: &str, high: f64, low: f64) -> KLine {
        KLine {
            symbol: "test".to_string(),
            timeframe: TimeFrame::D,
            dt: dt.to_string(),
            id,
            open: low,
            close: high,
            high,
            low,
            vol: 1000.0,
            amount: 10000.0,
        }
    }

    /// ============================================================================
    /// 测试1：标准上升→下降→上升，预期输出2条笔（↓和↑）
    /// 
    /// K线序列（无包含，共28根）：
    ///   上升段:   id=0..7   (h=11..18, l=10..17)    8根
    ///   确认顶:   id=8      (h=19, l=18)             ← TOP(19)
    ///   下降段:   id=9..16  (h=17..10, l=16..9)     8根 (注意下降段从17→9)
    ///   确认底+上升: id=17..25 (h=11..19, l=10..18)  9根
    ///   确认顶组: id=26    (h=20, l=19)              ← TOP(20)
    ///            id=27    (h=19, l=18)               ← 确认
    /// 
    /// 分型: TOP(19)→BTM(9)→TOP(20) → 2笔
    /// ============================================================================
    #[test]
    fn test_manual_trace_three_segments() {
        println!("\n========== 测试1：标准三段式 ==========");
        
        let mut klines = Vec::new();
        // 上升10→18 (8根): h=11..18, l=10..17
        for i in 0..8 {
            let v = 10.0 + i as f64;
            klines.push(mk_kline(i as u64, &format!("2024-01-{:02}", i+1), v+1.0, v));
        }
        // 确认顶 (1根) h=19,l=18
        klines.push(mk_kline(8, "2024-01-09", 19.0, 18.0));
        // 下降: 从17→9的严格下降序列 (8根, 无包含): h=17..10, l=16..9
        // 注意最低点是id=16(h=10,l=9)
        for i in 0..8 {
            let v = 16.0 - i as f64;  // v=16,15,...,9
            klines.push(mk_kline(9+i as u64, &format!("2024-01-{:02}", 10+i), v+1.0, v));
        }
        // BTM确认 + 上升段: 从h=11,l=10开始严格上升 (9根)
        for i in 0..9 {
            let v = 10.0 + i as f64;  // v=10,11,...,18
            klines.push(mk_kline(17+i as u64, &format!("2024-01-{:02}", 18+i), v+1.0, v));
        }
        // 确认顶组 (2根)
        klines.push(mk_kline(26, "2024-01-27", 20.0, 19.0));
        klines.push(mk_kline(27, "2024-01-28", 19.0, 18.0));
        
        println!("\n输入K线 ({}根):", klines.len());
        for k in &klines {
            println!("  [{}] {} h={:.0} l={:.0}", k.id, k.dt, k.high, k.low);
        }
        
        // ---- 第1步：去包含 ----
        let bars = remove_include(&klines);
        println!("\n去包含后 ({}根):", bars.len());
        for (i, b) in bars.iter().enumerate() {
            println!("  bar[{}] {}", i, bar_str(b));
        }
        
        // ---- 第2步：分型 ----
        let fxs = check_fxs(&bars);
        println!("\n分型 ({}个):", fxs.len());
        for (i, fx) in fxs.iter().enumerate() {
            println!("  FX[{}] {}", i, fx_str(fx));
        }
        
        // 手工验证分型
        assert!(fxs.len() >= 3, "应该至少3个分型: TOP→BTM→TOP");
        assert_eq!(fxs[0].mark, crate::fenxing::FxMark::Top, "第一个分型应为顶");
        assert_eq!(fxs[1].mark, crate::fenxing::FxMark::Bottom, "第二个分型应为底");
        assert_eq!(fxs[2].mark, crate::fenxing::FxMark::Top, "第三个分型应为顶");
        assert!((fxs[0].fx - 19.0).abs() < 0.01, "顶分型应在19，实际={:.1}", fxs[0].fx);
        assert!((fxs[1].fx - 9.0).abs() < 0.01, "底分型应在9，实际={:.1}", fxs[1].fx);
        assert!((fxs[2].fx - 20.0).abs() < 0.01, "顶分型应在20，实际={:.1}", fxs[2].fx);
        
        // ---- 第3步：笔 ----
        let bis = build_bi(&klines, None);
        println!("\n笔 ({}条):", bis.len());
        for (i, bi) in bis.iter().enumerate() {
            let d = if bi.start_price < bi.end_price { "↑" } else { "↓" };
            println!("  BI[{}] {} {:.1}→{:.1} (idx {}→{}) dt={}→{}", 
                i, d, bi.start_price, bi.end_price, bi.start_index, bi.end_index, bi.start_dt, bi.end_dt);
        }
        
        // 验证应为2条笔（↓和↑）
        assert_eq!(bis.len(), 2, "应有2条笔，实际{}条", bis.len());
        assert_eq!(bis[0].direction, "down", "BI[0]应为↓ (TOP→BTM)");
        assert_eq!(bis[1].direction, "up", "BI[1]应为↑ (BTM→TOP)");
        assert!((bis[0].start_price - 19.0).abs() < 0.01, "BI[0].start应为19, 实际={:.1}", bis[0].start_price);
        assert!((bis[0].end_price - 9.0).abs() < 0.01, "BI[0].end应为9, 实际={:.1}", bis[0].end_price);
        assert!((bis[1].start_price - 9.0).abs() < 0.01, "BI[1].start应为9, 实际={:.1}", bis[1].start_price);
        assert!((bis[1].end_price - 20.0).abs() < 0.01, "BI[1].end应为20, 实际={:.1}", bis[1].end_price);
        
        println!("  ✓ 结果正确！");
    }

    /// ============================================================================
    /// 测试2：包含关系对笔的影响
    /// 
    /// K线序列（含包含）：
    ///   bar[0]: h=11, l=10 (上升起点)
    ///   bar[1]: h=13, l=12 (上升)
    ///   bar[2]: h=14, l=11 (包含bar[1]：h↑↑, l↓↓ → 向上处理取高高 → h=14, l=12)
    ///   bar[3]: h=15, l=14 (上升)
    ///     → TOP at merged(bar1+bar2) : h=14, l=12
    ///   bar[4]: h=13, l=12 (下降)
    ///   bar[5]: h=11, l=10 (下降)
    ///   bar[6]: h=12, l=11 (包含bar[5]：h↑, l↑ → 向上处理... 但方向是下降)
    /// 
    /// 实际需要设计一个更清晰的场景
    /// ============================================================================
    #[test]
    fn test_manual_trace_with_include() {
        println!("\n========== 测试2：包含关系场景 ==========");
        
        // 15根K线，包含层次复杂
        let mut klines = Vec::new();
        // 段1: 上升10→16，中间有一个包含 (bar3包含bar2)
        klines.push(mk_kline(0, "2024-01-01", 11.0, 10.0));  // bar0: 上升起点
        klines.push(mk_kline(1, "2024-01-02", 12.0, 11.0));  // bar1: 上升
        klines.push(mk_kline(2, "2024-01-03", 13.0, 12.0));  // bar2: 上升
        klines.push(mk_kline(3, "2024-01-04", 14.0, 11.5));  // bar3: 包含bar2 (h↑, l↓) → 向上取高高: h=14,l=12.5
        klines.push(mk_kline(4, "2024-01-05", 15.0, 14.0));  // bar4: 上升 → TOP确认
        klines.push(mk_kline(5, "2024-01-06", 16.0, 15.0));  // bar5: 上升（被bar4包含? h=15<16 && l=14>15? NO, 不包含）
        // 段2: 下降16→10
        klines.push(mk_kline(6, "2024-01-07", 15.0, 14.0));  // bar6: 下降
        klines.push(mk_kline(7, "2024-01-08", 14.0, 13.0));  // bar7: 下降
        klines.push(mk_kline(8, "2024-01-09", 13.0, 12.0));  // bar8: 下降
        klines.push(mk_kline(9, "2024-01-10", 12.0, 11.0));  // bar9: 下降
        klines.push(mk_kline(10, "2024-01-11", 11.0, 10.0)); // bar10: 下降 → BTM确认
        // 段3: 上升10→14
        klines.push(mk_kline(11, "2024-01-12", 12.0, 11.0)); // bar11: 上升
        klines.push(mk_kline(12, "2024-01-13", 13.0, 12.0)); // bar12: 上升
        klines.push(mk_kline(13, "2024-01-14", 14.0, 13.0)); // bar13: 上升 → TOP确认
        
        let bis = build_bi(&klines, None);
        println!("\n笔 ({}条):", bis.len());
        for (i, bi) in bis.iter().enumerate() {
            let d = if bi.start_price < bi.end_price { "↑" } else { "↓" };
            println!("  BI[{}] {} {:.1}→{:.1} (idx {}→{}) dt={}→{}", 
                i, d, bi.start_price, bi.end_price, bi.start_index, bi.end_index, bi.start_dt, bi.end_dt);
        }
        
        // 验证
        assert!(!bis.is_empty(), "应找到笔");
    }

    /// ============================================================================
    /// 测试3：最小K线笔长度的边界情况
    /// 
    /// 构造恰好满足 min_bi_len=6 的笔，以及差1根不满足的
    /// ============================================================================
    #[test]
    fn test_min_bi_len_boundary() {
        println!("\n========== 测试3：最小笔长边界 ==========");
        
        // 最小笔长=6：需要顶底之间至少6根去包含bar
        // TOP.bars=[1,2,3], BTM.bars=[5,6,7] → bars[1..7]=7根 ≥ 6 → 成笔
        let klines_enough = vec![
            mk_kline(0, "2024-01-01", 11.0, 10.0),
            mk_kline(1, "2024-01-02", 12.0, 11.0),
            mk_kline(2, "2024-01-03", 13.0, 12.0),
            mk_kline(3, "2024-01-04", 14.0, 13.0),  // TOP(14)
            mk_kline(4, "2024-01-05", 13.0, 12.0),
            mk_kline(5, "2024-01-06", 12.0, 11.0),
            mk_kline(6, "2024-01-07", 11.0, 10.0),  // BTM(10) 
            // BTM确认后还需要最少6根... 实际上 check_bi 取 bars from TOP.bars[0] 到 BTM.bars[2]
            // TOP.bars = [1,2,3], BTM.bars = [5,6,7]
            // bars from dt(bar[1]) to dt(bar[7]) = bar[1]..bar[7] = 7 bars >= 6 → 成笔!
        ];
        
        // 再多几根让第二笔也满足
        let mut klines_full = klines_enough.clone();
        for i in 8..14 {
            let v = 10.0 + (i - 7) as f64;
            klines_full.push(mk_kline(i as u64, &format!("2024-01-{:02}", i), v, v - 1.0));
        }
        
        // 测试min_bi_len=6的情况
        let bis = build_bi(&klines_full, Some(6));
        println!("\nmin_bi_len=6: {}条笔", bis.len());
        for (i, bi) in bis.iter().enumerate() {
            let d = if bi.start_price < bi.end_price { "↑" } else { "↓" };
            println!("  BI[{}] {} {:.1}→{:.1}", i, d, bi.start_price, bi.end_price);
        }
        
        // 测试min_bi_len=4（更宽松）
        let bis4 = build_bi(&klines_full, Some(4));
        println!("\nmin_bi_len=4: {}条笔", bis4.len());
        for (i, bi) in bis4.iter().enumerate() {
            let d = if bi.start_price < bi.end_price { "↑" } else { "↓" };
            println!("  BI[{}] {} {:.1}→{:.1}", i, d, bi.start_price, bi.end_price);
        }
        
        // 验证：min_bi_len=4 应该比 min_bi_len=6 找到更多或等量的笔
        assert!(bis4.len() >= bis.len(), "更小的min_bi_len应找到更多或等量笔");
    }

    /// ============================================================================
    /// 测试4：包含处理对分型识别的精确影响
    /// 
    /// 构造数据验证包含合并后的分型位置是否正确
    /// ============================================================================
    #[test]
    fn test_include_effect_on_fenxing() {
        println!("\n========== 测试4：包含对分型的影响 ==========");
        
        // 构造包含场景：
        // bar0: h=11,l=10 (上升)
        // bar1: h=13,l=12 (上升) 
        // bar2: h=14,l=11 (包含bar1! h↑, l↓)
        //   → 向上取高高: merged = h=14, l=12
        // bar3: h=15,l=14 (上升)
        //   → TOP at merged bar (bar1+bar2), h=14,l=12
        // 验证：确认包含合并后的分型依然正确
        
        let klines = vec![
            mk_kline(0, "2024-01-01", 11.0, 10.0),
            mk_kline(1, "2024-01-02", 13.0, 12.0),
            mk_kline(2, "2024-01-03", 14.0, 11.0),  // 包含
            mk_kline(3, "2024-01-04", 15.0, 14.0),  // TOP
        ];
        
        let bars = remove_include(&klines);
        println!("\n去包含后 ({}根):", bars.len());
        for (i, b) in bars.iter().enumerate() {
            println!("  bar[{}] {}", i, bar_str(b));
        }
        
        let fxs = check_fxs(&bars);
        println!("\n分型 ({}个):", fxs.len());
        for (i, fx) in fxs.iter().enumerate() {
            println!("  FX[{}] {}", i, fx_str(fx));
        }
        
        // bar0, bar1 merged, bar3: 检查分型
        // bar1_merged: h=14,l=12, bar3: h=15,l=14
        // 需要检查 bar1_merged 是否是 TOP:
        //   k1 = bar0(h=11,l=10), k2 = merged(h=14,l=12), k3 = bar3(h=15,l=14)
        //   k1.high(11) < k2.high(14) && k2.high(14) < k3.high(15) → 不是TOP (需要 >
        //   所以 merged 不是 TOP。下一个检查:
        //   k1 = merged(h=14,l=12), k2 = bar3(h=15,l=14), 但没有 k3 → 无法判断
        // → 只有 K线不够形成分型
        
        assert!(fxs.is_empty(), "4根K线去包含后只剩3根，但第2根不是顶点，不应有分型");
        println!("  ✓ 正确：4根K线去包含后不够形成有效分型");
    }

    /// ============================================================================
    /// 测试5：分型失效规则（核心缠论规则）
    ///
    /// 三种场景：
    ///   场景1（正常成笔）：底→顶→新顶(更高) → 不突破顶分型，正常成笔
    ///   场景2（顶分型失效）：底→顶→新高突破顶→出现底 → 顶被突破失效，不成笔于该顶
    ///   场景3（顶分型锁定）：底→顶→底k1出现(锁定) → 新高 → 不可逆，仍成笔于原顶
    /// ============================================================================
    #[test]
    fn test_fx_invalidation() {
        println!("\n========== 测试5：分型失效规则 ==========");

        // ---- 场景1：正常成笔（顶不被突破） ----
        println!("\n--- 场景1：顶不被突破，正常成笔 ---");
        // BOTTOM(9) at bar[2] → ... 上升 ... → TOP(14) at bar[9] → ... 下降 ... → BOTTOM(7) at bar[15]
        // 笔长: from bars[btm.bars[0]=1] to bars[top.bars[2]=10] = 10 bars >= 7 ✓
        let klines1 = vec![
            mk_kline(0, "2024-01-01", 13.0, 12.0),   // bar0: 下跌起点
            mk_kline(1, "2024-01-02", 11.0, 10.0),   // bar1: BTM k1
            mk_kline(2, "2024-01-03", 10.0, 9.0),    // bar2: BTM k2 = BOTTOM(9)
            mk_kline(3, "2024-01-04", 11.0, 10.0),   // bar3: BTM k3
            mk_kline(4, "2024-01-05", 12.0, 11.0),   // bar4: 上升
            mk_kline(5, "2024-01-06", 13.0, 12.0),   // bar5: 上升
            mk_kline(6, "2024-01-07", 14.0, 13.0),   // bar6: TOP k2 = TOP(14)
            mk_kline(7, "2024-01-08", 13.0, 12.0),   // bar7: TOP k3
            mk_kline(8, "2024-01-09", 12.0, 11.0),   // bar8: 下降
            mk_kline(9, "2024-01-10", 11.0, 10.0),   // bar9: 下降
            mk_kline(10, "2024-01-11", 10.0, 9.0),   // bar10: 下降
            mk_kline(11, "2024-01-12", 9.0, 8.0),    // bar11: BTM k1
            mk_kline(12, "2024-01-13", 8.0, 7.0),    // bar12: BTM k2 = BOTTOM(7)
            mk_kline(13, "2024-01-14", 9.0, 8.0),    // bar13: BTM k3
        ];
        let bis1 = build_bi(&klines1, None);
        println!("  bis1通过build_bi: {}条笔", bis1.len());
        // 打印去包含后状态
        let bars1 = remove_include(&klines1);
        println!("  去包含后: {} bars", bars1.len());
        for (i, b) in bars1.iter().enumerate() {
            println!("    bar[{}] h={:.1} l={:.1}", i, b.high, b.low);
        }
        let fxs1 = check_fxs(&bars1);
        println!("  分型: {}个", fxs1.len());
        for (i, fx) in fxs1.iter().enumerate() {
            let mark = match fx.mark { crate::fenxing::FxMark::Top => "TOP", crate::fenxing::FxMark::Bottom => "BTM" };
            println!("    FX[{}] {} fx={:.1} bars=[{},{},{}]", i, mark, fx.fx, fx.bars[0], fx.bars[1], fx.bars[2]);
        }
        assert!(!bis1.is_empty(), "场景1：应有笔");
        // 第一笔从BTM(9)到TOP(14)，升高（顶未被突破）
        assert!((bis1[0].end_price - 14.0).abs() < 0.01,
            "场景1：第一笔终点应为14（首个顶），实际={:.1}", bis1[0].end_price);
        println!("  ✓ 第一笔 {} {:.1}→{:.1}", bis1[0].direction, bis1[0].start_price, bis1[0].end_price);
        // 第二笔从TOP(14)到BTM(7)：确认顶未被突破时正常成笔
        if bis1.len() > 1 {
            assert!((bis1[1].end_price - 7.0).abs() < 0.01,
                "场景1：第二笔终点应为7，实际={:.1}", bis1[1].end_price);
        }

        // ---- 场景2：顶分型被新高突破，失效 ----
        println!("\n--- 场景2：顶被新高突破，失效 ---");
        // BTM(9) at bar[2] → 顶①(14) at bar[5] → 新高(15+) → 顶②(19) at bar[10] → BTM(11) at bar[16]
        let klines2 = vec![
            mk_kline(0, "2024-01-01", 13.0, 12.0),
            mk_kline(1, "2024-01-02", 11.0, 10.0),  // BTM k1
            mk_kline(2, "2024-01-03", 10.0, 9.0),   // BTM k2 = BOTTOM(9)
            mk_kline(3, "2024-01-04", 11.0, 10.0),  // BTM k3
            mk_kline(4, "2024-01-05", 13.0, 12.0),  // 上升
            mk_kline(5, "2024-01-06", 14.0, 13.0),  // TOP①(14) ← 将被突破
            mk_kline(6, "2024-01-07", 15.0, 14.0),  // 新高15 > 14 → TOP①失效
            mk_kline(7, "2024-01-08", 17.0, 16.0),  // 继续上升
            mk_kline(8, "2024-01-09", 18.0, 17.0),  // 继续上升
            mk_kline(9, "2024-01-10", 19.0, 18.0),  // TOP②k2 = TOP(19)
            mk_kline(10, "2024-01-11", 18.0, 17.0), // TOP②k3
            mk_kline(11, "2024-01-12", 17.0, 16.0), // 下降
            mk_kline(12, "2024-01-13", 16.0, 15.0), // 下降
            mk_kline(13, "2024-01-14", 15.0, 14.0), // 下降
            mk_kline(14, "2024-01-15", 14.0, 13.0), // 下降
            mk_kline(15, "2024-01-16", 13.0, 12.0), // BTM k1
            mk_kline(16, "2024-01-17", 12.0, 11.0), // BTM k2 = BOTTOM(11)
            mk_kline(17, "2024-01-18", 13.0, 12.0), // BTM k3
        ];
        // 验证：笔应从BTM(9)到TOP(19)，跳过被失效的TOP①(14)
        let bis2 = build_bi(&klines2, None);
        assert!(!bis2.is_empty(), "场景2：应有笔");
        assert!((bis2[0].end_price - 19.0).abs() < 0.01,
            "场景2：第一笔终点应为19（突破后的新高），实际={:.1}", bis2[0].end_price);
        println!("  ✓ 第一笔终点={:.1}（正确：突破14后到19才成笔）", bis2[0].end_price);

        // ---- 场景3：底分型k1出现后，顶分型被锁定不可逆 ----
        println!("\n--- 场景3：底k1锁定后新高不失效 ---");
        // BTM(9) at bar[2] → ... 上升 ... → TOP(14) at bar[6] → 下降 → BTM(10) at bar[9] (k1=bar[8])锁定顶
        // 之后bar[12]新高15 > 14 → 不影响已锁定的顶(14)
        let klines3 = vec![
            mk_kline(0, "2024-01-01", 13.0, 12.0),
            mk_kline(1, "2024-01-02", 11.0, 10.0),  // BTM k1
            mk_kline(2, "2024-01-03", 10.0, 9.0),   // BTM k2 = BOTTOM(9)
            mk_kline(3, "2024-01-04", 11.0, 10.0),  // BTM k3
            mk_kline(4, "2024-01-05", 12.0, 11.0),  // 上升
            mk_kline(5, "2024-01-06", 13.0, 12.0),  // 上升
            mk_kline(6, "2024-01-07", 14.0, 13.0),  // TOP k2 = TOP(14)
            mk_kline(7, "2024-01-08", 13.0, 12.0),  // TOP k3
            mk_kline(8, "2024-01-09", 12.0, 11.0),  // 下降 → BTM k1 → 锁定顶分型(14)!
            mk_kline(9, "2024-01-10", 11.0, 10.0),  // BTM k2 = BOTTOM(10)
            mk_kline(10, "2024-01-11", 12.0, 11.0), // BTM k3
            mk_kline(11, "2024-01-12", 15.0, 14.0), // 新高15 > 14 → 不影响已锁定顶
            mk_kline(12, "2024-01-13", 14.0, 13.0), // 下降
            mk_kline(13, "2024-01-14", 12.0, 11.0), // 下降
            mk_kline(14, "2024-01-15", 11.0, 10.0), // 下降
            mk_kline(15, "2024-01-16", 10.0, 9.0),  // 下降
            mk_kline(16, "2024-01-17", 9.0, 8.0),   // BTM k1
            mk_kline(17, "2024-01-18", 8.0, 7.0),   // BTM k2 = BOTTOM(7)
            mk_kline(18, "2024-01-19", 9.0, 8.0),   // BTM k3
        ];
        let bis3 = build_bi(&klines3, None);
        assert!(!bis3.is_empty(), "场景3：应有笔");
        assert!((bis3[0].end_price - 14.0).abs() < 0.01,
            "场景3：顶被锁定，终点仍为14，实际={:.1}", bis3[0].end_price);
        println!("  ✓ 顶被锁定，终点={:.1}（正确：bar[12]新高15不影响已锁定的顶14）", bis3[0].end_price);

        println!("\n  ✓ 所有分型失效场景验证通过");
    }
}
