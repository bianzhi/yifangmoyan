//! 诊断：688008 日线和30F缠论分析
//! 运行: cargo test --test diag_688008 test_688008_daily_diag -- --nocapture

use yifang_data::{DataSource, KLineManager, TimeFrame, ZhongShu};
use yifang_czsc::analyzer::CzscAnalyzer;
use yifang_indicator::macd::calc_macd;
use std::collections::HashMap;

#[test]
fn test_688008_daily_diag() {
    let data_dir = "/Users/csdn/Code/moyan/moyan-project/data";
    let manager = KLineManager::new(Some(data_dir));

    // ── 日线 ──
    let daily = manager.get_klines("688008", TimeFrame::D, None, None)
        .expect("加载日线失败");
    println!("\n========== 688008 日线 ==========");
    println!("总数: {} 根", daily.len());
    if let (Some(f), Some(l)) = (daily.first(), daily.last()) {
        println!("范围: {} ~ {}", f.dt, l.dt);
    }

    // 取全部日线，不截断
    let n = daily.len();
    let recent: Vec<_> = daily.iter().rev().take(n).rev().cloned().collect();
    println!("分析最近 {} 根: {} ~ {}",
        recent.len(),
        recent.first().map(|k| k.dt.as_str()).unwrap_or("-"),
        recent.last().map(|k| k.dt.as_str()).unwrap_or("-"));

    let macd = calc_macd(&recent, 12, 26, 9);
    let result = CzscAnalyzer::analyze(&recent, &macd);

    // ── 打印 2026-02-02 附近的原始K线 ──
    println!("\n=== 2026-02-02 附近原始K线（最近80根） ===");
    for (i, k) in recent.iter().rev().take(80).rev().enumerate() {
        let abs_i = recent.len() - 80 + i;
        let marker = if k.dt.contains("2026-02") || k.dt.contains("2026-01") || k.dt.contains("2026-03") {
            " ◀◀"
        } else if k.dt.contains("2025-12") {
            " ◀"
        } else { "" };
        println!("  [{}] {} O:{:.2} H:{:.2} L:{:.2} C:{:.2}{}",
            abs_i, k.dt, k.open, k.high, k.low, k.close, marker);
    }

    // ── 笔 ──
    println!("\n=== 笔 (共{}) ===", result.bi.len());
    for (i, bi) in result.bi.iter().enumerate() {
        let s_dt = recent.get(bi.start_index as usize).map(|k| k.dt.as_str()).unwrap_or("?");
        let e_dt = recent.get(bi.end_index as usize).map(|k| k.dt.as_str()).unwrap_or("?");
        println!("  BI[{}]: {:>4} {:.2}→{:.2}  idx={}→{}  {}→{}",
            i, bi.direction, bi.start_price, bi.end_price,
            bi.start_index, bi.end_index, s_dt, e_dt);
    }

    // ── 中枢 ──
    println!("\n=== 笔中枢 (共{}) ===", result.bi_zs.len());
    for (i, zs) in result.bi_zs.iter().enumerate() {
        let s_dt = recent.get(zs.start_index as usize).map(|k| k.dt.as_str()).unwrap_or("?");
        let e_dt = recent.get(zs.end_index as usize).map(|k| k.dt.as_str()).unwrap_or("?");
        println!("  ZS[{}]: {} idx={}→{} ZG={:.2} ZD={:.2} GG={:.2} DD={:.2}  {}→{}",
            i, zs.zs_type, zs.start_index, zs.end_index,
            zs.zg, zs.zd, zs.gg, zs.dd, s_dt, e_dt);
    }

    // ── 背驰 ──
    println!("\n=== 背驰 (共{}) ===", result.beichi.len());
    for bc in &result.beichi {
        let dt = recent.get(bc.index as usize).map(|k| k.dt.as_str()).unwrap_or("?");
        let price = recent.get(bc.index as usize).map(|k| k.close).unwrap_or(f64::NAN);
        println!("  BC: type={} sub={} dir={} idx={} price={:.2} dt={}",
            bc.bc_type, bc.bc_sub_type, bc.direction, bc.index, price, dt);
        if !bc.reason.is_empty() {
            println!("       reason: {}", bc.reason);
        }
    }

    // ── 线段 ──
    println!("\n=== 线段 (共{}) ===", result.xd.len());
    for (i, xd) in result.xd.iter().enumerate() {
        let s_dt = recent.get(xd.start_index as usize).map(|k| k.dt.as_str()).unwrap_or("?");
        let e_dt = recent.get(xd.end_index as usize).map(|k| k.dt.as_str()).unwrap_or("?");
        println!("  XD[{}]: {:>4} {:.2}→{:.2} idx={}→{} {}→{}",
            i, xd.direction, xd.start_price, xd.end_price,
            xd.start_index, xd.end_index, s_dt, e_dt);
    }
    // ── 线段中枢 ──
    println!("\n=== 线段中枢 (共{}) ===", result.xd_zs.len());
    for (i, zs) in result.xd_zs.iter().enumerate() {
        let s_dt = recent.get(zs.start_index as usize).map(|k| k.dt.as_str()).unwrap_or("?");
        let e_dt = recent.get(zs.end_index as usize).map(|k| k.dt.as_str()).unwrap_or("?");
        println!("  XD_ZS[{}]: idx={}→{} ZG={:.2} ZD={:.2} {}→{}",
            i, zs.start_index, zs.end_index, zs.zg, zs.zd, s_dt, e_dt);
    }

    // ── 买卖点 ──
    println!("\n=== 买卖点 (共{}) ===", result.buy_sell.len());
    for bs in &result.buy_sell {
        let dt = recent.get(bs.index as usize).map(|k| k.dt.as_str()).unwrap_or("OOB");
        let price = recent.get(bs.index as usize).map(|k| k.close).unwrap_or(f64::NAN);
        println!("  {} idx={} dt={} price={:.2} reason={}", bs.bs_type, bs.index, dt, price, bs.reason);
    }

    // ── 重点：2026-02-02 附近信号 ──
    println!("\n=== 2026-02-02 附近信号 ===");
    for bs in &result.buy_sell {
        let dt = recent.get(bs.index as usize).map(|k| k.dt.as_str()).unwrap_or("OOB");
        if dt.contains("2026-02") || dt.contains("2026-01-3") || dt.contains("2026-03-0") {
            let price = recent.get(bs.index as usize).map(|k| k.close).unwrap_or(f64::NAN);
            println!("  {} idx={} dt={} price={:.2}", bs.bs_type, bs.index, dt, price);
        }
    }

    println!("\n========== 688008 30F ==========");
    if let Ok(m30) = manager.get_klines("688008", TimeFrame::F30, None, None) {
        println!("30F总数: {} 根", m30.len());
        let n30 = m30.len().min(800);
        let recent30: Vec<_> = m30.iter().rev().take(n30).rev().cloned().collect();
        let macd30 = calc_macd(&recent30, 12, 26, 9);
        let r30 = CzscAnalyzer::analyze(&recent30, &macd30);

        println!("\n=== 30F 笔中枢 ===");
        for (i, zs) in r30.bi_zs.iter().enumerate() {
            println!("  ZS[{}]: idx={}→{} ZG={:.2} ZD={:.2}", i, zs.start_index, zs.end_index, zs.zg, zs.zd);
        }

        println!("\n=== 30F 买卖点 ===");
        let mut m30_by_type: HashMap<&str, Vec<_>> = HashMap::new();
        for bs in &r30.buy_sell {
            m30_by_type.entry(bs.bs_type.as_str()).or_default().push(bs);
        }
        for t in &["1buy", "2buy", "3buy", "2+3buy", "1sell", "2sell", "3sell"] {
            if let Some(pts) = m30_by_type.get(t) {
                println!("  {}: {} 个", t, pts.len());
                for p in pts {
                    let dt = recent30.get(p.index as usize).map(|k| k.dt.as_str()).unwrap_or("OOB");
                    let price = recent30.get(p.index as usize).map(|k| k.close).unwrap_or(f64::NAN);
                    if dt.contains("2026-02") || dt.contains("2026-01-3") {
                        println!("    ◀ idx={} dt={} price={:.2}", p.index, dt, price);
                    } else {
                        println!("    idx={} dt={} price={:.2}", p.index, dt, price);
                    }
                }
            }
        }
    }
}
