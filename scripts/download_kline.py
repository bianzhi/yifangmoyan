#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
A股K线数据补齐脚本
从新浪财经获取 2023-01-01 至今的 全部A股 日K、周K、月K 数据
保存到 moyan-project/data/kline_cache/ 目录，与现有 parquet 格式兼容

用法:
    python3 scripts/download_kline.py                    # 补齐全部 日/周/月
    python3 scripts/download_kline.py --level 1d         # 只补日线
    python3 scripts/download_kline.py --level 1d,1wk     # 补日线+周线
    python3 scripts/download_kline.py --limit 10          # 只处理前10只（测试用）
    python3 scripts/download_kline.py --force             # 强制覆盖已有数据
"""

import os
import sys
import json
import time
import argparse
import urllib.request
import urllib.error
from datetime import datetime
from pathlib import Path
from concurrent.futures import ThreadPoolExecutor, as_completed

# ─────────────────── 配置 ───────────────────

CACHE_DIR = Path("/Users/csdn/Code/moyan/moyan-project/data/kline_cache")
START_DATE = "2023-01-01"
END_DATE = datetime.now().strftime("%Y-%m-%d")

# 股票代码 → Sina symbol 映射
# 6xxxxx → sh6xxxxx, 0xxxxx → sz0xxxxx, 3xxxxx → sz3xxxxx, 688xxx → sh688xxx
def code_to_sina(code: str) -> str:
    """6位代码转新浪symbol"""
    if code.startswith(('6', '9')):
        return f'sh{code}'
    elif code.startswith(('0', '3')):
        return f'sz{code}'
    elif code.startswith(('4', '8')):
        return f'bj{code}'
    else:
        return f'sz{code}'

# Sina kline scale 参数
LEVEL_MAP = {
    '1d': 240,      # 日线
    '1wk': 1200,    # 周线 (Sina内部5日聚合)
    '1mo': None,    # 月线需要从日线重采样
}

# Sina K线目录名
LEVEL_DIR = {
    '1d': '1d',
    '1wk': '1wk',
    '1mo': '1mo',
}

HEADERS = {
    'User-Agent': 'Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36',
    'Referer': 'https://finance.sina.com.cn/',
}

# ─────────────────── 获取股票列表 ───────────────────

def get_stock_codes(skip_existing: bool = False, levels: list[str] = None) -> list[str]:
    """从现有 parquet 缓存中获取股票代码列表
    
    Args:
        skip_existing: 如果为True，跳过已经从2023年开始有数据的股票
        levels: 检查哪些级别
    """
    daily_dir = CACHE_DIR / "1d"
    if not daily_dir.exists():
        print(f"❌ 缓存目录不存在: {daily_dir}")
        sys.exit(1)
    
    codes = sorted([
        f.replace('.parquet', '') 
        for f in os.listdir(daily_dir) 
        if f.endswith('.parquet')
    ])
    
    if not skip_existing:
        return codes
    
    # 过滤出仍需更新的股票
    import pyarrow.parquet as pq
    need_update = []
    for code in codes:
        fp = daily_dir / f"{code}.parquet"
        try:
            t = pq.read_table(str(fp))
            cols = t.column_names
            dt_col = None
            for name in ['datetime', 'Date', 'date', 'time']:
                if name in cols:
                    dt_col = name
                    break
            if dt_col is None:
                dt_col = [c for c in cols if 'date' in c.lower() or 'time' in c.lower()]
                dt_col = dt_col[0] if dt_col else None
            
            if dt_col:
                first = str(t.column(dt_col)[0])[:10]
                if first > '2023-06-01':
                    need_update.append(code)
            else:
                need_update.append(code)
        except Exception:
            need_update.append(code)
    
    return need_update

# ─────────────────── 数据下载 ───────────────────

def fetch_sina_kline(sina_symbol: str, scale: int, datalen: int = 1500, retries: int = 3) -> list[dict] | None:
    """
    从新浪获取K线数据
    scale: 240=日线, 1200=周线
    datalen: 返回的K线数量（从最近往前数）
    """
    url = f'https://money.finance.sina.com.cn/quotes_service/api/json_v2.php/CN_MarketData.getKLineData?symbol={sina_symbol}&scale={scale}&ma=no&datalen={datalen}'
    req = urllib.request.Request(url, headers=HEADERS)
    
    for attempt in range(retries):
        try:
            r = urllib.request.urlopen(req, timeout=15)
            data = json.loads(r.read().decode())
            if data and len(data) > 0:
                return data
            return None
        except (urllib.error.URLError, json.JSONDecodeError, TimeoutError) as e:
            if attempt < retries - 1:
                time.sleep(0.5 * (attempt + 1))
            else:
                return None
        except Exception as e:
            return None
    return None

def sina_data_to_records(data: list[dict], start_date: str) -> list[dict]:
    """将新浪K线数据转为标准记录格式，并过滤起始日期"""
    records = []
    for item in data:
        day = item.get('day', '')
        if day < start_date:
            continue
        records.append({
            'datetime': day,
            'Open': float(item['open']),
            'High': float(item['high']),
            'Low': float(item['low']),
            'Close': float(item['close']),
            'Volume': int(item['volume']),
        })
    return records

def resample_to_weekly(daily_records: list[dict]) -> list[dict]:
    """将日线数据重采样为周线"""
    if not daily_records:
        return []
    
    # 按周分组
    from datetime import datetime
    weekly = {}
    for r in daily_records:
        dt = datetime.strptime(r['datetime'], '%Y-%m-%d')
        # ISO week: year-weeknumber
        iso = dt.isocalendar()
        week_key = f"{iso[0]}-W{iso[1]:02d}"
        
        if week_key not in weekly:
            weekly[week_key] = {
                'datetime': r['datetime'],  # 周内最后一个交易日的日期
                'Open': r['Open'],
                'High': r['High'],
                'Low': r['Low'],
                'Close': r['Close'],
                'Volume': r['Volume'],
                '_sort': r['datetime'],
            }
        else:
            weekly[week_key]['High'] = max(weekly[week_key]['High'], r['High'])
            weekly[week_key]['Low'] = min(weekly[week_key]['Low'], r['Low'])
            weekly[week_key]['Close'] = r['Close']
            weekly[week_key]['Volume'] += r['Volume']
            weekly[week_key]['datetime'] = r['datetime']
            weekly[week_key]['_sort'] = r['datetime']
    
    # 按日期排序
    result = sorted(weekly.values(), key=lambda x: x['_sort'])
    for r in result:
        del r['_sort']
    return result

def resample_to_monthly(daily_records: list[dict]) -> list[dict]:
    """将日线数据重采样为月线"""
    if not daily_records:
        return []
    
    monthly = {}
    for r in daily_records:
        month_key = r['datetime'][:7]  # YYYY-MM
        
        if month_key not in monthly:
            monthly[month_key] = {
                'datetime': r['datetime'],
                'Open': r['Open'],
                'High': r['High'],
                'Low': r['Low'],
                'Close': r['Close'],
                'Volume': r['Volume'],
            }
        else:
            monthly[month_key]['High'] = max(monthly[month_key]['High'], r['High'])
            monthly[month_key]['Low'] = min(monthly[month_key]['Low'], r['Low'])
            monthly[month_key]['Close'] = r['Close']
            monthly[month_key]['Volume'] += r['Volume']
            monthly[month_key]['datetime'] = r['datetime']
    
    return sorted(monthly.values(), key=lambda x: x['datetime'])

# ─────────────────── Parquet 读写 ───────────────────

def save_parquet(records: list[dict], filepath: Path):
    """保存为parquet文件，与现有格式兼容"""
    import pyarrow as pa
    import pyarrow.parquet as pq
    
    if not records:
        return
    
    # 转换 datetime 字符串为 timestamp
    from datetime import datetime
    datetimes = []
    opens, highs, lows, closes, volumes = [], [], [], [], []
    for r in records:
        dt = datetime.strptime(r['datetime'], '%Y-%m-%d')
        datetimes.append(dt)
        opens.append(r['Open'])
        highs.append(r['High'])
        lows.append(r['Low'])
        closes.append(r['Close'])
        volumes.append(r['Volume'])
    
    table = pa.table({
        'Open': opens,
        'High': highs,
        'Low': lows,
        'Close': closes,
        'Volume': volumes,
        'datetime': datetimes,
    })
    
    filepath.parent.mkdir(parents=True, exist_ok=True)
    pq.write_table(table, filepath)

def load_existing_parquet(filepath: Path) -> list[dict] | None:
    """读取已有的parquet文件"""
    import pyarrow.parquet as pq
    
    if not filepath.exists():
        return None
    
    try:
        table = pq.read_table(filepath)
        records = []
        for i in range(len(table)):
            dt = table.column('datetime')[i].as_py()
            if isinstance(dt, str):
                dt_str = dt[:10]
            else:
                dt_str = dt.strftime('%Y-%m-%d')
            records.append({
                'datetime': dt_str,
                'Open': float(table.column('Open')[i].as_py()),
                'High': float(table.column('High')[i].as_py()),
                'Low': float(table.column('Low')[i].as_py()),
                'Close': float(table.column('Close')[i].as_py()),
                'Volume': int(table.column('Volume')[i].as_py()),
            })
        return records
    except Exception:
        return None

def merge_records(old_records: list[dict], new_records: list[dict]) -> list[dict]:
    """合并新旧数据，去重"""
    if not old_records:
        return new_records
    if not new_records:
        return old_records
    
    seen = set()
    merged = []
    for r in old_records + new_records:
        key = r['datetime']
        if key not in seen:
            seen.add(key)
            merged.append(r)
    
    # 按日期排序，去重保留新数据
    merged.sort(key=lambda x: x['datetime'])
    return merged

# ─────────────────── 单只股票下载 ───────────────────

def download_stock(stock_code: str, levels: list[str], start_date: str, force: bool = False) -> dict:
    """
    下载单只股票的K线数据
    返回: {level: {status, count, msg}}
    """
    sina_symbol = code_to_sina(stock_code)
    result = {}
    
    # 先获取日线数据（周线/月线从中重采样）
    daily_records = None
    
    for level in levels:
        level_dir = CACHE_DIR / LEVEL_DIR[level]
        filepath = level_dir / f"{stock_code}.parquet"
        
        try:
            if level == '1d':
                # 获取日线
                data = fetch_sina_kline(sina_symbol, scale=240, datalen=1500)
                if data is None:
                    # 北交所可能失败，尝试bj前缀
                    if not sina_symbol.startswith('bj'):
                        result[level] = {'status': 'fail', 'count': 0, 'msg': 'no data'}
                    else:
                        result[level] = {'status': 'skip', 'count': 0, 'msg': 'bj not supported'}
                    continue
                
                raw_records = sina_data_to_records(data, start_date)
                daily_records = raw_records  # 缓存给周/月线用
                
                if not force:
                    old = load_existing_parquet(filepath)
                    if old:
                        raw_records = merge_records(old, raw_records)
                
                save_parquet(raw_records, filepath)
                result[level] = {'status': 'ok', 'count': len(raw_records), 'msg': ''}
                
            elif level == '1wk':
                # 周线：优先从Sina获取，失败则从日线重采样
                data = fetch_sina_kline(sina_symbol, scale=1200, datalen=500)
                if data:
                    raw_records = sina_data_to_records(data, start_date)
                else:
                    # 从日线重采样
                    if daily_records is None:
                        # 需要先获取日线
                        d = fetch_sina_kline(sina_symbol, scale=240, datalen=1500)
                        if d:
                            daily_records = sina_data_to_records(d, start_date)
                    
                    if daily_records:
                        raw_records = resample_to_weekly(daily_records)
                    else:
                        result[level] = {'status': 'fail', 'count': 0, 'msg': 'no daily data for resample'}
                        continue
                
                if not force:
                    old = load_existing_parquet(filepath)
                    if old:
                        raw_records = merge_records(old, raw_records)
                
                save_parquet(raw_records, filepath)
                result[level] = {'status': 'ok', 'count': len(raw_records), 'msg': ''}
                
            elif level == '1mo':
                # 月线：从日线重采样
                if daily_records is None:
                    d = fetch_sina_kline(sina_symbol, scale=240, datalen=1500)
                    if d:
                        daily_records = sina_data_to_records(d, start_date)
                
                if daily_records:
                    raw_records = resample_to_monthly(daily_records)
                else:
                    result[level] = {'status': 'fail', 'count': 0, 'msg': 'no daily data for resample'}
                    continue
                
                if not force:
                    old = load_existing_parquet(filepath)
                    if old:
                        raw_records = merge_records(old, raw_records)
                
                save_parquet(raw_records, filepath)
                result[level] = {'status': 'ok', 'count': len(raw_records), 'msg': ''}
        
        except Exception as e:
            result[level] = {'status': 'error', 'count': 0, 'msg': str(e)}
    
    return result

# ─────────────────── 并发下载 ───────────────────

def download_all(stock_codes: list[str], levels: list[str], start_date: str, 
                force: bool = False, workers: int = 3, delay: float = 0.1) -> dict:
    """
    并发下载所有股票K线数据
    workers: 并发数（不要太高，避免被限流）
    delay: 每个请求之间的延迟
    """
    total = len(stock_codes)
    stats = {
        'total': total,
        'ok': 0,
        'fail': 0,
        'skip': 0,
        'error': 0,
        'level_counts': {level: 0 for level in levels},
        'start_time': time.time(),
        'failed_codes': [],
    }
    
    def process_one(code: str) -> tuple[str, dict]:
        result = download_stock(code, levels, start_date, force)
        time.sleep(delay)  # 限流保护
        return code, result
    
    with ThreadPoolExecutor(max_workers=workers) as executor:
        futures = {executor.submit(process_one, code): code for code in stock_codes}
        
        for i, future in enumerate(as_completed(futures), 1):
            code, result = future.result()
            
            # 统计
            all_ok = True
            for level, info in result.items():
                if info['status'] == 'ok':
                    stats['level_counts'][level] += 1
                elif info['status'] == 'skip':
                    stats['skip'] += 1
                    all_ok = False
                elif info['status'] == 'fail':
                    stats['fail'] += 1
                    all_ok = False
                else:
                    stats['error'] += 1
                    all_ok = False
            
            if all_ok:
                stats['ok'] += 1
            else:
                stats['failed_codes'].append(code)
            
            # 进度显示
            elapsed = time.time() - stats['start_time']
            speed = i / elapsed if elapsed > 0 else 0
            eta = (total - i) / speed if speed > 0 else 0
            
            level_info = ' '.join(
                f"{level}:{info['status']}({info['count']})" 
                for level, info in result.items()
            )
            print(f"\r[{i}/{total}] {speed:.1f}/s ETA:{eta/60:.1f}m | {code}: {level_info}   ", end='', flush=True)
    
    stats['elapsed'] = time.time() - stats['start_time']
    return stats

# ─────────────────── main ───────────────────

def main():
    parser = argparse.ArgumentParser(description='A股K线数据补齐')
    parser.add_argument('--level', default='1d,1wk,1mo', help='K线级别，逗号分隔 (默认: 1d,1wk,1mo)')
    parser.add_argument('--start', default=START_DATE, help=f'起始日期 (默认: {START_DATE})')
    parser.add_argument('--end', default=END_DATE, help=f'结束日期 (默认: 今天)')
    parser.add_argument('--limit', type=int, default=0, help='限制处理股票数量（0=全部）')
    parser.add_argument('--force', action='store_true', help='强制覆盖已有数据（不合并）')
    parser.add_argument('--skip-existing', action='store_true', help='跳过已有2023年起数据的股票')
    parser.add_argument('--workers', type=int, default=3, help='并发数 (默认: 3)')
    parser.add_argument('--delay', type=float, default=0.1, help='请求间隔秒数 (默认: 0.1)')
    args = parser.parse_args()
    
    levels = [l.strip() for l in args.level.split(',')]
    for l in levels:
        if l not in LEVEL_MAP:
            print(f"❌ 不支持的K线级别: {l}，可选: {list(LEVEL_MAP.keys())}")
            sys.exit(1)
    
    print("=" * 70)
    print("A股K线数据补齐工具")
    print("=" * 70)
    print(f"数据范围: {args.start} ~ {args.end}")
    print(f"K线级别: {levels}")
    print(f"缓存目录: {CACHE_DIR}")
    print(f"并发数: {args.workers}，请求间隔: {args.delay}s")
    print(f"强制覆盖: {args.force}")
    print()
    
    # 获取股票列表
    stock_codes = get_stock_codes(skip_existing=args.skip_existing, levels=levels)
    print(f"📊 共 {len(stock_codes)} 只股票")
    
    if args.limit > 0:
        stock_codes = stock_codes[:args.limit]
        print(f"⚠️ 限制处理前 {args.limit} 只")
    
    # 确认目录
    for level in levels:
        level_dir = CACHE_DIR / LEVEL_DIR[level]
        level_dir.mkdir(parents=True, exist_ok=True)
        existing = len(list(level_dir.glob("*.parquet")))
        print(f"  {level}: {existing} 个已有文件")
    
    print()
    
    # 执行下载
    stats = download_all(
        stock_codes=stock_codes,
        levels=levels,
        start_date=args.start,
        force=args.force,
        workers=args.workers,
        delay=args.delay,
    )
    
    # 打印统计
    print()
    print("=" * 70)
    print("下载完成!")
    print(f"  总耗时: {stats['elapsed']:.1f}s ({stats['elapsed']/60:.1f}min)")
    print(f"  成功: {stats['ok']}, 失败: {stats['fail']}, 跳过: {stats['skip']}, 错误: {stats['error']}")
    for level, count in stats['level_counts'].items():
        print(f"  {level}: {count} 只股票更新")
    
    if stats['failed_codes']:
        print(f"\n⚠️ 失败的股票代码 ({len(stats['failed_codes'])} 个):")
        print(', '.join(stats['failed_codes'][:50]))
        if len(stats['failed_codes']) > 50:
            print(f"... 共 {len(stats['failed_codes'])} 个")

if __name__ == '__main__':
    main()
