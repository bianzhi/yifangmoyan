//! 缠论 + 威科夫融合解读引擎
//!
//! 将缠论买卖点与威科夫事件在 ±5K线窗口内自动关联，
//! 生成融合解读和信号强度评级。

use yifang_data::{
    CzscResult, FusionResult, FusionSignal, WyckoffEvent, WyckoffResult,
};

/// 融合对齐窗口大小（±N根K线）
const ALIGN_WINDOW: u64 = 5;

/// 融合规则表
struct FusionRule {
    czsc_types: &'static [&'static str],
    wyckoff_types: &'static [&'static str],
    interpretation: &'static str,
    strength: u8,
    direction: &'static str,
}

/// 完整融合规则表（对齐产品设计文档2.4节）
const FUSION_RULES: &[FusionRule] = &[
    FusionRule {
        czsc_types: &["1buy"],
        wyckoff_types: &["Spring"],
        interpretation: "底部反转双重确认：缠论一买+威科夫弹簧",
        strength: 5,
        direction: "bullish",
    },
    FusionRule {
        czsc_types: &["1buy"],
        wyckoff_types: &["SC"],
        interpretation: "背驰+卖出高潮见底：缠论一买+威科夫SC",
        strength: 4,
        direction: "bullish",
    },
    FusionRule {
        czsc_types: &["1buy"],
        wyckoff_types: &["ST"],
        interpretation: "底部确认：缠论一买+威科夫二次测试",
        strength: 4,
        direction: "bullish",
    },
    FusionRule {
        czsc_types: &["2buy"],
        wyckoff_types: &["LPS"],
        interpretation: "回调确认吸筹完成：缠论二买+威科夫最后支撑",
        strength: 4,
        direction: "bullish",
    },
    FusionRule {
        czsc_types: &["2buy"],
        wyckoff_types: &["SOS"],
        interpretation: "强势回踩确认：缠论二买+威科夫强势信号",
        strength: 4,
        direction: "bullish",
    },
    FusionRule {
        czsc_types: &["3buy"],
        wyckoff_types: &["JOC"],
        interpretation: "突破确认趋势延续：缠论三买+威科夫跳过小溪",
        strength: 4,
        direction: "bullish",
    },
    FusionRule {
        czsc_types: &["1sell"],
        wyckoff_types: &["UTAD"],
        interpretation: "顶背离+派发冲高：缠论一卖+威科夫UTAD",
        strength: 5,
        direction: "bearish",
    },
    FusionRule {
        czsc_types: &["1sell"],
        wyckoff_types: &["BC"],
        interpretation: "顶背离+买入高潮：缠论一卖+威科夫BC",
        strength: 4,
        direction: "bearish",
    },
    FusionRule {
        czsc_types: &["2sell"],
        wyckoff_types: &["SOW"],
        interpretation: "反弹无力+弱势信号：缠论二卖+威科夫SOW",
        strength: 4,
        direction: "bearish",
    },
    FusionRule {
        czsc_types: &["3sell"],
        wyckoff_types: &["LPSY"],
        interpretation: "中枢破坏+结构破位：缠论三卖+威科夫最后供给",
        strength: 4,
        direction: "bearish",
    },
    FusionRule {
        czsc_types: &["3sell"],
        wyckoff_types: &["SOW"],
        interpretation: "破位确认：缠论三卖+威科夫弱势信号",
        strength: 4,
        direction: "bearish",
    },
    // 阶段转换 + 背驰
    FusionRule {
        czsc_types: &["1buy", "2buy"],
        wyckoff_types: &["AR"],
        interpretation: "底部反转确认：缠论买点+威科夫自动反弹",
        strength: 3,
        direction: "bullish",
    },
    FusionRule {
        czsc_types: &["1sell", "2sell"],
        wyckoff_types: &["PSY"],
        interpretation: "顶部反转预警：缠论卖点+威科夫初步供给",
        strength: 3,
        direction: "bearish",
    },
];

/// 执行融合分析
pub fn analyze_fusion(czsc: &CzscResult, wyckoff: &WyckoffResult) -> FusionResult {
    let mut signals: Vec<FusionSignal> = Vec::new();

    for bs in &czsc.buy_sell {
        // 找窗口内的威科夫事件
        let nearby_events: Vec<&WyckoffEvent> = wyckoff
            .events
            .iter()
            .filter(|e| (e.index as i64 - bs.index as i64).unsigned_abs() <= ALIGN_WINDOW)
            .collect();

        if nearby_events.is_empty() {
            continue;
        }

        // 按规则表匹配
        let mut matched = false;
        for rule in FUSION_RULES {
            if !rule.czsc_types.contains(&bs.bs_type.as_str()) {
                continue;
            }

            let matching_events: Vec<&WyckoffEvent> = nearby_events
                .iter()
                .filter(|e| rule.wyckoff_types.contains(&e.event_type.as_str()))
                .copied()
                .collect();

            if !matching_events.is_empty() {
                signals.push(FusionSignal {
                    czsc_type: bs.bs_type.clone(),
                    wyckoff_events: matching_events.iter().map(|e| e.event_type.clone()).collect(),
                    index: bs.index,
                    dt: bs.dt.clone(),
                    price: bs.price,
                    interpretation: rule.interpretation.to_string(),
                    strength: rule.strength,
                    direction: rule.direction.to_string(),
                });
                matched = true;
                break;
            }
        }

        // 如果没有精确规则匹配，但有附近事件，生成通用融合
        if !matched && !nearby_events.is_empty() {
            let is_bullish = bs.bs_type.contains("buy");
            let wyckoff_names: Vec<String> = nearby_events.iter().map(|e| e.event_type.clone()).collect();
            let interpretation = if is_bullish {
                format!("缠论{}附近出现威科夫事件{}", bs.bs_type, wyckoff_names.join("/"))
            } else {
                format!("缠论{}附近出现威科夫事件{}", bs.bs_type, wyckoff_names.join("/"))
            };

            signals.push(FusionSignal {
                czsc_type: bs.bs_type.clone(),
                wyckoff_events: wyckoff_names,
                index: bs.index,
                dt: bs.dt.clone(),
                price: bs.price,
                interpretation,
                strength: 2,
                direction: if is_bullish { "bullish" } else { "bearish" }.to_string(),
            });
        }
    }

    FusionResult { signals }
}

#[cfg(test)]
mod tests {
    use super::*;
    use yifang_data::BuySellPoint;

    #[test]
    fn test_fusion_1buy_spring() {
        let czsc = CzscResult {
            buy_sell: vec![BuySellPoint {
                bs_type: "1buy".to_string(),
                index: 100,
                dt: "2024-03-15".to_string(),
                price: 10.0,
            }],
            ..Default::default()
        };
        let wyckoff = WyckoffResult {
            events: vec![WyckoffEvent {
                event_type: "Spring".to_string(),
                index: 98,
                dt: "2024-03-14".to_string(),
                price: 9.8,
                description: "Spring test".to_string(),
            }],
            ..Default::default()
        };

        let result = analyze_fusion(&czsc, &wyckoff);
        assert_eq!(result.signals.len(), 1);
        assert_eq!(result.signals[0].strength, 5);
        assert_eq!(result.signals[0].direction, "bullish");
        assert!(result.signals[0].wyckoff_events.contains(&"Spring".to_string()));
    }

    #[test]
    fn test_fusion_1sell_utad() {
        let czsc = CzscResult {
            buy_sell: vec![BuySellPoint {
                bs_type: "1sell".to_string(),
                index: 200,
                dt: "2024-06-01".to_string(),
                price: 50.0,
            }],
            ..Default::default()
        };
        let wyckoff = WyckoffResult {
            events: vec![WyckoffEvent {
                event_type: "UTAD".to_string(),
                index: 202,
                dt: "2024-06-03".to_string(),
                price: 52.0,
                description: "UTAD test".to_string(),
            }],
            ..Default::default()
        };

        let result = analyze_fusion(&czsc, &wyckoff);
        assert_eq!(result.signals.len(), 1);
        assert_eq!(result.signals[0].strength, 5);
        assert_eq!(result.signals[0].direction, "bearish");
    }

    #[test]
    fn test_no_fusion_no_match() {
        let czsc = CzscResult {
            buy_sell: vec![BuySellPoint {
                bs_type: "1buy".to_string(),
                index: 100,
                dt: "2024-03-15".to_string(),
                price: 10.0,
            }],
            ..Default::default()
        };
        let wyckoff = WyckoffResult {
            events: vec![WyckoffEvent {
                event_type: "BC".to_string(),
                index: 10,
                dt: "2024-01-01".to_string(),
                price: 20.0,
                description: "BC test".to_string(),
            }],
            ..Default::default()
        };

        let result = analyze_fusion(&czsc, &wyckoff);
        assert!(result.signals.is_empty());
    }

    #[test]
    fn test_fusion_generic_match() {
        let czsc = CzscResult {
            buy_sell: vec![BuySellPoint {
                bs_type: "2buy".to_string(),
                index: 100,
                dt: "2024-03-15".to_string(),
                price: 10.0,
            }],
            ..Default::default()
        };
        let wyckoff = WyckoffResult {
            events: vec![WyckoffEvent {
                event_type: "SC".to_string(),
                index: 102,
                dt: "2024-03-16".to_string(),
                price: 9.5,
                description: "SC test".to_string(),
            }],
            ..Default::default()
        };

        let result = analyze_fusion(&czsc, &wyckoff);
        assert_eq!(result.signals.len(), 1);
        assert_eq!(result.signals[0].strength, 2); // 通用融合
    }
}
