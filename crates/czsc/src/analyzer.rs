//! 缠论分析器 — 整合所有缠论分析步骤
//!
//! 流程: K 线 → 去包含 → 分型 → 笔 → 线段 → 中枢 → 背驰 → 买卖点

use yifang_data::{KLine, CzscResult, MacdData};

use crate::include::remove_include;
use crate::fenxing::{check_fxs, to_fenxing};
use crate::bi::build_bi;
use crate::xd::build_xd;
use crate::zs::{build_bi_zs, build_xd_zs};
use crate::beichi::detect_bi_beichi;
use crate::buy_sell::detect_buy_sell;

/// 缠论分析器
pub struct CzscAnalyzer;

impl CzscAnalyzer {
    /// 对 K 线序列进行完整缠论分析
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

        // 4. 构建线段
        let xds = build_xd(&bis);

        // 5. 识别中枢
        let bi_zs = build_bi_zs(&bis);
        let xd_zs = build_xd_zs(&xds);

        // 6. 背驰检测
        let beichi = detect_bi_beichi(&bis, macd);

        // 7. 买卖点识别
        let buy_sell = detect_buy_sell(&bis, &bi_zs, &beichi);

        CzscResult {
            fenxing,
            bi: bis,
            xd: xds,
            bi_zs,
            xd_zs,
            buy_sell,
            beichi,
        }
    }
}
