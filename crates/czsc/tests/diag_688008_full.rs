//! 诊断：688008 完整买卖点分析 (2024年起)
//! 运行: cargo test --test diag_688008_full test_688008_full -- --nocapture

use yifang_data::{DataSource, KLineManager, TimeFrame};
use yifang_czsc::analyzer::CzscAnalyzer;
use yifang_indicator::macd::calc_macd;

#[test]
fn test_688008_full() {
    let data_dir = "/Users/csdn/Code/moyan/moyan-project/data";
    let manager = KLineManager::new(Some(data_dir));

    let all = manager.get_klines("688008", TimeFrame::D, None, None).expect("加载日线失败");
    let recent: Vec<_> = all.iter().cloned().collect();
    let macd = calc_macd(&recent, 12, 26, 9);
    let result = CzscAnalyzer::analyze(&recent, &macd);

    let dt_of = |idx: u64| -> String {
        recent.get(idx as usize).map(|k| k.dt.clone()).unwrap_or_else(|| "?".into())
    };

    // ==================== 所有笔 ====================
    println!("\n===== 全部笔 ({}条) =====\n", result.bi.len());
    for (i, bi) in result.bi.iter().enumerate() {
        println!("BI[{:3}] {:>4} {:8.2}→{:8.2} idx={:4}→{:4}  {} → {}", 
            i, bi.direction, bi.start_price, bi.end_price, bi.start_index, bi.end_index,
            dt_of(bi.start_index), dt_of(bi.end_index));
    }

    // ==================== 所有中枢 ====================
    println!("\n===== 笔中枢 ({}个) =====\n", result.bi_zs.len());
    for (i, zs) in result.bi_zs.iter().enumerate() {
        println!("ZS[{:2}] idx={:4}→{:4} ZG={:8.2} ZD={:8.2} GG={:8.2} DD={:8.2}  {} → {}",
            i, zs.start_index, zs.end_index, zs.zg, zs.zd, zs.gg, zs.dd,
            dt_of(zs.start_index), dt_of(zs.end_index));
    }

    // ==================== 所有背驰 ====================
    println!("\n===== 背驰 ({}个) =====\n", result.beichi.len());
    for bc in &result.beichi {
        let price = recent.get(bc.index as usize).map(|k| k.close).unwrap_or(f64::NAN);
        println!("  type={} sub={} dir={} idx={} price={:.2} dt={} reason={}",
            bc.bc_type, bc.bc_sub_type, bc.direction, bc.index, price, dt_of(bc.index), bc.reason);
    }

    // ==================== 线段中枢 ====================
    println!("\n===== 线段中枢 ({}个) =====\n", result.xd_zs.len());
    for (i, zs) in result.xd_zs.iter().enumerate() {
        println!("XDZS[{}] idx={}→{} ZG={:.2} ZD={:.2} {} → {}",
            i, zs.start_index, zs.end_index, zs.zg, zs.zd, dt_of(zs.start_index), dt_of(zs.end_index));
    }

    // ==================== 所有买卖点 + 判定理由 ====================
    println!("\n===== 所有买卖点 ({}个) =====\n", result.buy_sell.len());
    for bs in &result.buy_sell {
        let price = recent.get(bs.index as usize).map(|k| k.close).unwrap_or(f64::NAN);
        println!("  {:>12} idx={:4} dt={} price={:.2} reason={}", 
            bs.bs_type, bs.index, dt_of(bs.index), price, bs.reason);
    }

    // ==================== 2026年信号汇总 ====================
    println!("\n===== 2026年区域分析 =====\n");
    let bs_2026: Vec<_> = result.buy_sell.iter().filter(|bs| {
        recent.get(bs.index as usize).map(|k| k.dt.starts_with("2026")).unwrap_or(false)
    }).collect();
    println!("2026年买卖点: {} 个", bs_2026.len());
    if bs_2026.is_empty() {
        println!("  ⚠️ 2026年无买卖点！分析原因：");
        let trend_bc: Vec<_> = result.beichi.iter()
            .filter(|b| b.bc_sub_type == "trend")
            .collect();
        println!("  趋势背驰总数: {} 个", trend_bc.len());
        for bc in &trend_bc {
            println!("    {} dir={} index={} dt={}", bc.bc_type, bc.direction, bc.index, dt_of(bc.index));
        }
        println!("  笔中枢: {} 个", result.bi_zs.len());
        println!("  线段中枢: {} 个", result.xd_zs.len());
    }

    // 2026年附近的笔
    println!("\n2026年附近笔:");
    for (i, bi) in result.bi.iter().enumerate() {
        let sd = dt_of(bi.start_index);
        let ed = dt_of(bi.end_index);
        if sd.starts_with("2026") || ed.starts_with("2026") {
            println!("  BI[{}] {:>4} {:.2}→{:.2} idx={}→{} {} → {}", 
                i, bi.direction, bi.start_price, bi.end_price, bi.start_index, bi.end_index, sd, ed);
        }
    }
}
