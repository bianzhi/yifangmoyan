//! End-to-end integration test: parquet → klines → czsc analysis → chart data → JSON

use yifang_data::{ChartData, DataSource, KLineManager, TimeFrame};
use yifang_czsc::CzscAnalyzer;
use yifang_indicator::calc_macd;

#[test]
fn test_e2e_daily_czsc_analysis() {
    let manager = KLineManager::new(None);
    let klines = manager.get_klines("000001", TimeFrame::D, None, None).unwrap();
    assert!(!klines.is_empty(), "Should have daily klines");
    
    // Calc MACD
    let macd = calc_macd(&klines, 12, 26, 9);
    assert!(!macd.dif.is_empty(), "MACD DIF should not be empty");
    
    // Run CZSC analysis
    let czsc_result = CzscAnalyzer::analyze(&klines, &macd);
    
    println!("=== CZSC Analysis for 000001 (Daily) ===");
    println!("FenXing: {}, Bi: {}, XianDuan: {}", 
        czsc_result.fenxing.len(), czsc_result.bi.len(), czsc_result.xd.len());
    println!("Bi ZhongShu: {}, Xd ZhongShu: {}, BuySell: {}, BeiChi: {}",
        czsc_result.bi_zs.len(), czsc_result.xd_zs.len(), 
        czsc_result.buy_sell.len(), czsc_result.beichi.len());
    
    assert!(!czsc_result.fenxing.is_empty(), "Should have fenxing");
    assert!(!czsc_result.bi.is_empty(), "Should have bi");
}

#[test]
fn test_e2e_chart_data_serialization() {
    let manager = KLineManager::new(None);
    let klines = manager.get_klines("000001", TimeFrame::D, None, None).unwrap();
    
    let macd = calc_macd(&klines, 12, 26, 9);
    let czsc_result = CzscAnalyzer::analyze(&klines, &macd);
    
    let chart_data = ChartData {
        symbol: "000001".to_string(),
        name: "平安银行".to_string(),
        timeframe: TimeFrame::D,
        klines,
        macd,
        czsc: Some(czsc_result),
        wyckoff: None,
    };
    
    // Test serde JSON serialization (Tauri IPC uses serde_json)
    let json = serde_json::to_string(&chart_data).expect("Should serialize to JSON");
    println!("ChartData JSON: {} bytes", json.len());
    assert!(!json.is_empty());
    
    // Verify round-trip
    let de: ChartData = serde_json::from_str(&json).expect("Should deserialize");
    assert_eq!(de.symbol, "000001");
    assert_eq!(de.timeframe, TimeFrame::D);
    assert!(de.czsc.is_some());
}
