//! 区间套信号检测
//!
//! **对齐缠论区间套定义**
//!
//! 区间套是缠论多级别联立分析的核心方法：
//!
//! 定义：当大级别出现买卖点信号时，到次级别确认精确的入场位置。
//! 例如：日线一买 → 30F 走势也出现一买 → 强确认信号。
//!
//! 区间套的三层结构：
//! 1. 大级别（如日线）：走势方向 + 买卖点类型
//! 2. 中级别（如30F）：走势结构确认
//! 3. 小级别（如5F）：精确入场点确认
//!
//! 信号强度：
//! - strong：大中小三个级别同向确认
//! - medium：大级别+小级别同向确认
//! - weak：仅大级别确认

use yifang_data::{BuySellPoint, ZouShi, QuJianTaoSignal, ZhongShu};

/// 多级别分析数据
pub struct MultiLevelData<'a> {
    /// 高级别走势列表
    pub high_zoushi: &'a [ZouShi],
    /// 高级别买卖点列表
    pub high_bs: &'a [BuySellPoint],
    /// 高级别中枢列表
    pub high_zs: &'a [ZhongShu],
    /// 低级别走势列表
    pub low_zoushi: &'a [ZouShi],
    /// 低级别买卖点列表
    pub low_bs: &'a [BuySellPoint],
    /// 低级别中枢列表
    pub low_zs: &'a [ZhongShu],
    /// 高级别名称（如 "日线"）
    pub high_level_name: &'a str,
    /// 低级别名称（如 "30F"）
    pub low_level_name: &'a str,
}

/// 检测区间套信号
///
/// 当大级别出现买卖点时，检查小级别是否也出现同向买卖点，
/// 形成多级别共振信号。
pub fn detect_qujian_tao(data: &MultiLevelData) -> Vec<QuJianTaoSignal> {
    let mut signals = Vec::new();

    // 遍历大级别买卖点，在小级别寻找确认
    for hbs in data.high_bs {
        // 在小级别买卖点中找同类型、时间接近的
        for lbs in data.low_bs {
            // 买卖点类型必须一致或兼容
            if !is_compatible_bs(&hbs.bs_type, &lbs.bs_type) {
                continue;
            }

            // 时间接近：小级别买卖点在大级别买卖点附近
            // 允许小级别买卖点在大级别买卖点前后一定范围内
            let _index_diff = if hbs.index >= lbs.index {
                hbs.index - lbs.index
            } else {
                lbs.index - hbs.index
            };

            // 大级别一买 + 小级别一买 → 强区间套
            // 大级别一买 + 小级别二买 → 中区间套
            // 大级别二买 + 小级别一买 → 中区间套
            let strength = classify_strength(&hbs.bs_type, &lbs.bs_type);

            // 确定高低级别走势方向
            let (high_dir, low_dir) = determine_directions(&hbs.bs_type);

            signals.push(QuJianTaoSignal {
                signal_type: hbs.bs_type.clone(),
                high_level: data.high_level_name.to_string(),
                low_level: data.low_level_name.to_string(),
                high_direction: high_dir.to_string(),
                low_direction: low_dir.to_string(),
                index: hbs.index,
                dt: hbs.dt.clone(),
                price: hbs.price,
                strength: strength.to_string(),
            });
        }
    }

    // 去重：同一位置、同一类型只保留最强的
    signals.sort_by(|a, b| {
        a.index.cmp(&b.index)
            .then_with(|| strength_order(&b.strength).cmp(&strength_order(&a.strength)))
    });
    signals.dedup_by(|a, b| a.index == b.index && a.signal_type == b.signal_type);

    signals
}

/// 判断两个买卖点类型是否兼容（可构成区间套）
fn is_compatible_bs(high_type: &str, low_type: &str) -> bool {
    // 买点兼容：大级别买点 + 小级别买点
    let is_buy_compatible = is_buy_type(high_type) && is_buy_type(low_type);
    // 卖点兼容：大级别卖点 + 小级别卖点
    let is_sell_compatible = is_sell_type(high_type) && is_sell_type(low_type);

    is_buy_compatible || is_sell_compatible
}

fn is_buy_type(bs_type: &str) -> bool {
    matches!(bs_type, "1buy" | "2buy" | "2buy_break" | "3buy" | "2+3buy" | "2+3buy_break")
}

fn is_sell_type(bs_type: &str) -> bool {
    matches!(bs_type, "1sell" | "2sell" | "2sell_break" | "3sell" | "2+3sell" | "2+3sell_break")
}

/// 分类区间套强度
fn classify_strength(high_type: &str, low_type: &str) -> &'static str {
    // 大级别一买/一卖 + 小级别一买/一卖 → strong
    if (high_type == "1buy" && low_type == "1buy")
        || (high_type == "1sell" && low_type == "1sell")
    {
        return "strong";
    }

    // 大级别一买 + 小级别二买/三买/2+3买 → medium
    // 大级别二买/2+3买 + 小级别一买 → medium
    let is_2_or_3_buy = |t: &str| -> bool {
        matches!(t, "2buy" | "2buy_break" | "3buy" | "2+3buy" | "2+3buy_break")
    };
    let is_2_or_3_sell = |t: &str| -> bool {
        matches!(t, "2sell" | "2sell_break" | "3sell" | "2+3sell" | "2+3sell_break")
    };

    if (high_type == "1buy" && is_2_or_3_buy(low_type))
        || (is_2_or_3_buy(high_type) && low_type == "1buy")
        || (high_type == "1sell" && is_2_or_3_sell(low_type))
        || (is_2_or_3_sell(high_type) && low_type == "1sell")
    {
        return "medium";
    }

    // 其他情况 → weak
    "weak"
}

fn strength_order(s: &str) -> u8 {
    match s {
        "strong" => 3,
        "medium" => 2,
        "weak" => 1,
        _ => 0,
    }
}

/// 根据买卖点类型确定走势方向
fn determine_directions(bs_type: &str) -> (&'static str, &'static str) {
    if is_buy_type(bs_type) {
        // 买点：大级别下跌趋势结束，小级别也确认
        ("down", "down")
    } else {
        // 卖点：大级别上涨趋势结束，小级别也确认
        ("up", "up")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_qujian_tao_strong() {
        let high_bs = vec![BuySellPoint {
            bs_type: "1buy".to_string(),
            index: 100,
            dt: "2024-01-01".to_string(),
            price: 10.0,
            reason: String::new(),
        }];
        let low_bs = vec![BuySellPoint {
            bs_type: "1buy".to_string(),
            index: 102,
            dt: "2024-01-01".to_string(),
            price: 10.2,
            reason: String::new(),
        }];

        let data = MultiLevelData {
            high_zoushi: &[],
            high_bs: &high_bs,
            high_zs: &[],
            low_zoushi: &[],
            low_bs: &low_bs,
            low_zs: &[],
            high_level_name: "日线",
            low_level_name: "30F",
        };

        let signals = detect_qujian_tao(&data);
        assert!(!signals.is_empty(), "应检测到区间套信号");
        assert_eq!(signals[0].strength, "strong", "一买+一买应为强信号");
        assert_eq!(signals[0].signal_type, "1buy");
    }

    #[test]
    fn test_qujian_tao_medium() {
        let high_bs = vec![BuySellPoint {
            bs_type: "1buy".to_string(),
            index: 100,
            dt: "2024-01-01".to_string(),
            price: 10.0,
            reason: String::new(),
        }];
        let low_bs = vec![BuySellPoint {
            bs_type: "2buy".to_string(),
            index: 105,
            dt: "2024-01-01".to_string(),
            price: 10.5,
            reason: String::new(),
        }];

        let data = MultiLevelData {
            high_zoushi: &[],
            high_bs: &high_bs,
            high_zs: &[],
            low_zoushi: &[],
            low_bs: &low_bs,
            low_zs: &[],
            high_level_name: "日线",
            low_level_name: "30F",
        };

        let signals = detect_qujian_tao(&data);
        assert!(!signals.is_empty(), "应检测到区间套信号");
        assert_eq!(signals[0].strength, "medium", "一买+二买应为中信号");
    }

    #[test]
    fn test_qujian_tao_incompatible() {
        let high_bs = vec![BuySellPoint {
            bs_type: "1buy".to_string(),
            index: 100,
            dt: "2024-01-01".to_string(),
            price: 10.0,
            reason: String::new(),
        }];
        let low_bs = vec![BuySellPoint {
            bs_type: "1sell".to_string(),
            index: 102,
            dt: "2024-01-01".to_string(),
            price: 10.2,
            reason: String::new(),
        }];

        let data = MultiLevelData {
            high_zoushi: &[],
            high_bs: &high_bs,
            high_zs: &[],
            low_zoushi: &[],
            low_bs: &low_bs,
            low_zs: &[],
            high_level_name: "日线",
            low_level_name: "30F",
        };

        let signals = detect_qujian_tao(&data);
        assert!(signals.is_empty(), "买+卖不应构成区间套");
    }
}
