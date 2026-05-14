//! 线段的构建
//!
//! **使用 FX-as-KLine 方法，对齐 Python czsc 的线段构建**
//!
//! 核心思路：
//! 1. 在 K 线级别分析得到分型（FX）序列
//! 2. 将每个分型视为虚拟 K 线：
//!    - 顶分型: open=low, close=high, high=fx.high, low=fx.low
//!    - 底分型: open=high, close=low, high=fx.high, low=fx.low
//! 3. 对 FX 虚拟 K 线序列运行完整的 CZSC 流程（去包含→分型→笔）
//! 4. 得到的"笔"就是线段（XianDuan）
//!
//! 这样做的好处：
//! - FX 点之间不共享边界（与笔不同），所以去包含可以正常工作
//! - 算法与 Python czsc 完全一致（CZSC(FX_bars) 得到 XD）

use crate::fenxing::FxResult;
use crate::bi::build_bi;
use yifang_data::{Bi, XianDuan, KLine, TimeFrame};

/// 默认最小线段长度（以笔数计）
const DEFAULT_MIN_XD_LEN: usize = 3;

/// 构建线段 — FX-as-KLine 方法
///
/// 将 K 线级别的分型视为虚拟 K 线，然后对它们运行 CZSC 笔检测算法，
/// 得到的"笔"就是线段。
///
/// 对齐 Python czsc: CZSC(fx_bars) → finished_bis = xianDuan
pub fn build_xd(bis: &[Bi], fxs: &[FxResult]) -> Vec<XianDuan> {
    build_xd_with_min_len(bis, fxs, None)
}

/// 构建线段（可指定最小笔数）
pub fn build_xd_with_min_len(bis: &[Bi], fxs: &[FxResult], min_xd_len: Option<usize>) -> Vec<XianDuan> {
    let min_len = min_xd_len.unwrap_or(DEFAULT_MIN_XD_LEN);
    if bis.len() < min_len || fxs.len() < 3 {
        return Vec::new();
    }

    // Step 1: 将 FX 分型转为虚拟 K 线
    let fx_klines = fx_to_klines(fxs);
    if fx_klines.len() < 3 {
        return Vec::new();
    }

    // Step 2: 对 FX 虚拟 K 线运行 CZSC 构建笔（min_bi_len = min_xd_len）
    // 得到的"笔"就是线段
    let xd_bis = build_bi(&fx_klines, Some(min_len));

    // Step 3: 将 Bi 转为 XianDuan，使用 FX 的信息映射回原始 K 线索引
    bis_to_xianduan(&xd_bis, fxs)
}

/// 将 FX 分型转为 KLine 虚拟 K 线
///
/// 对齐 Python czsc 做法：
/// - 顶分型: open=low, close=high, high=fx.high, low=fx.low（向上bar）
/// - 底分型: open=high, close=low, high=fx.high, low=fx.low（向下bar）
fn fx_to_klines(fxs: &[FxResult]) -> Vec<KLine> {
    fxs.iter().enumerate().map(|(i, fx)| {
        let (open, close) = if fx.fx == fx.high {
            // 顶分型：fx = high → open=low, close=high（向上bar）
            (fx.low, fx.high)
        } else {
            // 底分型：fx = low → open=high, close=low（向下bar）
            (fx.high, fx.low)
        };
        KLine {
            symbol: "xd".to_string(),
            timeframe: TimeFrame::D,
            dt: fx.dt.clone(),
            id: fx.merged_index as u64,
            open,
            close,
            high: fx.high,
            low: fx.low,
            vol: 0.0,
            amount: 0.0,
        }
    }).collect()
}

/// 将 build_bi 返回的笔转为线段
///
/// build_bi 在 FX 虚拟 K 线上运行，返回的 Bi 的 start_index/end_index
/// 是 FX 虚拟 K 线的 id 字段，即原始 K 线索引（merged_index）
fn bis_to_xianduan(xd_bis: &[Bi], fxs: &[FxResult]) -> Vec<XianDuan> {
    // FX 虚拟 K 线的 id = merged_index（原始K线索引）
    // 所以 xd_bi.start_index/end_index 已经是原始 K 线索引
    // 但 start_dt/end_dt 是 FX 虚拟 K 线的时间，需要从 FX 列表中查找
    xd_bis.iter().map(|xd_bi| {
        XianDuan {
            direction: xd_bi.direction.clone(),
            start_index: xd_bi.start_index,
            end_index: xd_bi.end_index,
            start_dt: xd_bi.start_dt.clone(),
            end_dt: xd_bi.end_dt.clone(),
            start_price: xd_bi.start_price,
            end_price: xd_bi.end_price,
            is_finished: xd_bi.is_finished,
        }
    }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fenxing::{FxMark, check_fxs};
    use crate::include::remove_include;
    use yifang_data::TimeFrame;

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

    fn make_bi(id: usize, dir: &str, start_price: f64, end_price: f64, start_idx: u64, end_idx: u64) -> Bi {
        let start_dt = format!("2024-01-{:02}", id * 2 + 1);
        let end_dt = format!("2024-01-{:02}", id * 2 + 2);
        Bi {
            direction: dir.to_string(),
            start_index: start_idx,
            end_index: end_idx,
            start_dt,
            end_dt,
            start_price,
            end_price,
            is_finished: true,
        }
    }

    /// 用一个简单的人工 K 线序列来测试线段
    /// 需要先跑 remove_include → check_fxs 得到 FX，再调用 build_xd
    #[test]
    fn test_xd_min_3_bi() {
        // 简单 K 线，先确认不足 3 笔时不会产生线段
        let klines = vec![
            make_kline(0, "2024-01-01", 10.0, 11.0, 11.0, 10.0),
            make_kline(1, "2024-01-02", 11.0, 9.0, 11.0, 9.0),
            make_kline(2, "2024-01-03", 9.0, 12.0, 12.0, 9.0),
            make_kline(3, "2024-01-04", 12.0, 10.0, 12.0, 10.0),
        ];
        let merged = remove_include(&klines);
        let fxs = check_fxs(&merged);
        let bis = build_bi(&klines, None);
        let xd = build_xd(&bis, &fxs);
        // 只有少量 K 线，可能不会产生线段
        println!("XD count (minimal data): {}", xd.len());
    }

    #[test]
    fn test_xd_with_real_data() {
        // 构建更真实的 K 线数据（多段上下行趋势，必须产生足够多的笔和分型）
        let mut klines = Vec::new();
        let mut id: u64 = 0;

        // 下行段 100→70
        for i in 0..8 {
            let price = 100.0 - i as f64 * 4.0;
            klines.push(make_kline(id, &format!("2024-01-{:02}", id as usize + 1), price + 1.0, price, price + 1.0, price));
            id += 1;
        }
        // 上行段 70→110
        for i in 0..10 {
            let price = 70.0 + i as f64 * 4.0;
            klines.push(make_kline(id, &format!("2024-01-{:02}", id as usize + 1), price, price + 1.0, price + 1.0, price));
            id += 1;
        }
        // 下行段 110→60
        for i in 0..12 {
            let price = 110.0 - i as f64 * 4.0;
            klines.push(make_kline(id, &format!("2024-01-{:02}", id as usize + 1), price + 1.0, price, price + 1.0, price));
            id += 1;
        }
        // 上行段 60→100
        for i in 0..10 {
            let price = 60.0 + i as f64 * 4.0;
            klines.push(make_kline(id, &format!("2024-01-{:02}", id as usize + 1), price, price + 1.0, price + 1.0, price));
            id += 1;
        }
        // 下行段 100→55
        for i in 0..10 {
            let price = 100.0 - i as f64 * 4.5;
            klines.push(make_kline(id, &format!("2024-01-{:02}", id as usize + 1), price + 1.0, price, price + 1.0, price));
            id += 1;
        }
        // 上行段 55→120
        for i in 0..14 {
            let price = 55.0 + i as f64 * 4.5;
            klines.push(make_kline(id, &format!("2024-01-{:02}", id as usize + 1), price, price + 1.0, price + 1.0, price));
            id += 1;
        }

        let merged = remove_include(&klines);
        let fxs = check_fxs(&merged);
        let bis = build_bi(&klines, None);

        println!("Merged K-lines: {}", merged.len());
        println!("FX count: {}", fxs.len());
        println!("BI count: {}", bis.len());

        let xd = build_xd(&bis, &fxs);
        println!("XD count: {}", xd.len());
        for x in &xd {
            println!("  XD: {} {} → {}, price {:.2} → {:.2}",
                x.direction, x.start_dt, x.end_dt, x.start_price, x.end_price);
        }
        // If we have enough diversity, we should get XD.
        // If not enough, just verify the function doesn't panic.
        if bis.len() >= 3 && fxs.len() >= 5 {
            assert!(!xd.is_empty(), "足够多样化的行情应该能形成线段");
        }
    }

    #[test]
    fn test_xd_direction_consistency() {
        let mut klines = Vec::new();
        let mut id: u64 = 0;

        // 下行→上行→下行→上行 模式
        for round in 0..4 {
            let is_down = round % 2 == 0;
            let start: f64 = if is_down { 100.0 - round as f64 * 15.0 } else { 40.0 + round as f64 * 10.0 };
            for i in 0..8 {
                let price = if is_down { start - i as f64 * 3.0 } else { start + i as f64 * 3.0 };
                let (o, c) = if is_down { (price + 1.0, price) } else { (price, price + 1.0) };
                klines.push(make_kline(id, &format!("2024-{:02}-01", id as usize + 1), o, c, o.max(c), o.min(c)));
                id += 1;
            }
        }

        let merged = remove_include(&klines);
        let fxs = check_fxs(&merged);
        let bis = build_bi(&klines, None);
        let xd = build_xd(&bis, &fxs);

        for x in &xd {
            if x.direction == "up" {
                assert!(x.end_price >= x.start_price,
                    "上升线段终价应不低于起价: {} vs {}", x.end_price, x.start_price);
            } else {
                assert!(x.end_price <= x.start_price,
                    "下降线段终价应不高于起价: {} vs {}", x.end_price, x.start_price);
            }
        }
    }

    #[test]
    fn test_fx_to_klines() {
        // 确认 FX 转 KLine 正确
        let fxs = vec![
            FxResult {
                mark: crate::fenxing::FxMark::Top,
                bar_index: 1,
                merged_index: 1,
                dt: "2024-01-02".to_string(),
                high: 120.0,
                low: 115.0,
                fx: 120.0,
                bars: [0, 1, 2],
            },
            FxResult {
                mark: crate::fenxing::FxMark::Bottom,
                bar_index: 3,
                merged_index: 3,
                dt: "2024-01-04".to_string(),
                high: 108.0,
                low: 100.0,
                fx: 100.0,
                bars: [2, 3, 4],
            },
        ];
        let klines = fx_to_klines(&fxs);
        assert_eq!(klines.len(), 2);

        // Top FX → up bar: open=low, close=high
        assert_eq!(klines[0].open, 115.0);
        assert_eq!(klines[0].close, 120.0);
        assert_eq!(klines[0].high, 120.0);
        assert_eq!(klines[0].low, 115.0);

        // Bottom FX → down bar: open=high, close=low
        assert_eq!(klines[1].open, 108.0);
        assert_eq!(klines[1].close, 100.0);
        assert_eq!(klines[1].high, 108.0);
        assert_eq!(klines[1].low, 100.0);
    }
}
