//! 缠论分析器 — 整合所有缠论分析步骤
//!
//! 完整流程: K 线 → 去包含 → 分型 → 笔 → 线段 → 中枢 → 背驰 → 买卖点 → 走势分解
//!
//! 多级别联立：
//! - 笔级别：笔中枢、笔背驰、笔级别买卖点
//! - 线段级别：线段中枢、线段背驰、线段级别买卖点
//! - 走势级别：走势递归分解（笔→线段→1F走势→5F走势→…）
//! - 区间套：多级别联立确认

use yifang_data::{KLine, CzscResult, MacdData};

use crate::include::remove_include;
use crate::fenxing::{check_fxs, to_fenxing};
use crate::bi::build_bi;
use crate::xd::build_xd;
use crate::zs::{build_bi_zs, build_xd_zs};
use crate::beichi::{detect_bi_beichi, detect_xd_beichi};
use crate::buy_sell::{detect_buy_sell, detect_xd_buy_sell};
use crate::zoushi::build_zoushi_from_xd;

/// 缠论分析器
pub struct CzscAnalyzer;

impl CzscAnalyzer {
    /// 对 K 线序列进行完整缠论分析
    ///
    /// 分析流程：
    /// 1. 去除包含关系
    /// 2. 识别分型
    /// 3. 构建笔
    /// 4. 构建线段
    /// 5. 识别笔中枢 + 线段中枢
    /// 6. 笔背驰 + 线段背驰
    /// 7. 笔级别买卖点 + 线段级别买卖点
    /// 8. 走势递归分解
    pub fn analyze(klines: &[KLine], macd: &MacdData) -> CzscResult {
        if klines.len() < 5 {
            return CzscResult::default();
        }

        // 1. 去除包含关系
        let merged = remove_include(klines);

        // 2. 识别分型
        let fxs = check_fxs(&merged);
        let fenxing = to_fenxing(&fxs);

        // 3. 构建笔
        let bis = build_bi(klines, None);

        // 4. 构建线段（特征序列分型破坏法）
        let xds = build_xd(&bis);

        // 5. 识别中枢
        let bi_zs = build_bi_zs(&bis);
        let xd_zs = build_xd_zs(&xds);

        // 6. 背驰检测（笔级别 + 线段级别）
        let bi_beichi = detect_bi_beichi(&bis, macd, &bi_zs);
        let xd_beichi = detect_xd_beichi(&xds, macd, &xd_zs);
        let mut beichi = bi_beichi.clone();
        beichi.extend(xd_beichi.clone());

        // 7. 买卖点识别（笔级别 + 线段级别）
        // 重要：笔级别买卖点只看笔背驰，线段级别买卖点只看线段背驰，避免重复
        let bi_bs = detect_buy_sell(&bis, &bi_zs, &bi_beichi);
        let xd_bs = detect_xd_buy_sell(&xds, &xd_zs, &xd_beichi);
        let mut buy_sell = bi_bs;
        buy_sell.extend(xd_bs);
        buy_sell.sort_by_key(|p| p.index);
        buy_sell.dedup_by(|a, b| a.index == b.index && a.bs_type == b.bs_type);

        // 8. 走势递归分解
        let zoushi = build_zoushi_from_xd(&xds);

        // 9. 区间套——需要多级别数据，在单级别分析中暂不执行
        //    区间套由 MultiLevelAnalyzer 在多级别联立时使用
        let qujian_tao = Vec::new();

        CzscResult {
            fenxing,
            bi: bis,
            xd: xds,
            bi_zs,
            xd_zs,
            buy_sell,
            beichi,
            zoushi,
            qujian_tao,
        }
    }
}
