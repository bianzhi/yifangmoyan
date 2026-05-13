/// 逐源测试：分别测试东方财富、新浪、网易三个数据源
/// 每个板块单独测每个源，验证数据正确性
use yifang_data::{
    fetch_board_stock_codes, fetch_board_online_count,
    fetch_board_codes_sina, fetch_board_codes_netease,
};

fn check_prefix(codes: &[String], board: &str) -> (bool, Vec<String>) {
    let bad: Vec<String> = codes.iter().filter(|c| {
        match board {
            "sh_main" => !c.starts_with("60") || c.starts_with("688"),
            "sz_main" => !c.starts_with("00"),
            "gem"     => !c.starts_with("30"),
            "star"    => !c.starts_with("68"),
            "bse"     => !c.starts_with("8") && !c.starts_with("4"),
            _ => false,
        }
    }).cloned().take(10).collect();
    (bad.is_empty(), bad)
}

fn main() {
    let boards = vec![
        ("sh_main", "沪A主板"),
        ("sz_main", "深A主板"),
        ("gem",     "创业板"),
        ("star",    "科创板"),
        ("bse",     "北交所"),
    ];

    println!("╔══════════════════════════════════════════════════════╗");
    println!("║          逐数据源自测 - 东方财富/新浪/网易          ║");
    println!("╚══════════════════════════════════════════════════════╝\n");

    let mut all_pass = true;

    for (board_id, board_name) in &boards {
        println!("━━━ {} ({}) ━━━", board_name, board_id);

        // ── 1. 东方财富 ──
        print!("  [东方财富] 股票列表: ");
        match fetch_board_stock_codes(board_id) {
            Ok(codes) => {
                let count = codes.len();
                let (prefix_ok, bad) = check_prefix(&codes, board_id);
                println!("{} 只 {}", count,
                    if prefix_ok { "✅ 前缀正确" } else { "❌ 前缀异常" });
                if !prefix_ok {
                    println!("    异常代码: {:?}", bad);
                    all_pass = false;
                }
                if count == 0 {
                    println!("    ❌ 空列表！");
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
                println!("{} 只 {}", count,
                    if prefix_ok { "✅ 前缀正确" } else { "❌ 前缀异常" });
                if !prefix_ok {
                    println!("    异常代码: {:?}", bad);
                    all_pass = false;
                }
                if count == 0 {
                    println!("    ❌ 空列表！");
                    all_pass = false;
                }
            }
            Err(e) => {
                println!("❌ 失败: {}", e);
                all_pass = false;
            }
        }

        // ── 3. 网易 ──
        print!("  [网易]     股票列表: ");
        match fetch_board_codes_netease(board_id) {
            Ok(codes) => {
                let count = codes.len();
                let (prefix_ok, bad) = check_prefix(&codes, board_id);
                println!("{} 只 {}", count,
                    if prefix_ok { "✅ 前缀正确" } else { "❌ 前缀异常" });
                if !prefix_ok {
                    println!("    异常代码: {:?}", bad);
                    all_pass = false;
                }
                if count == 0 {
                    println!("    ❌ 空列表！");
                    all_pass = false;
                }
            }
            Err(e) => {
                println!("❌ 失败: {}", e);
                all_pass = false;
            }
        }

        // ── 4. 在线总数（聚合接口，降级链路） ──
        print!("  [在线总数] 降级链路: ");
        match fetch_board_online_count(board_id) {
            Ok(total) => {
                println!("{} 只 ✅", total);
                if total == 0 {
                    println!("    ❌ 总数为0！");
                    all_pass = false;
                }
            }
            Err(e) => {
                println!("❌ 全部失败: {}", e);
                all_pass = false;
            }
        }

        println!();
        // 请求间延迟，避免限流
        std::thread::sleep(std::time::Duration::from_secs(1));
    }

    // ── all_a 板块 ──
    println!("━━━ 全A股 (all_a) ━━━");
    print!("  [东方财富] 股票列表: ");
    match fetch_board_stock_codes("all_a") {
        Ok(codes) => {
            let count = codes.len();
            println!("{} 只 ✅", count);
            if count < 5000 {
                println!("    ⚠️ 数量偏少（预期5000+），可能遗漏");
                all_pass = false;
            }
        }
        Err(e) => {
            println!("❌ 失败: {}", e);
            all_pass = false;
        }
    }

    print!("  [在线总数] 降级链路: ");
    match fetch_board_online_count("all_a") {
        Ok(total) => {
            println!("{} 只 ✅", total);
        }
        Err(e) => {
            println!("❌ 失败: {}", e);
            all_pass = false;
        }
    }

    println!();
    println!("════════════════════════════════════════════════════════");
    if all_pass {
        println!("🎉 全部通过！");
    } else {
        println!("⚠️ 存在失败项，请检查上方输出");
    }
    println!("════════════════════════════════════════════════════════");
}
