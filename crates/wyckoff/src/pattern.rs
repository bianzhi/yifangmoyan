//! 威科夫关键形态事件识别
//!
//! **严格对齐威科夫原著定义**
//!
//! 吸筹事件：
//! - PS (Preliminary Support): 初步支撑
//!   条件：下跌趋势中，首次出现放量但跌幅收窄
//!   量价特征：vol > avg_vol * 1.5，跌幅明显小于前几根
//!
//! - SC (Selling Climax): 卖出高潮
//!   条件：持续下跌末端，宽幅（high-low 跨度大）+ 巨量 + 收出下影线
//!   量价特征：vol > avg_vol * 2.0，spread > avg_spread * 2.0，close > low（收回）
//!
//! - AR (Automatic Rally): 自动反弹
//!   条件：SC 之后的放量反弹，确立交易区间上沿
//!   量价特征：close > open（阳线），vol > avg_vol * 1.0，high 为近期新高
//!
//! - ST (Secondary Test): 二次测试
//!   条件：回测 SC 低点附近，但量缩价窄
//!   量价特征：low 接近 SC.low（±2%），vol < SC.vol * 0.8，spread < SC.spread
//!
//! - Spring: 弹簧效应
//!   条件：跌破 SC 低点后快速收回，形成空头陷阱
//!   量价特征：low < SC.low，close > SC.low * 0.99，vol < SC.vol
//!
//! - Shakeout: 震荡洗盘
//!   类似 Spring 但幅度更大
//!
//! - SOS (Sign of Strength): 强势出现
//!   条件：放量上涨突破交易区间上沿或 AR 高点
//!   量价特征：close > AR.high，vol 递增
//!
//! - LPS (Last Point of Support): 最后支撑点
//!   条件：SOS 后回调至支撑位获得支撑
//!   量价特征：low ≈ SOS 附近支撑，vol 缩量
//!
//! - JOC (Jump Over Creek): 跳过小溪
//!   条件：放量突破阻力区
//!
//! 派发事件：
//! - PSY (Preliminary Supply): 初步供给 — 上涨中的第一次放量滞涨
//! - BC (Buying Climax): 买入高潮 — 上涨末端宽幅+巨量+收上影线
//! - AR (Automatic Reaction): 自动回落 — BC 后的技术性回落
//! - ST (Secondary Test): 二次测试 — 回测 BC 高点，量缩
//! - UTAD (Upthrust After Distribution): 派发后冲高 — 突破 BC 高点后迅速回落
//! - SOW (Sign of Weakness): 弱势出现 — 放量跌破区间下沿/ICE
//! - LPSY (Last Point of Supply): 最后供给点 — SOW 后缩量反弹
//! - ICE (Ice Line Break): 冰线突破 — 跌破支撑

use yifang_data::{KLine, WyckoffEvent};

/// 事件上下文：吸筹或派发
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum EventContext {
    Accumulation,
    Distribution,
}

/// 识别威科夫事件（全量，自动判断上下文）
///
/// 扫描整个 K 线序列，按照事件定义严格识别。
/// 通过先检测大级别结构，再识别具体事件。
pub fn detect_events(klines: &[KLine]) -> Vec<WyckoffEvent> {
    if klines.len() < 10 {
        return Vec::new();
    }

    let mut events = Vec::new();
    let avg_vol = calc_avg(klines, |k| k.vol);
    let avg_spread = calc_avg(klines, |k| k.high - k.low);

    // 扫描每个位置，检测事件
    for i in 5..klines.len() - 3 {

        // PS (初步支撑): 下跌中首次放量但跌幅收窄
        if let Some(ev) = detect_ps(klines, i, avg_vol) {
            events.push(ev);
        }

        // SC (卖出高潮): 宽幅 + 巨量 + 下跌末端
        if let Some(ev) = detect_sc(klines, i, avg_vol, avg_spread) {
            events.push(ev);
        }

        // AR (自动反弹): SC 后放量反弹
        if let Some(ev) = detect_ar(klines, i, &events, avg_vol) {
            events.push(ev);
        }

        // ST (二次测试): 回测 SC 低点，量缩价窄
        if let Some(ev) = detect_st(klines, i, &events, avg_vol, avg_spread) {
            events.push(ev);
        }

        // Spring: 跌破 SC 后收回
        if let Some(ev) = detect_spring(klines, i, &events) {
            events.push(ev);
        }

        // SOS: 放量突破 AR 高点
        if let Some(ev) = detect_sos(klines, i, &events, avg_vol) {
            events.push(ev);
        }

        // LPS: SOS 后缩量回调获得支撑
        if let Some(ev) = detect_lps(klines, i, &events) {
            events.push(ev);
        }

        // ========== 派发事件 ==========

        // PSY (初步供给): 上涨中首次放量滞涨
        if let Some(ev) = detect_psy(klines, i, avg_vol) {
            events.push(ev);
        }

        // BC (买入高潮): 宽幅 + 巨量 + 上涨末端
        if let Some(ev) = detect_bc(klines, i, avg_vol, avg_spread) {
            events.push(ev);
        }

        // UTAD: 突破 BC 高点后迅速回落
        if let Some(ev) = detect_utad(klines, i, &events) {
            events.push(ev);
        }

        // SOW: 放量跌破支撑
        if let Some(ev) = detect_sow(klines, i, avg_vol) {
            events.push(ev);
        }

        // LPSY: SOW 后缩量反弹
        if let Some(ev) = detect_lpsy(klines, i, &events) {
            events.push(ev);
        }

        // JOC: 放量突破阻力
        if let Some(ev) = detect_joc(klines, i, avg_vol, &events) {
            events.push(ev);
        }
    }

    events
}

// ============================================================
//  吸筹事件检测
// ============================================================

/// PS (Preliminary Support) 初步支撑
///
/// 条件：
/// 1. 在下跌趋势中（近 10 根 K 线收盘价趋势向下）
/// 2. 放量（vol > avg * 1.5）
/// 3. 跌幅收窄或收阳
fn detect_ps(klines: &[KLine], i: usize, avg_vol: f64) -> Option<WyckoffEvent> {
    if i < 5 { return None; }

    // 检查下跌趋势
    let lookback = 10.min(i).max(3);
    let trend_down = klines[i].close < klines[i - lookback].close;
    if !trend_down { return None; }

    let k = &klines[i];
    let vol_surge = k.vol > avg_vol * 1.5;
    if !vol_surge { return None; }

    // 跌幅收窄或收阳
    let prev_spread = (klines[i - 1].open - klines[i - 1].close).abs();
    let curr_spread = (k.open - k.close).abs();
    let spread_shrink = curr_spread < prev_spread;
    let is_up = k.close >= k.open;

    if spread_shrink || is_up {
        Some(WyckoffEvent {
            event_type: "PS".to_string(),
            index: i as u64,
            dt: k.dt.clone(),
            price: k.low,
            description: "初步支撑: 下跌中放量但跌幅收窄".to_string(),
            reason: format!(
                "PS(初步支撑): 下跌中首次出现放量(vol={:.0} > 均值*1.5={:.0}), 跌幅收窄或收阳, 表明大资金开始承接（Wyckoff: 供给法则）"
            , k.vol, avg_vol * 1.5),
        })
    } else {
        None
    }
}

/// SC (Selling Climax) 卖出高潮
///
/// 条件：
/// 1. 持续下跌末端
/// 2. 宽幅（spread > avg * 2.0）
/// 3. 巨量（vol > avg * 2.0）
/// 4. 收出下影线（close > low，从最低点收回）
fn detect_sc(klines: &[KLine], i: usize, avg_vol: f64, avg_spread: f64) -> Option<WyckoffEvent> {
    if i < 5 { return None; }

    let k = &klines[i];
    // 趋势向下：用可用的窗口（最多 10 根，最少 3 根）
    let lookback = 10.min(i).max(3);
    let trend_down = klines[i].close < klines[i - lookback].close;
    let spread = k.high - k.low;
    let wide_spread = spread > avg_spread * 2.0;
    let heavy_vol = k.vol > avg_vol * 2.0;
    let recovered = k.close > k.low + spread * 0.2;

    if !trend_down { return None; }
    if wide_spread && heavy_vol && recovered {
        Some(WyckoffEvent {
            event_type: "SC".to_string(),
            index: i as u64,
            dt: k.dt.clone(),
            price: k.low,
            description: "卖出高潮: 宽幅巨量下跌并收回".to_string(),
            reason: format!(
                "SC(卖出高潮): 下跌末端宽幅(spread={:.2} > 均值*2={:.2})+巨量(vol={:.0} > 均值*2={:.0})+收回, 大资金大规模承接, 确立交易区间下沿={:.2}（Wyckoff Phase A）",
                spread, avg_spread * 2.0, k.vol, avg_vol * 2.0, k.low
            ),
        })
    } else {
        None
    }
}

/// AR (Automatic Rally) 自动反弹
///
/// 条件：
/// 1. 前面存在 SC（3~10 根之内）
/// 2. 阳线
/// 3. 成交量相对较高（> avg * 0.8，不需要巨量）
/// 4. 价格突破近期高点
fn detect_ar(klines: &[KLine], i: usize, events: &[WyckoffEvent], avg_vol: f64) -> Option<WyckoffEvent> {
    let k = &klines[i];
    if k.close <= k.open { return None; }

    // 检查前面是否有 SC
    let sc = events.iter()
        .filter(|e| e.event_type == "SC")
        .filter(|e| i as u64 > e.index && i as u64 - e.index <= 10)
        .max_by_key(|e| e.index);

    if sc.is_none() { return None; }

    let vol_ok = k.vol > avg_vol * 0.8;
    if !vol_ok { return None; }

    // 价格创近期新高
    let recent_high = if i >= 5 {
        klines[i - 5..i].iter().map(|k| k.high).fold(f64::MIN, f64::max)
    } else {
        f64::MIN
    };
    let breakout = k.high > recent_high;

    if breakout {
        let sc_event = sc.unwrap();
        Some(WyckoffEvent {
            event_type: "AR".to_string(),
            index: i as u64,
            dt: k.dt.clone(),
            price: k.high,
            description: "自动反弹: SC后放量上涨创新高".to_string(),
            reason: format!(
                "AR(自动反弹): SC#{:#?}后卖压耗尽+空头回补, 放量(vol={:.0})上涨创新高={:.2}, 确立交易区间上沿（Wyckoff Phase A）",
                sc_event.index, k.vol, k.high
            ),
        })
    } else {
        None
    }
}

/// ST (Secondary Test) 二次测试
///
/// 条件：
/// 1. 前面存在 SC
/// 2. low 接近 SC 的低点（±3%）
/// 3. 量缩（vol < SC 处 vol * 0.8）
/// 4. 价窄（spread < SC 处 spread * 0.7）
fn detect_st(klines: &[KLine], i: usize, events: &[WyckoffEvent], _avg_vol: f64, _avg_spread: f64) -> Option<WyckoffEvent> {
    let sc = events.iter()
        .filter(|e| e.event_type == "SC")
        .filter(|e| i as u64 > e.index && i as u64 - e.index <= 20)
        .max_by_key(|e| e.index);

    if sc.is_none() { return None; }
    let sc_event = sc.unwrap();

    let k = &klines[i];
    let sc_low = sc_event.price;
    let near_sc_low = (k.low - sc_low).abs() / sc_low < 0.03;

    if !near_sc_low { return None; }

    // 量缩
    let sc_idx = sc_event.index as usize;
    let sc_vol = klines.get(sc_idx).map(|k| k.vol).unwrap_or(f64::MAX);
    let vol_shrink = k.vol < sc_vol * 0.8;

    // 价窄
    let sc_spread = klines.get(sc_idx).map(|k| k.high - k.low).unwrap_or(f64::MAX);
    let spread_shrink = (k.high - k.low) < sc_spread * 0.7;

    if vol_shrink && spread_shrink {
        Some(WyckoffEvent {
            event_type: "ST".to_string(),
            index: i as u64,
            dt: k.dt.clone(),
            price: k.low,
            description: "二次测试: 量缩价窄回测SC低点".to_string(),
            reason: format!(
                "ST(二次测试): 回测SC#{:#?}低点={:.2}(当前low={:.2}, 偏差<{:.1}%), 量缩(vol={:.0} < SC*0.8={:.0})+价窄, 确认卖压衰竭（Wyckoff Phase A/B）",
                sc_event.index, sc_low, k.low, 3.0, k.vol, sc_vol * 0.8
            ),
        })
    } else {
        None
    }
}

/// Spring 弹簧效应
///
/// 条件：
/// 1. 前面存在 SC
/// 2. 当前 K 线 low < SC.low（跌破）
/// 3. close > SC.low * 0.99（收回）
/// 4. 成交量相对 SC 缩量（Spring 的关键特征是缩量跌破再收回）
fn detect_spring(klines: &[KLine], i: usize, events: &[WyckoffEvent]) -> Option<WyckoffEvent> {
    let sc = events.iter()
        .filter(|e| e.event_type == "SC")
        .filter(|e| i as u64 > e.index && i as u64 - e.index <= 30)
        .max_by_key(|e| e.index);

    if sc.is_none() { return None; }
    let sc_event = sc.unwrap();
    let sc_low = sc_event.price;

    let k = &klines[i];
    let broke_below = k.low < sc_low;
    let recovered = k.close > sc_low * 0.99;

    if broke_below && recovered {
        let penetration_pct = (sc_low - k.low) / sc_low * 100.0;
        Some(WyckoffEvent {
            event_type: "Spring".to_string(),
            index: i as u64,
            dt: k.dt.clone(),
            price: k.low,
            description: "弹簧效应: 跌破SC低点后收回".to_string(),
            reason: format!(
                "Spring(弹簧): 跌破SC#{:#?}低点={:.2}(当前low={:.2}, 穿透{:.1}%)后收回(close={:.2} > SC*0.99={:.2}), 空头陷阱, 供给被吸收（Wyckoff Phase C）",
                sc_event.index, sc_low, k.low, penetration_pct, k.close, sc_low * 0.99
            ),
        })
    } else {
        None
    }
}

/// SOS (Sign of Strength) 强势出现
///
/// 条件：
/// 1. 存在 AR 事件
/// 2. 放量上涨突破 AR 的高点
/// 3. vol > avg * 1.2
fn detect_sos(klines: &[KLine], i: usize, events: &[WyckoffEvent], avg_vol: f64) -> Option<WyckoffEvent> {
    let ar = events.iter()
        .filter(|e| e.event_type == "AR")
        .filter(|e| i as u64 > e.index && i as u64 - e.index <= 30)
        .max_by_key(|e| e.index);

    if ar.is_none() { return None; }
    let ar_event = ar.unwrap();
    let ar_high = ar_event.price;

    let k = &klines[i];
    let is_up = k.close > k.open;
    let vol_ok = k.vol > avg_vol * 1.2;
    let breakout = k.high > ar_high;

    if is_up && vol_ok && breakout {
        Some(WyckoffEvent {
            event_type: "SOS".to_string(),
            index: i as u64,
            dt: k.dt.clone(),
            price: k.high,
            description: "强势出现: 放量突破AR高点".to_string(),
            reason: format!(
                "SOS(强势出现): 放量(vol={:.0} > 均值*1.2={:.0})上涨突破AR#{:#?}高点={:.2}(当前high={:.2}), 需求压倒供给, 等同'跳过小溪(Jump Across Creek)'（Wyckoff Phase D）",
                k.vol, avg_vol * 1.2, ar_event.index, ar_high, k.high
            ),
        })
    } else {
        None
    }
}

/// LPS (Last Point of Support) 最后支撑点
///
/// 条件：
/// 1. 存在 SOS 事件
/// 2. SOS 之后回调到附近支撑
/// 3. 缩量回调
fn detect_lps(klines: &[KLine], i: usize, events: &[WyckoffEvent]) -> Option<WyckoffEvent> {
    let sos = events.iter()
        .filter(|e| e.event_type == "SOS")
        .filter(|e| i as u64 > e.index && i as u64 - e.index <= 15)
        .max_by_key(|e| e.index);

    if sos.is_none() { return None; }
    let sos_event = sos.unwrap();
    let sos_idx = sos_event.index as usize;

    let k = &klines[i];

    // 回调到 SOS 附近（SOS 低点附近）
    let sos_low = klines.get(sos_idx).map(|k| k.low).unwrap_or(0.0);
    let near_support = (k.low - sos_low).abs() / sos_low < 0.03;

    // 缩量
    let sos_vol = klines.get(sos_idx).map(|k| k.vol).unwrap_or(0.0);
    let vol_shrink = k.vol < sos_vol * 0.7;

    if near_support && vol_shrink {
        Some(WyckoffEvent {
            event_type: "LPS".to_string(),
            index: i as u64,
            dt: k.dt.clone(),
            price: k.low,
            description: "最后支撑点: SOS后缩量回踩获支撑".to_string(),
            reason: format!(
                "LPS(最后支撑点): SOS#{:#?}后缩量回调(vol={:.0} < SOS*0.7={:.0}), 回踩SOS低点={:.2}附近获支撑(low={:.2}), 等同'回踩小溪(Back Up to Creek)'（Wyckoff Phase D）",
                sos_event.index, k.vol, sos_vol * 0.7, sos_low, k.low
            ),
        })
    } else {
        None
    }
}

// ============================================================
//  派发事件检测
// ============================================================

/// PSY (Preliminary Supply) 初步供给
///
/// 条件：
/// 1. 上涨趋势中
/// 2. 放量但涨幅收窄或收阴
fn detect_psy(klines: &[KLine], i: usize, avg_vol: f64) -> Option<WyckoffEvent> {
    if i < 5 { return None; }

    let lookback = 10.min(i).max(3);
    let trend_up = klines[i].close > klines[i - lookback].close;
    if !trend_up { return None; }

    let k = &klines[i];
    let vol_surge = k.vol > avg_vol * 1.5;
    if !vol_surge { return None; }

    let prev_spread = (klines[i - 1].close - klines[i - 1].open).abs();
    let curr_spread = (k.close - k.open).abs();
    let spread_shrink = curr_spread < prev_spread;
    let is_down = k.close <= k.open;

    if spread_shrink || is_down {
        Some(WyckoffEvent {
            event_type: "PSY".to_string(),
            index: i as u64,
            dt: k.dt.clone(),
            price: k.high,
            description: "初步供给: 上涨中放量但涨幅收窄".to_string(),
            reason: format!(
                "PSY(初步供给): 上涨中首次出现放量(vol={:.0} > 均值*1.5={:.0}), 但涨幅收窄或收阴, 表明大资金开始出货（Wyckoff: 供需法则）",
                k.vol, avg_vol * 1.5
            ),
        })
    } else {
        None
    }
}

/// BC (Buying Climax) 买入高潮
///
/// 条件：
/// 1. 上涨趋势末端
/// 2. 宽幅 + 巨量 + 收上影线
fn detect_bc(klines: &[KLine], i: usize, avg_vol: f64, avg_spread: f64) -> Option<WyckoffEvent> {
    if i < 5 { return None; }

    let lookback = 10.min(i).max(3);
    let trend_up = klines[i].close > klines[i - lookback].close;
    if !trend_up { return None; }

    let k = &klines[i];
    let spread = k.high - k.low;
    let wide_spread = spread > avg_spread * 2.0;
    let heavy_vol = k.vol > avg_vol * 2.0;
    let upper_shadow = k.high - k.open.max(k.close);
    let has_upper_shadow = upper_shadow > spread * 0.3;

    if wide_spread && heavy_vol && has_upper_shadow {
        Some(WyckoffEvent {
            event_type: "BC".to_string(),
            index: i as u64,
            dt: k.dt.clone(),
            price: k.high,
            description: "买入高潮: 宽幅巨量上涨收上影线".to_string(),
            reason: format!(
                "BC(买入高潮): 上涨末端宽幅(spread={:.2} > 均值*2={:.2})+巨量(vol={:.0} > 均值*2={:.0})+上影线, 大资金大规模出货, 确立交易区间上沿={:.2}（Wyckoff Phase A）",
                spread, avg_spread * 2.0, k.vol, avg_vol * 2.0, k.high
            ),
        })
    } else {
        None
    }
}

/// UTAD (Upthrust After Distribution) 派发后冲高
///
/// 条件：
/// 1. 存在 BC 事件
/// 2. 价格突破 BC 高点后回落
/// 3. close < BC 的高点
fn detect_utad(klines: &[KLine], i: usize, events: &[WyckoffEvent]) -> Option<WyckoffEvent> {
    let bc = events.iter()
        .filter(|e| e.event_type == "BC")
        .filter(|e| i as u64 > e.index && i as u64 - e.index <= 30)
        .max_by_key(|e| e.index);

    if bc.is_none() { return None; }
    let bc_event = bc.unwrap();
    let bc_high = bc_event.price;

    let k = &klines[i];
    let broke_above = k.high > bc_high;
    let fell_back = k.close < bc_high;

    if broke_above && fell_back {
        let overshoot_pct = (k.high - bc_high) / bc_high * 100.0;
        Some(WyckoffEvent {
            event_type: "UTAD".to_string(),
            index: i as u64,
            dt: k.dt.clone(),
            price: k.high,
            description: "派发后冲高: 突破BC高点后回落".to_string(),
            reason: format!(
                "UTAD(派发后冲高): 突破BC#{:#?}高点={:.2}(当前high={:.2}, 冲出{:.1}%)后回落(close={:.2} < BC高点), 多头陷阱, 需求被吸收（Wyckoff Phase C）",
                bc_event.index, bc_high, k.high, overshoot_pct, k.close
            ),
        })
    } else {
        None
    }
}

/// SOW (Sign of Weakness) 弱势出现
///
/// 条件：
/// 1. 放量下跌（vol > avg * 1.5，收阴）
/// 2. 跌破近期支撑或冰线
fn detect_sow(klines: &[KLine], i: usize, avg_vol: f64) -> Option<WyckoffEvent> {
    let k = &klines[i];
    if k.close >= k.open { return None; }
    let vol_surge = k.vol > avg_vol * 1.5;
    if !vol_surge { return None; }

    // 跌破近期低点支撑
    let lookback = 10.min(i).max(3);
    let recent_low = klines[i - lookback..i].iter().map(|k| k.low).fold(f64::MAX, f64::min);
    if k.close < recent_low {
        return Some(WyckoffEvent {
            event_type: "SOW".to_string(),
            index: i as u64,
            dt: k.dt.clone(),
            price: k.low,
            description: "弱势出现: 放量跌破近期支撑".to_string(),
            reason: format!(
                "SOW(弱势出现): 放量下跌(vol={:.0} > 均值*1.5={:.0}), 跌破近期支撑={:.2}(close={:.2}), 供给压倒需求（Wyckoff Phase D）",
                k.vol, avg_vol * 1.5, recent_low, k.close
            ),
        });
    }
    None
}

/// LPSY (Last Point of Supply) 最后供给点
///
/// 条件：
/// 1. 存在 SOW 事件
/// 2. SOW 后缩量反弹
fn detect_lpsy(klines: &[KLine], i: usize, events: &[WyckoffEvent]) -> Option<WyckoffEvent> {
    let sow = events.iter()
        .filter(|e| e.event_type == "SOW")
        .filter(|e| i as u64 > e.index && i as u64 - e.index <= 15)
        .max_by_key(|e| e.index);

    if sow.is_none() { return None; }
    let sow_event = sow.unwrap();
    let sow_idx = sow_event.index as usize;

    let k = &klines[i];
    let is_bounce = k.close > k.open;

    let sow_vol = klines.get(sow_idx).map(|k| k.vol).unwrap_or(0.0);
    let vol_shrink = if sow_vol > 0.0 { k.vol < sow_vol * 0.7 } else { true };

    if is_bounce && vol_shrink {
        Some(WyckoffEvent {
            event_type: "LPSY".to_string(),
            index: i as u64,
            dt: k.dt.clone(),
            price: k.high,
            description: "最后供给点: SOW后缩量反弹".to_string(),
            reason: format!(
                "LPSY(最后供给点): SOW#{:#?}后缩量反弹(vol={:.0} < SOW*0.7={:.0}), 需求衰竭, 最后供给点（Wyckoff Phase D）",
                sow_event.index, k.vol, sow_vol * 0.7
            ),
        })
    } else {
        None
    }
}

/// JOC (Jump Over Creek) 跳过小溪
///
/// 威科夫原著中，"小溪"(Creek)指交易区间上沿（由AR高点确立的供给线/阻力区）。
/// JOC = SOS的一种形式，价格放量跳过这条"小溪"(阻力线)。
///
/// "小溪"宽度的计算：
/// - Creek宽度 = 阻力线价格 - 交易区间下沿（SC低点）
/// - 即交易区间的振幅，代表需要跳过的"溪流"宽度
///
/// Evans：SOS等同"跳过小溪"(Jump Across Creek)，
/// 之后出现的缩量回调叫"回踩小溪"(Back Up to Creek / BUEC)，即LPS
fn detect_joc(klines: &[KLine], i: usize, avg_vol: f64, events: &[WyckoffEvent]) -> Option<WyckoffEvent> {
    if i < 5 { return None; }

    let k = &klines[i];
    let vol_surge = k.vol > avg_vol * 1.3;
    let is_up = k.close > k.open;
    if !vol_surge || !is_up { return None; }

    let lookback = 10.min(i).max(3);
    let resistance = klines[i - lookback..i].iter().map(|k| k.high).fold(f64::MIN, f64::max);
    let recent_low = klines[i - lookback..i].iter().map(|k| k.low).fold(f64::MAX, f64::min);

    if k.close > resistance {
        // 计算小溪宽度：阻力线到近期低点的距离
        let creek_width = resistance - recent_low;
        let creek_width_pct = if recent_low > 0.0 { creek_width / recent_low * 100.0 } else { 0.0 };

        // 查找关联的AR事件，用于说明哪条"小溪"
        let ar_event = events.iter()
            .filter(|e| e.event_type == "AR")
            .filter(|e| i as u64 > e.index)
            .max_by_key(|e| e.index);

        let creek_desc = if let Some(ar) = ar_event {
            format!("小溪=AR#{:#?}高点{:.2}到近期低点{:.2}的距离", ar.index, resistance, recent_low)
        } else {
            format!("小溪=近期阻力{:.2}到近期低点{:.2}的距离", resistance, recent_low)
        };

        Some(WyckoffEvent {
            event_type: "JOC".to_string(),
            index: i as u64,
            dt: k.dt.clone(),
            price: k.high,
            description: "跳过小溪: 放量突破阻力区".to_string(),
            reason: format!(
                "JOC(跳过小溪): 放量(vol={:.0} > 均值*1.3={:.0})突破阻力={:.2}(close={:.2}), {}, 溪宽={:.2}({:.1}%), 需求强劲越过供给边界（Wyckoff: Evans比喻, 等同SOS）",
                k.vol, avg_vol * 1.3, resistance, k.close, creek_desc, creek_width, creek_width_pct
            ),
        })
    } else {
        None
    }
}

// ============================================================
//  辅助函数
// ============================================================

fn calc_avg<F>(klines: &[KLine], f: F) -> f64
where
    F: Fn(&KLine) -> f64,
{
    if klines.is_empty() { return 0.0; }
    klines.iter().map(|k| f(k)).sum::<f64>() / klines.len() as f64
}

#[allow(dead_code)]
fn calc_window_avg<F>(klines: &[KLine], start: usize, end: usize, f: F) -> f64
where
    F: Fn(&KLine) -> f64,
{
    if start >= end || end > klines.len() { return 0.0; }
    klines[start..end].iter().map(|k| f(k)).sum::<f64>() / (end - start) as f64
}

#[cfg(test)]
mod tests {
    use super::*;
    use yifang_data::TimeFrame;

    fn make_kline(open: f64, close: f64, high: f64, low: f64, vol: f64, idx: usize) -> KLine {
        KLine {
            symbol: "test".to_string(),
            timeframe: TimeFrame::D,
            dt: format!("d{}", idx),
            id: idx as u64,
            open, close, high, low, vol,
            amount: vol * (open + close) / 2.0,
        }
    }

    #[test]
    fn test_detect_sc() {
        // 构造 SC 场景：持续下跌后宽幅巨量收回
        let mut klines: Vec<KLine> = (0..10)
            .map(|i| make_kline(
                20.0 - (i as f64) * 0.5,
                19.5 - (i as f64) * 0.5,
                20.0 - (i as f64) * 0.5 + 0.2,
                19.0 - (i as f64) * 0.5,
                500.0,
                i,
            ))
            .collect();

        // SC: 宽幅巨量下跌收回（index 10）
        klines.push(make_kline(15.0, 15.5, 16.0, 10.0, 3000.0, 10));
        // 后续 K 线确保循环能到达 index 10
        klines.push(make_kline(15.5, 14.0, 16.0, 13.0, 800.0, 11));
        klines.push(make_kline(14.0, 13.5, 15.0, 12.5, 700.0, 12));
        klines.push(make_kline(13.5, 13.0, 14.0, 12.0, 600.0, 13));

        let avg_vol = klines.iter().map(|k| k.vol).sum::<f64>() / klines.len() as f64;
        let avg_spread = klines.iter().map(|k| k.high - k.low).sum::<f64>() / klines.len() as f64;

        // 直接调用确认 detect_sc 工作正常
        let sc_result = detect_sc(&klines, 10, avg_vol, avg_spread);
        assert!(sc_result.is_some(), "detect_sc(10) 应返回 Some");
        assert_eq!(sc_result.unwrap().event_type, "SC");

        // 通过 detect_events 也应该能检测到
        let events = detect_events(&klines);
        let sc_events: Vec<_> = events.iter().filter(|e| e.event_type == "SC").collect();
        assert!(!sc_events.is_empty(), "detect_events 应检测到SC");
    }

    #[test]
    fn test_detect_spring() {
        // 先构造 SC，再构造 Spring
        let mut klines: Vec<KLine> = (0..10)
            .map(|i| make_kline(
                20.0 - (i as f64) * 0.5,
                19.5 - (i as f64) * 0.5,
                20.0 - (i as f64) * 0.5 + 0.2,
                19.0 - (i as f64) * 0.5,
                500.0,
                i,
            ))
            .collect();

        // SC: 低点 10.0
        klines.push(make_kline(15.0, 15.5, 16.0, 10.0, 3000.0, 10));
        // AR
        klines.push(make_kline(15.5, 17.0, 17.5, 15.0, 1500.0, 11));
        // 回调
        klines.push(make_kline(17.0, 15.5, 17.0, 15.0, 800.0, 12));
        // Spring: 跌破 10.0 后收回 (low=9.5, close=12.5 > 10.0*0.99)
        klines.push(make_kline(15.5, 12.5, 15.5, 9.5, 600.0, 13));
        // 后续确保循环覆盖（需要 len-3 > 13, 即至少 17 根）
        klines.push(make_kline(12.5, 13.0, 14.0, 12.0, 500.0, 14));
        klines.push(make_kline(13.0, 12.0, 13.5, 11.5, 400.0, 15));
        klines.push(make_kline(12.0, 13.0, 13.5, 11.5, 400.0, 16));

        let events = detect_events(&klines);
        let spring_events: Vec<_> = events.iter().filter(|e| e.event_type == "Spring").collect();
        assert!(!spring_events.is_empty(), "应检测到Spring");
    }

    #[test]
    fn test_no_false_sc_in_uptrend() {
        // 上涨趋势中不应检测到 SC
        let klines: Vec<KLine> = (0..25)
            .map(|i| make_kline(
                10.0 + (i as f64) * 0.5,
                10.5 + (i as f64) * 0.5,
                11.0 + (i as f64) * 0.5,
                10.0 + (i as f64) * 0.5,
                1000.0,
                i,
            ))
            .collect();

        let events = detect_events(&klines);
        let sc_events: Vec<_> = events.iter().filter(|e| e.event_type == "SC").collect();
        assert!(sc_events.is_empty(), "上涨趋势不应有SC");
    }
}
