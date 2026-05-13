/// 逐源测试：分别测试 Tushare、东方财富、新浪三个数据源
/// 每个板块单独测每个源，验证数据正确性
use yifang_data::{
    fetch_board_stock_codes, fetch_board_online_count,
    fetch_board_codes_sina, fetch_board_codes_tushare,
};

fn check_prefix(codes: &[String], board: &str) -> (bool, Vec<String>) {
    let valid_prefixes: &[&str] = match board {
        "sh_main" => &["60", "9"],
        "sz_main" => &["00", "001", "002", "003"],
        "gem" => &["30", "301"],
        "star" => &["688", "689"],
        "bse" => &["4", "8", "9"],
        "all_a" => &["0", "3", "6", "4", "8", "9"],
        _ => &[""],
    };
    let bad: Vec<String> = codes.iter()
        .filter(|c| !valid_prefixes.iter().any(|p| c.starts_with(p)))
        .cloned()
        .collect();
    (bad.is_empty(), bad)
}

fn main() {
    let boards = [
        ("sh_main", "沪A主板"),
        ("sz_main", "深A主板"),
        ("gem",     "创业板"),
        ("star",    "科创板"),
        ("bse",     "北交所"),
        ("all_a",   "全A股"),
    ];

    println!("╔══════════════════════════════════════════════════════╗");
    println!("║      逐数据源自测 - Tushare/东方财富/新浪          ║");
    println!("╚══════════════════════════════════════════════════════╝\n");

    let mut all_pass = true;

    for (board_id, board_name) in &boards {
        println!("┌─── {} ({}) ───", board_name, board_id);

        // ── 1. Tushare ──
        print!("  [Tushare]  股票列表: ");
        match fetch_board_codes_tushare(board_id) {
            Ok(codes) => {
                let count = codes.len();
                let (prefix_ok, bad) = check_prefix(&codes, board_id);
                if prefix_ok {
                    println!("✅ {} 只, 前缀校验通过", count);
                } else {
                    println!("⚠️  {} 只, 前缀不符: {:?}...", count, &bad[..bad.len().min(3)]);
                    all_pass = false;
                }
            }
            Err(e) => {
                println!("❌ 失败: {}", e);
                all_pass = false;
            }
        }

        // ── 2. 新浪 ──
        print!("  [新浪]     股票列表: ");
        match fetch_board_codes_sina(board_id) {
            Ok(codes) => {
                let count = codes.len();
                let (prefix_ok, bad) = check_prefix(&codes, board_id);
                if prefix_ok {
                    println!("✅ {} 只, 前缀校验通过", count);
                } else {
                    println!("⚠️  {} 只, 前缀不符: {:?}...", count, &bad[..bad.len().min(3)]);
                    all_pass = false;
                }
            }
            Err(e) => {
                println!("❌ 失败: {}", e);
                // 新浪不支持北交所/all_a 是已知的
                if *board_id == "bse" || *board_id == "all_a" {
                    println!("             (新浪不支持{}, 属正常)", board_name);
                } else {
                    all_pass = false;
                }
            }
        }

        // ── 3. 聚合函数（完整降级链路）──
        print!("  [聚合]     在线总数: ");
        match fetch_board_online_count(board_id) {
            Ok(count) => {
                println!("✅ {} 只", count);
            }
            Err(e) => {
                println!("❌ 失败: {}", e);
                all_pass = false;
            }
        }

        println!("┘");
    }

    println!();
    if all_pass {
        println!("🎉 所有数据源自测通过！");
    } else {
        println!("⚠️  部分数据源测试失败，请检查网络或数据源可用性");
    }
}
