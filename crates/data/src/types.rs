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

/// 威科夫分析结果
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct WyckoffResult {
    /// 趋势线
    pub trend_lines: Vec<TrendLine>,
    /// 威科夫事件标注
    pub events: Vec<WyckoffEvent>,
    /// 交易区间
    pub trading_ranges: Vec<TradingRange>,
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

/// 威科夫事件
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WyckoffEvent {
    /// 事件类型:
    /// - "TR" (交易区间)
    /// - "SC" (卖出高潮)
    /// - "AR" (自动反弹)
    /// - "ST" (二次测试)
    /// - "Spring" (弹簧效应)
    /// - "UTAD" (向上冲浪后的下跌)
    /// - "JOC" (跳过小溪)
    /// - "LPS" (最后支撑点)
    /// - "ICE" (冰线)
    /// - "SOS" (强势出现)
    /// - "SOW" (弱势出现)
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

/// 交易区间
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
