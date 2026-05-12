//! Integration test: verify parquet data loading from moyan-project

use yifang_data::{DataSource, KLineManager, TimeFrame};

#[test]
fn test_load_daily_klines() {
    let manager = KLineManager::new(None);
    
    assert!(manager.is_available(), "Data directory should exist");
    
    let klines = manager.get_klines("000001", TimeFrame::D, None, None)
        .expect("Should load 000001 daily klines");
    
    println!("Loaded {} daily klines for 000001", klines.len());
    assert!(!klines.is_empty(), "Should have some daily klines");
    
    // Print first and last dt to see the format
    println!("First dt: '{}'", klines[0].dt);
    println!("Last dt: '{}'", klines.last().unwrap().dt);
    
    // Verify first kline structure
    let first = &klines[0];
    assert_eq!(first.symbol, "000001");
    assert_eq!(first.timeframe, TimeFrame::D);
    assert!(!first.dt.is_empty(), "datetime should not be empty");
    assert!(first.open > 0.0, "open price should be positive");
    assert!(first.high >= first.low, "high >= low");
}

#[test]
fn test_load_15m_klines() {
    let manager = KLineManager::new(None);
    
    let klines = manager.get_klines("000001", TimeFrame::F15, None, None)
        .expect("Should load 000001 15m klines");
    
    println!("Loaded {} 15m klines for 000001", klines.len());
    assert!(!klines.is_empty(), "Should have some 15m klines");
}

#[test]
fn test_search_stocks() {
    let manager = KLineManager::new(None);
    
    let results = manager.search_stocks("000001").expect("Should search stocks");
    println!("Search '000001': found {} results", results.len());
    assert!(!results.is_empty());
    assert_eq!(results[0].symbol, "000001");
    
    // Search by prefix
    let results2 = manager.search_stocks("600").expect("Should search stocks");
    println!("Search '600': found {} results", results2.len());
    assert!(results2.len() > 1, "Should find multiple 600xxx stocks");
}

#[test]
fn test_date_filtering() {
    let manager = KLineManager::new(None);
    
    let all = manager.get_klines("000001", TimeFrame::D, None, None).unwrap();
    let filtered = manager.get_klines("000001", TimeFrame::D, Some("2024-01-01"), Some("2024-12-31")).unwrap();
    
    println!("All: {}, Filtered 2024: {}", all.len(), filtered.len());
    assert!(filtered.len() <= all.len(), "Filtered should be <= all");
    
    if !filtered.is_empty() {
        assert!(filtered[0].dt.as_str() >= "2024-01-01", "First should be >= start");
        assert!(filtered.last().unwrap().dt.as_str() <= "2024-12-31", "Last should be <= end");
    }
}
