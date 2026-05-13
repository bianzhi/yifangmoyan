//! 核心数据类型定义

use serde::{Deserialize, Serialize};

/// K 线时间周期
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TimeFrame {
    /// 月线
    M,
    /// 周线
    W,
    /// 日线
    D,
    /// 60 分钟
    F60,
    /// 30 分钟
    F30,
    /// 15 分钟
    F15,
    /// 5 分钟
    F5,
    /// 1 分钟
    F1,
}

impl TimeFrame {
    /// 获取中文名称
    pub fn label(&self) -> &str {
        match self {
            TimeFrame::M => "月线",
            TimeFrame::W => "周线",
            TimeFrame::D => "日线",
            TimeFrame::F60 => "60F",
            TimeFrame::F30 => "30F",
            TimeFrame::F15 => "15F",
            TimeFrame::F5 => "5F",
            TimeFrame::F1 => "1F",
        }
    }

    /// 获取所有周期，从大到小
    pub fn all() -> &'static [TimeFrame] {
        &[
            TimeFrame::M,
            TimeFrame::W,
            TimeFrame::D,
            TimeFrame::F60,
            TimeFrame::F30,
            TimeFrame::F15,
            TimeFrame::F5,
            TimeFrame::F1,
        ]
    }

    /// 获取上级周期
    pub fn higher_level(&self) -> Option<TimeFrame> {
        match self {
            TimeFrame::M => None,
            TimeFrame::W => Some(TimeFrame::M),
            TimeFrame::D => Some(TimeFrame::W),
            TimeFrame::F60 => Some(TimeFrame::D),
            TimeFrame::F30 => Some(TimeFrame::F60),
            TimeFrame::F15 => Some(TimeFrame::F30),
            TimeFrame::F5 => Some(TimeFrame::F15),
            TimeFrame::F1 => Some(TimeFrame::F5),
        }
    }

    /// 获取次级别周期
    pub fn sub_level(&self) -> Option<TimeFrame> {
        match self {
            TimeFrame::M => Some(TimeFrame::W),
            TimeFrame::W => Some(TimeFrame::D),
            TimeFrame::D => Some(TimeFrame::F60),
            TimeFrame::F60 => Some(TimeFrame::F30),
            TimeFrame::F30 => Some(TimeFrame::F15),
            TimeFrame::F15 => Some(TimeFrame::F5),
            TimeFrame::F5 => Some(TimeFrame::F1),
            TimeFrame::F1 => None,
        }
    }

    /// K 线合成时需要的分钟数 (日线=480 交易分钟, 周线=2400, 月线≈9600)
    pub fn minutes(&self) -> Option<u32> {
        match self {
            TimeFrame::F1 => Some(1),
            TimeFrame::F5 => Some(5),
            TimeFrame::F15 => Some(15),
            TimeFrame::F30 => Some(30),
            TimeFrame::F60 => Some(60),
            TimeFrame::D => Some(240),  // A 股一天 4 小时 = 240 分钟
            TimeFrame::W => Some(1200), // 一周 5 天
            TimeFrame::M => Some(4800), // 一月约 20 天
        }
    }
}

impl std::fmt::Display for TimeFrame {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.label())
    }
}

/// 单根 K 线数据
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KLine {
    /// 股票代码 (如 "000001")
    pub symbol: String,
    /// 时间周期
    pub timeframe: TimeFrame,
    /// 时间戳
    pub dt: String,
    /// 序号，从 0 开始
    pub id: u64,
    /// 开盘价
    pub open: f64,
    /// 收盘价
    pub close: f64,
    /// 最高价
    pub high: f64,
    /// 最低价
    pub low: f64,
    /// 成交量
    pub vol: f64,
    /// 成交额
    pub amount: f64,
}

impl KLine {
    /// 是否阳线
    pub fn is_up(&self) -> bool {
        self.close >= self.open
    }

    /// 实体长度
    pub fn body(&self) -> f64 {
        (self.close - self.open).abs()
    }

    /// 上影线长度
    pub fn upper_shadow(&self) -> f64 {
        self.high - self.open.max(self.close)
    }

    /// 下影线长度
    pub fn lower_shadow(&self) -> f64 {
        self.open.min(self.close) - self.low
    }

    /// 涨跌幅
    pub fn change_pct(&self) -> f64 {
        if self.open == 0.0 {
            0.0
        } else {
            (self.close - self.open) / self.open * 100.0
        }
    }
}

/// 股票基本信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StockInfo {
    /// 股票代码
    pub symbol: String,
    /// 股票名称
    pub name: String,
    /// 拼音简写
    pub pinyin: String,
    /// 市场 (SH/SZ)
    pub market: String,
}

/// 图表数据 — 后端返回给前端的聚合结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChartData {
    /// 股票代码
    pub symbol: String,
    /// 股票名称
    pub name: String,
    /// 时间周期
    pub timeframe: TimeFrame,
    /// K 线数据
    pub klines: Vec<KLine>,
    /// MACD 指标数据
    pub macd: MacdData,
    /// 缠论分析结果
    pub czsc: Option<CzscResult>,
    /// 威科夫分析结果
    pub wyckoff: Option<WyckoffResult>,
}

/// MACD 指标数据
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MacdData {
    pub dif: Vec<f64>,
    pub dea: Vec<f64>,
    pub macd_hist: Vec<f64>,
}

/// 缠论分析结果
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CzscResult {
    /// 分型列表
    pub fenxing: Vec<FenXing>,
    /// 笔列表
    pub bi: Vec<Bi>,
    /// 线段列表
    pub xd: Vec<XianDuan>,
    /// 笔中枢列表
    pub bi_zs: Vec<ZhongShu>,
    /// 线段中枢列表
    pub xd_zs: Vec<ZhongShu>,
    /// 买卖点列表
    pub buy_sell: Vec<BuySellPoint>,
    /// 背驰标记
    pub beichi: Vec<BeiChi>,
    /// 走势列表（走势递归分解）
    pub zoushi: Vec<ZouShi>,
    /// 区间套信号
    pub qujian_tao: Vec<QuJianTaoSignal>,
}

/// 分型
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FenXing {
    /// 分型类型: "top" / "bottom"
    pub fx_type: String,
    /// 所在 K 线索引
    pub index: u64,
    /// 时间
    pub dt: String,
    /// 分型价格
    pub price: f64,
}

/// 笔
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Bi {
    /// 方向: "up" / "down"
    pub direction: String,
    /// 起点 K 线索引
    pub start_index: u64,
    /// 终点 K 线索引
    pub end_index: u64,
    /// 起点时间
    pub start_dt: String,
    /// 终点时间
    pub end_dt: String,
    /// 起点价格
    pub start_price: f64,
    /// 终点价格
    pub end_price: f64,
    /// 是否已完成
    pub is_finished: bool,
}

/// 线段
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct XianDuan {
    /// 方向: "up" / "down"
    pub direction: String,
    /// 起点 K 线索引
    pub start_index: u64,
    /// 终点 K 线索引
    pub end_index: u64,
    /// 起点时间
    pub start_dt: String,
    /// 终点时间
    pub end_dt: String,
    /// 起点价格
    pub start_price: f64,
    /// 终点价格
    pub end_price: f64,
    /// 是否已完成
    pub is_finished: bool,
}

/// 中枢
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ZhongShu {
    /// 中枢类型: "bi_zs" / "xd_zs"
    pub zs_type: String,
    /// 起始 K 线索引
    pub start_index: u64,
    /// 结束 K 线索引
    pub end_index: u64,
    /// 起始时间
    pub start_dt: String,
    /// 结束时间
    pub end_dt: String,
    /// 中枢上沿 (zg)
    pub zg: f64,
    /// 中枢下沿 (zd)
    pub zd: f64,
    /// 中枢最高点 (gg)
    pub gg: f64,
    /// 中枢最低点 (dd)
    pub dd: f64,
}

/// 买卖点
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuySellPoint {
    /// 类型: "1buy", "2buy", "3buy", "1sell", "2sell", "3sell"
    pub bs_type: String,
    /// K 线索引
    pub index: u64,
    /// 时间
    pub dt: String,
    /// 价格
    pub price: f64,
}

/// 背驰标记
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BeiChi {
    /// 背驰类型: "bi_beichi" / "xd_beichi" / "zoushi_beichi"
    pub bc_type: String,
    /// K 线索引
    pub index: u64,
    /// 时间
    pub dt: String,
    /// 方向: "up" / "down"
    pub direction: String,
    /// 背驰子类型: "trend" (趋势背驰) / "panzheng" (盘整背驰)
    pub bc_sub_type: String,
}

impl Default for BeiChi {
    fn default() -> Self {
        Self {
            bc_type: String::new(),
            index: 0,
            dt: String::new(),
            direction: String::new(),
            bc_sub_type: "trend".to_string(),
        }
    }
}

/// 走势类型
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ZouShi {
    /// 走势方向: "up" / "down"
    pub direction: String,
    /// 走势类型: "trend" (趋势) / "panzheng" (盘整)
    pub zs_type: String,
    /// 起点 K 线索引
    pub start_index: u64,
    /// 终点 K 线索引
    pub end_index: u64,
    /// 起点时间
    pub start_dt: String,
    /// 终点时间
    pub end_dt: String,
    /// 起点价格
    pub start_price: f64,
    /// 终点价格
    pub end_price: f64,
    /// 包含的中枢列表
    pub zs_list: Vec<ZhongShu>,
    /// 是否已完成
    pub is_finished: bool,
}

/// 区间套信号
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuJianTaoSignal {
    /// 信号类型: "1buy" / "1sell" / "2buy" / "2sell" / "3buy" / "3sell"
    pub signal_type: String,
    /// 大级别
    pub high_level: String,
    /// 小级别
    pub low_level: String,
    /// 大级别走势方向
    pub high_direction: String,
    /// 小级别确认方向
    pub low_direction: String,
    /// K 线索引
    pub index: u64,
    /// 时间
    pub dt: String,
    /// 价格
    pub price: f64,
    /// 区间套强度: "strong" / "medium" / "weak"
    pub strength: String,
}

/// 威科夫事件
///
/// 威科夫理论中的关键市场事件标注，严格对齐原著定义：
///
/// **吸筹阶段事件：**
/// - PS (Preliminary Support): 初步支撑 — 下跌趋势中的第一次显著买盘
/// - SC (Selling Climax): 卖出高潮 — 恐慌性抛售的顶峰，宽幅+巨量
/// - AR (Automatic Rally): 自动反弹 — SC 后的技术性反弹，确立区间上沿
/// - ST (Secondary Test): 二次测试 — 回测 SC 低点，确认供给耗尽（量缩价窄）
/// - Spring: 弹簧效应 — 价格跌破 SC 低点后迅速收回，空头陷阱
/// - Shakeout: 震荡洗盘 — 类似 Spring 但幅度更大
/// - SOS (Sign of Strength): 强势出现 — 放量上涨突破交易区间上沿
/// - LPS (Last Point of Support): 最后支撑点 — SOS 后回踩支撑确认
/// - JOC (Jump Over Creek): 跳过小溪 — 价格越过阻力区（"小溪"= AR 形成的供给区）
///
/// **派发阶段事件：**
/// - PSY (Preliminary Supply): 初步供给 — 上涨趋势中的第一次显著卖盘
/// - BC (Buying Climax): 买入高潮 — 贪婪性买入的顶峰，宽幅+巨量
/// - AR (Automatic Reaction): 自动回落 — BC 后的技术性回落，确立区间下沿
/// - ST (Secondary Test): 二次测试 — 回测 BC 高点，确认需求耗尽（量缩价窄）
/// - UTAD (Upthrust After Distribution): 派发后冲高 — 突破 BC 高点后迅速回落，多头陷阱
/// - SOW (Sign of Weakness): 弱势出现 — 放量下跌跌破交易区间下沿
/// - LPSY (Last Point of Supply): 最后供给点 — SOW 后反弹确认
/// - ICE (Ice Line Break): 冰线突破 — 价格跌破关键支撑线
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WyckoffEvent {
    /// 事件类型
    pub event_type: String,
    /// K 线索引
    pub index: u64,
    /// 时间
    pub dt: String,
    /// 价格
    pub price: f64,
    /// 描述
    pub description: String,
}

/// 交易区间 (Trading Range)
///
/// 威科夫理论中吸筹/派发的横盘区间。
/// - 吸筹区间：由 SC 的低点和 AR 的高点界定
/// - 派发区间：由 BC 的高点和 AR 的低点界定
///
/// 关键价格水平：
/// - upper / lower: 区间上下沿
/// - ice_line: 冰线 — 吸筹区间中 AR 低点连线形成的供给线，
///   跌破冰线意味着供给压倒需求
/// - midpoint: 区间中点 — 判断价格在区间内的相对位置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradingRange {
    /// 起始 K 线索引
    pub start_index: u64,
    /// 结束 K 线索引
    pub end_index: u64,
    /// 上沿
    pub upper: f64,
    /// 下沿
    pub lower: f64,
    /// 冰线价格
    pub ice_line: f64,
}

/// 趋势线
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrendLine {
    /// 线类型: "support" / "resistance" / "channel"
    pub line_type: String,
    /// 起始 K 线索引
    pub start_index: u64,
    /// 结束 K 线索引
    pub end_index: u64,
    /// 起始价格
    pub start_price: f64,
    /// 结束价格
    pub end_price: f64,
}

/// 威科夫市场阶段（粗粒度）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum WyckoffPhase {
    /// 吸筹 (Accumulation)
    Accumulation,
    /// 拉升 (Markup)
    Markup,
    /// 派发 (Distribution)
    Distribution,
    /// 下跌 (Markdown)
    Markdown,
}

impl Default for WyckoffPhase {
    fn default() -> Self {
        WyckoffPhase::Accumulation
    }
}

impl std::fmt::Display for WyckoffPhase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WyckoffPhase::Accumulation => write!(f, "吸筹"),
            WyckoffPhase::Markup => write!(f, "拉升"),
            WyckoffPhase::Distribution => write!(f, "派发"),
            WyckoffPhase::Markdown => write!(f, "下跌"),
        }
    }
}

/// 威科夫吸筹子阶段
///
/// 对应 Wyckoff 原著的吸筹示意图：
/// Phase A: 止跌 (SC, AR, ST)
/// Phase B: 横盘蓄力 (多次测试 + Spring)
/// Phase C: 主力测试 (Spring/ Shakeout)
/// Phase D: 启动 (SOS, LPS, JOC)
/// Phase E: 离开交易区间
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AccumulationSubPhase {
    /// A 阶段: 止跌
    PhaseA,
    /// B 阶段: 横盘蓄力
    PhaseB,
    /// C 阶段: 主力测试
    PhaseC,
    /// D 阶段: 启动
    PhaseD,
    /// E 阶段: 离开
    PhaseE,
}

/// 威科夫派发子阶段
///
/// Phase A: 停止上涨 (PSY, BC, AR, ST)
/// Phase B: 横盘派发 (多次测试 + UTAD)
/// Phase C: 主力出货 (UTAD)
/// Phase D: 破位 (SOW, LPSY, ICE)
/// Phase E: 离开交易区间下跌
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DistributionSubPhase {
    /// A 阶段: 停涨
    PhaseA,
    /// B 阶段: 横盘派发
    PhaseB,
    /// C 阶段: 主力出货
    PhaseC,
    /// D 阶段: 破位
    PhaseD,
    /// E 阶段: 离开
    PhaseE,
}

/// 阶段标注——标记 K 线序列中每个位置的威科夫阶段
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PhaseLabel {
    /// K 线索引
    pub index: u64,
    /// 时间
    pub dt: String,
    /// 威科夫主要阶段
    pub phase: WyckoffPhase,
    /// 子阶段（吸筹/派发内细分）
    pub sub_phase: String,
}

/// 努力与结果（Effort vs Result）分析
///
/// 威科夫核心法则：努力与结果的关系
/// - 上涨时：大量努力（高成交量）+ 小结果（小涨幅）→ 供给出现，看跌
/// - 上涨时：小努力（低成交量）+ 大结果（大涨幅）→ 需求主导，看涨
/// - 下跌时：大量努力（高成交量）+ 小结果（小跌幅）→ 需求出现，看涨
/// - 下跌时：小努力（低成交量）+ 大结果（大跌幅）→ 供给主导，看跌
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EffortResult {
    /// K 线索引
    pub index: u64,
    /// 时间
    pub dt: String,
    /// 努力（成交量相对均值的倍数）
    pub effort: f64,
    /// 结果（价格变动幅度占价格的比例）
    pub result: f64,
    /// 量价协调性: "harmonious" / "divergent"
    pub harmony: String,
    /// 解读: "demand_dominant" / "supply_dominant" / "neutral"
    pub interpretation: String,
}

/// 威科夫供需线
///
/// 供给线 (Supply Line): 连接反弹高点，等同传统下降趋势线
/// 需求线 (Demand Line): 连接回调低点，等同传统上升趋势线
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SupplyDemandLine {
    /// 线类型: "supply" / "demand"
    pub line_type: String,
    /// 起始 K 线索引
    pub start_index: u64,
    /// 结束 K 线索引
    pub end_index: u64,
    /// 起始价格
    pub start_price: f64,
    /// 结束价格
    pub end_price: f64,
    /// 斜率
    pub slope: f64,
}

/// 威科夫分析结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WyckoffResult {
    /// 阶段标注序列
    pub phase_labels: Vec<PhaseLabel>,
    /// 威科夫事件标注
    pub events: Vec<WyckoffEvent>,
    /// 交易区间
    pub trading_ranges: Vec<TradingRange>,
    /// 趋势线（兼容旧字段）
    pub trend_lines: Vec<TrendLine>,
    /// 供需线
    pub supply_demand_lines: Vec<SupplyDemandLine>,
    /// 努力与结果分析
    pub effort_results: Vec<EffortResult>,
}

impl Default for WyckoffResult {
    fn default() -> Self {
        WyckoffResult {
            phase_labels: Vec::new(),
            events: Vec::new(),
            trading_ranges: Vec::new(),
            trend_lines: Vec::new(),
            supply_demand_lines: Vec::new(),
            effort_results: Vec::new(),
        }
    }
}
