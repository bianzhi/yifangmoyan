/// 临时测试脚本：验证各板块股票列表获取是否正确
use yifang_data::{fetch_board_stock_codes, fetch_board_online_count};

fn main() {
    let boards = vec![
        ("sh_main", "沪A主板"),
        ("sz_main", "深A主板"),
        ("gem", "创业板"),
        ("star", "科创板"),
        ("bse", "北交所"),
    ];

    println!("========== 板块股票列表获取测试 ==========\n");

    for (board_id, board_name) in &boards {
        println!("--- {} ({}) ---", board_name, board_id);

        // 1. 测试获取股票列表
        match fetch_board_stock_codes(board_id) {
            Ok(codes) => {
                let count = codes.len();
                println!("  股票列表: {} 只", count);
                if count > 0 {
                    // 显示前5个和后5个
                    let show_n = 5.min(count);
                    print!("  前{}个: ", show_n);
                    for c in codes.iter().take(show_n) {
                        print!("{} ", c);
                    }
                    println!();
                    if count > 10 {
                        print!("  后{}个: ", show_n);
                        for c in codes.iter().rev().take(show_n).rev() {
                            print!("{} ", c);
                        }
                        println!();
                    }

                    // 验证代码前缀是否正确
                    let prefix_check = match *board_id {
                        "sh_main" => codes.iter().all(|c| c.starts_with("60") && !c.starts_with("688")),
                        "sz_main" => codes.iter().all(|c| c.starts_with("00")),
                        "gem" => codes.iter().all(|c| c.starts_with("30")),
                        "star" => codes.iter().all(|c| c.starts_with("68")),
                        "bse" => codes.iter().all(|c| c.starts_with("8") || c.starts_with("4")),
                        _ => true,
                    };
                    if prefix_check {
                        println!("  ✅ 代码前缀校验通过");
                    } else {
                        println!("  ❌ 代码前缀校验失败！存在不属于此板块的代码");
                        let bad: Vec<_> = codes.iter().filter(|c| {
                            match *board_id {
                                "sh_main" => !c.starts_with("60") || c.starts_with("688"),
                                "sz_main" => !c.starts_with("00"),
                                "gem" => !c.starts_with("30"),
                                "star" => !c.starts_with("68"),
                                "bse" => !c.starts_with("8") && !c.starts_with("4"),
                                _ => false,
                            }
                        }).take(10).collect();
                        println!("  异常代码示例: {:?}", bad);
                    }
                } else {
                    println!("  ❌ 获取到空列表！");
                }
            }
            Err(e) => {
                println!("  ❌ 获取失败: {}", e);
            }
        }

        // 2. 测试获取在线总数
        match fetch_board_online_count(board_id) {
            Ok(total) => {
                println!("  在线总数: {}", total);
            }
            Err(e) => {
                println!("  在线总数获取失败: {}", e);
            }
        }

        println!();
    }

    // 3. 测试 all_a
    println!("--- 全A股 (all_a) ---");
    match fetch_board_stock_codes("all_a") {
        Ok(codes) => {
            let count = codes.len();
            println!("  股票列表: {} 只", count);
            // 统计各前缀分布
            let mut prefix_dist: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
            for c in &codes {
                let prefix = if c.starts_with("60") || c.starts_with("68") {
                    "沪市(60/68)".to_string()
                } else if c.starts_with("00") {
                    "深主板(00)".to_string()
                } else if c.starts_with("30") {
                    "创业板(30)".to_string()
                } else if c.starts_with("8") || c.starts_with("4") {
                    "北交所(8/4)".to_string()
                } else {
                    format!("其他({})", &c[..2])
                };
                *prefix_dist.entry(prefix).or_insert(0) += 1;
            }
            println!("  前缀分布: {:?}", prefix_dist);

            // 去重检查
            let unique: std::collections::HashSet<_> = codes.iter().collect();
            if unique.len() != codes.len() {
                println!("  ⚠️ 存在重复！唯一: {}, 总数: {}", unique.len(), codes.len());
            }
        }
        Err(e) => {
            println!("  ❌ 获取失败: {}", e);
        }
    }

    println!("\n========== 测试完成 ==========");
}
