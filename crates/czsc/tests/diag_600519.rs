//! 诊断：贵州茅台 600519 缠论分析 — 详细版
//! 运行: cargo test --test diag_600519 test_600519_30f_analysis -- --nocapture

use yifang_data::{DataSource, KLineManager, TimeFrame, ZhongShu};
use yifang_czsc::analyzer::CzscAnalyzer;
use yifang_indicator::macd::calc_macd;
use std::collections::HashMap;

#[test]
fn test_600519_30f_analysis() {
    let data_dir = "/Users/csdn/Code/moyan/moyan-project/data";
    let manager = KLineManager::new(Some(data_dir));

    let m30 = manager.get_klines("600519", TimeFrame::F30, None, None)
        .expect("加载30F失败");

    println!("\n========== 贵州茅台 600519 ==========");
    println!("30F: {} 根K线", m30.len());
    if let (Some(f), Some(l)) = (m30.first(), m30.last()) {
        println!("30F范围: {} ~ {}", f.dt, l.dt);
    }

    let macd = calc_macd(&m30, 12, 26, 9);
    let result = CzscAnalyzer::analyze(&m30, &macd);

    println!("\n=== 笔中枢详情 ===");
    for (i, zs) in result.bi_zs.iter().enumerate() {
        println!("  笔ZS[{}]: idx {}-{}, ZG={:.2} ZD={:.2} (GG={:.2} DD={:.2})",
            i, zs.start_index, zs.end_index,
            zs.zg, zs.zd, zs.gg, zs.dd);
    }

    println!("\n=== 笔详情 ===");
    for (i, bi) in result.bi.iter().enumerate() {
        println!("  笔[{}]: {:>4} {:.2}→{:.2}  idx={}→{}",
            i, bi.direction, bi.start_price, bi.end_price,
            bi.start_index, bi.end_index);
    }

    println!("\n=== 背驰详情 ===");
    for (i, bc) in result.beichi.iter().enumerate() {
        let price = m30.get(bc.index as usize).map(|k| k.close).unwrap_or(f64::NAN);
        println!("  BC[{}]: type={} sub={} dir={} idx={} price={:.2}",
            i, bc.bc_type, bc.bc_sub_type, bc.direction, bc.index, price);
        if !bc.reason.is_empty() {
            println!("         reason: {}", bc.reason);
        }
    }

    // 手动分组背驰
    let trend_bc: Vec<_> = result.beichi.iter().filter(|b| b.bc_sub_type == "trend").collect();
    println!("\n=== 趋势背驰: {} 个 ===", trend_bc.len());
    for bc in &trend_bc {
        println!("  dir={} idx={} reason={}", bc.direction, bc.index, bc.reason);
    }

    // 买卖点
    println!("\n=== 买卖点 ===");
    let mut by_type: HashMap<&str, Vec<&yifang_data::BuySellPoint>> = HashMap::new();
    for bs in &result.buy_sell {
        by_type.entry(bs.bs_type.as_str()).or_default().push(bs);
    }
    for t in &["1buy", "2buy", "3buy", "1sell", "2sell", "3sell"] {
        if let Some(pts) = by_type.get(t) {
            println!("  {}: {} 个", t, pts.len());
            for p in pts {
                let price = m30.get(p.index as usize).map(|k| k.close).unwrap_or(f64::NAN);
                let dt = m30.get(p.index as usize).map(|k| k.dt.as_str()).unwrap_or("OOB");
                println!("    idx={} dt={} price={:.2}", p.index, dt, price);
            }
        }
    }

    // 重点：检查 check_trend_backdivergence 的核心逻辑
    // 找下跌趋势中是否有2个递进向下中枢
    println!("\n=== 趋势检测自查 ===");
    self_check_trend(&result.bi_zs, &result.bi);

    // 也分析日线
    println!("\n========== 日线分析 ==========");
    let daily = manager.get_klines("600519", TimeFrame::D, None, None)
        .expect("加载日线失败");
    println!("日线: {} 根", daily.len());
    if let (Some(f), Some(l)) = (daily.first(), daily.last()) {
        println!("日线范围: {} ~ {}", f.dt, l.dt);
    }
    // 只看最近200根日线（约1年）
    let recent_daily: Vec<_> = daily.iter().rev().take(200).rev().cloned().collect();
    println!("最近 {} 根日线: {} ~ {}",
        recent_daily.len(),
        recent_daily.first().map(|k| k.dt.as_str()).unwrap_or("-"),
        recent_daily.last().map(|k| k.dt.as_str()).unwrap_or("-"));
    let d_macd = calc_macd(&recent_daily, 12, 26, 9);
    let d_result = CzscAnalyzer::analyze(&recent_daily, &d_macd);
    println!("日线 笔:{} 中枢:{} 背驰:{} 买卖点:{}",
        d_result.bi.len(), d_result.bi_zs.len(),
        d_result.beichi.len(), d_result.buy_sell.len());
    for bc in &d_result.beichi {
        println!("  BC: type={} sub={} dir={} idx={} reason={}",
            bc.bc_type, bc.bc_sub_type, bc.direction, bc.index, bc.reason);
    }
    let mut d_by_type: HashMap<&str, Vec<_>> = HashMap::new();
    for bs in &d_result.buy_sell {
        d_by_type.entry(bs.bs_type.as_str()).or_default().push(bs);
    }
    for t in &["1buy", "2buy", "3buy", "1sell", "2sell", "3sell"] {
        if let Some(pts) = d_by_type.get(t) {
            println!("  {}: {} 个", t, pts.len());
            for p in pts {
                let price = recent_daily.get(p.index as usize).map(|k| k.close).unwrap_or(f64::NAN);
                let dt = recent_daily.get(p.index as usize).map(|k| k.dt.as_str()).unwrap_or("OOB");
                println!("    idx={} dt={} price={:.2}", p.index, dt, price);
            }
        }
    }
}

fn self_check_trend(zs: &[ZhongShu], bi: &[yifang_data::Bi]) {
    // 找下跌趋势：两个向下递进的中枢（上下方向需要从笔方向推断）
    // 这里暂时不对方向做过滤，列出所有递进的中枢对
    println!("  笔中枢: {} 个", zs.len());
    for (i, z) in zs.iter().enumerate() {
        println!("    ZS[{}]: idx {}-{} ZG={:.2} ZD={:.2}",
            i, z.start_index, z.end_index, z.zg, z.zd);
    }

    for i in 0..zs.len() {
        let z1 = &zs[i];
        for j in (i+1)..zs.len() {
            let z2 = &zs[j];
            // 上涨递进：Z2的低点 > Z1的高点（中枢上移）
            let up_progression = z2.zd > z1.zg;
            // 下跌递进：Z2的高点 < Z1的低点（中枢下移）
            let down_progression = z2.zg < z1.zd;
            
            if up_progression || down_progression {
                let dir = if up_progression { "上涨递进↑" } else { "下跌递进↓" };
                println!("\n  {}: ZS[{}]({:.2}~{:.2}) → ZS[{}]({:.2}~{:.2})",
                    dir,
                    i, z1.zd, z1.zg, j, z2.zd, z2.zg);
                println!("    ZS[{}]: idx {}-{}, ZS[{}]: idx {}-{}",
                    i, z1.start_index, z1.end_index,
                    j, z2.start_index, z2.end_index);
                
                // 找两个中枢之间的向下段（如果下跌递进）或向上段（如果上涨递进）
                let target_dir = if down_progression { "down" } else { "up" };
                for (bi_idx, b) in bi.iter().enumerate() {
                    if b.direction != target_dir { continue; }
                    
                    // b段：起点在ZS1之后（>=ZS1.start_index），终点在ZS2之前（< ZS2.start_index），且离开ZS1范围
                    let is_b = b.start_index >= z1.start_index
                        && b.end_index > z1.end_index
                        && b.end_index <= z2.start_index;
                    
                    // c段：起点在ZS2区域内（>=ZS2.start_index），终点突破ZS2（>ZS2.end_index）
                    let is_c = b.start_index >= z2.start_index
                        && b.end_index > z2.end_index;
                    
                    if is_b || is_c {
                        let label = if is_b && is_c { "【b+c重叠?】" }
                            else if is_b { "← b段" }
                            else { "← c段" };
                        println!("      笔[{}]: {} {:.2}→{:.2} idx={}→{} {}",
                            bi_idx, b.direction, b.start_price, b.end_price,
                            b.start_index, b.end_index, label);
                    }
                }
            }
        }
    }
}
