//! Benchmarks for RustafariDB: OLTP (sysbench-style) and OLAP (aggregation over columnar).
//!
//! - **OLTP**: Point/range SELECTs; compare with MySQL/PostgreSQL via sysbench (docs/benchmark.md).
//! - **OLAP**: SUM/COUNT/AVG over large tables; compare with SingleStore, StarRocks, Snowflake, Databricks (docs/benchmark-olap.md).

use clap::{Parser, Subcommand};
use rand::Rng;
use rustafari_executor::{SessionState, SqlExecutor};
use std::sync::Arc;
use std::time::{Duration, Instant};

#[derive(Subcommand)]
enum Command {
    /// Sysbench-style OLTP read-only (point + range SELECTs)
    Oltp(OltpArgs),
    /// OLAP: SUM/COUNT/AVG over a large columnar-backed table
    Olap(OlapArgs),
    /// TPC-H: lineitem + Q1 (pricing summary report, GROUP BY + ORDER BY)
    Tpch(TpchArgs),
}

/// Sysbench-style OLTP read-only benchmark
#[derive(Parser)]
struct OltpArgs {
    /// Number of rows in the sbtest table (like sysbench --table-size)
    #[arg(long, default_value = "10000")]
    table_size: usize,

    #[arg(long, default_value = "10")]
    point_selects: usize,

    #[arg(long, default_value = "1")]
    simple_ranges: usize,

    #[arg(long, default_value = "100")]
    range_size: usize,

    #[arg(long, default_value = "10")]
    time: u64,

    #[arg(long, default_value = "0")]
    total_events: u64,

    #[arg(long, default_value = "true")]
    report_latency: bool,
}

/// OLAP benchmark: analytical queries (SUM, COUNT, AVG) over many rows
#[derive(Parser)]
struct OlapArgs {
    /// Number of rows to load into the analytics table
    #[arg(long, default_value = "1000000")]
    rows: usize,

    /// Number of aggregate queries to run for latency stats
    #[arg(long, default_value = "100")]
    queries: usize,

    /// Report latency percentiles
    #[arg(long, default_value = "true")]
    report_latency: bool,
}

/// TPC-H benchmark: lineitem table + Q1 (pricing summary)
#[derive(Parser)]
struct TpchArgs {
    /// Scale factor (lineitem rows ≈ 6M * SF; use 0.01 for ~60k rows)
    #[arg(long, default_value = "0.01")]
    scale: f64,

    /// Number of Q1 runs for latency
    #[arg(long, default_value = "20")]
    runs: usize,
}

#[derive(Parser)]
#[command(name = "rustafari-bench")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Oltp(args) => run_oltp(args),
        Command::Olap(args) => run_olap(args),
        Command::Tpch(args) => run_tpch(args),
    }
}

fn run_oltp(args: OltpArgs) -> anyhow::Result<()> {
    println!("RustafariDB sysbench-style OLTP read-only benchmark");
    println!("  table_size: {}  point_selects: {}  simple_ranges: {}  range_size: {}",
        args.table_size, args.point_selects, args.simple_ranges, args.range_size);
    println!();

    let state = Arc::new(SessionState::new());

    SqlExecutor::execute(state.as_ref(), 
        "CREATE TABLE sbtest (id BIGINT, k BIGINT, c TEXT, pad TEXT)")?;

    println!("Loading {} rows...", args.table_size);
    let load_start = Instant::now();
    const BATCH: usize = 500;
    for start in (0..args.table_size).step_by(BATCH) {
        let end = (start + BATCH).min(args.table_size);
        let mut values: Vec<String> = Vec::with_capacity(BATCH * 4);
        for i in start..end {
            let k = (i % args.table_size).max(1) as i64;
            let c = format!("{:0>119}-", i);
            let pad = format!("{:0>59}-", i);
            values.push(format!("({}, {}, '{}', '{}')", i + 1, k, c.replace('\'', "''"), pad.replace('\'', "''")));
        }
        let sql = format!("INSERT INTO sbtest VALUES {}", values.join(", "));
        SqlExecutor::execute(state.as_ref(), &sql)?;
    }
    let load_elapsed = load_start.elapsed();
    println!("  Loaded in {:.2?} ({:.0} rows/s)\n", load_elapsed, args.table_size as f64 / load_elapsed.as_secs_f64());

    let mut rng = rand::thread_rng();
    let table_size = args.table_size as i64;
    let run_duration = if args.time > 0 { Duration::from_secs(args.time) } else { Duration::from_secs(u64::MAX) };
    let mut events: u64 = 0;
    let mut queries: u64 = 0;
    let mut latencies_us: Vec<u64> = Vec::new();
    if args.report_latency {
        latencies_us.reserve(100_000.min((args.time as usize) * 20_000));
    }
    let start = Instant::now();
    let deadline = start + run_duration;
    let event_limit = if args.time == 0 { args.total_events } else { u64::MAX };

    while start.elapsed() < run_duration && events < event_limit {
        let t_start = Instant::now();
        for _ in 0..args.point_selects {
            let id = rng.gen_range(1..=table_size.max(1));
            let _ = SqlExecutor::execute(state.as_ref(), &format!("SELECT c FROM sbtest WHERE id = {} LIMIT 1", id))?;
            queries += 1;
        }
        for _ in 0..args.simple_ranges {
            let id = rng.gen_range(1..=(table_size - args.range_size as i64).max(1));
            let id_end = id + args.range_size as i64;
            let _ = SqlExecutor::execute(state.as_ref(), &format!(
                "SELECT c FROM sbtest WHERE id >= {} AND id <= {} LIMIT {}", id, id_end, args.range_size))?;
            queries += 1;
        }
        if args.report_latency && latencies_us.len() < 500_000 {
            latencies_us.push(t_start.elapsed().as_micros() as u64);
        }
        events += 1;
        if args.time > 0 && Instant::now() >= deadline {
            break;
        }
    }

    let elapsed = start.elapsed();
    println!("Results:");
    println!("  duration:     {:.2?}", elapsed);
    println!("  transactions: {} ({:.1} tps)", events, events as f64 / elapsed.as_secs_f64());
    println!("  queries:      {} ({:.1} qps)", queries, queries as f64 / elapsed.as_secs_f64());
    if args.report_latency && !latencies_us.is_empty() {
        latencies_us.sort();
        let p50 = latencies_us[latencies_us.len() * 50 / 100];
        let p95 = latencies_us[latencies_us.len() * 95 / 100];
        let p99 = latencies_us[latencies_us.len() * 99 / 100];
        println!("  latency (ms): p50={:.2}  p95={:.2}  p99={:.2}", p50 as f64 / 1000., p95 as f64 / 1000., p99 as f64 / 1000.);
    }
    println!();
    println!("Compare with MySQL/PostgreSQL using sysbench (see docs/benchmark.md).");
    Ok(())
}

fn run_olap(args: OlapArgs) -> anyhow::Result<()> {
    println!("RustafariDB OLAP benchmark (columnar SUM/COUNT/AVG)");
    println!("  rows: {}  queries: {}", args.rows, args.queries);
    println!();

    let state = Arc::new(SessionState::new());

    SqlExecutor::execute(state.as_ref(),
        "CREATE TABLE olap_bench (id BIGINT, key_col BIGINT, value_col BIGINT)")?;

    println!("Loading {} rows (batches of 50k)...", args.rows);
    let load_start = Instant::now();
    const BATCH: usize = 50_000;
    let mut loaded = 0usize;
    for start in (0..args.rows).step_by(BATCH) {
        let end = (start + BATCH).min(args.rows);
        let mut values: Vec<String> = Vec::with_capacity(BATCH * 3);
        for i in start..end {
            let key = (i % 1000) as i64;
            let value = (i * 2) as i64;
            values.push(format!("({}, {}, {})", i + 1, key, value));
        }
        let sql = format!("INSERT INTO olap_bench VALUES {}", values.join(", "));
        SqlExecutor::execute(state.as_ref(), &sql)?;
        loaded = end;
        if loaded % 500_000 == 0 && loaded > 0 {
            println!("  ... {} rows", loaded);
        }
    }
    let load_elapsed = load_start.elapsed();
    println!("  Loaded {} rows in {:.2?} ({:.0} rows/s)\n", loaded, load_elapsed, loaded as f64 / load_elapsed.as_secs_f64());

    // Warm-up
    let _ = SqlExecutor::execute(state.as_ref(), "SELECT SUM(value_col), COUNT(*), AVG(value_col) FROM olap_bench")?;

    let mut latencies_us: Vec<u64> = Vec::with_capacity(args.queries);
    let start = Instant::now();
    for i in 0..args.queries {
        let t0 = Instant::now();
        let result = SqlExecutor::execute(state.as_ref(), "SELECT SUM(value_col), COUNT(*), AVG(value_col) FROM olap_bench")?;
        latencies_us.push(t0.elapsed().as_micros() as u64);
        if let rustafari_executor::ExecutionResult::Rows(rows) = result {
            if rows.len() == 1 && i == 0 {
                println!("  Sample result: SUM={:?} COUNT={:?} AVG={:?}", rows[0].values.get(0), rows[0].values.get(1), rows[0].values.get(2));
            }
        }
    }
    let elapsed = start.elapsed();

    let rows_per_query = args.rows;
    let qps = args.queries as f64 / elapsed.as_secs_f64();
    let rows_per_sec = (args.rows * args.queries) as f64 / elapsed.as_secs_f64();

    println!("Results:");
    println!("  duration:        {:.2?}", elapsed);
    println!("  queries:         {} ({:.1} q/s)", args.queries, qps);
    println!("  rows per query:  {}", rows_per_query);
    println!("  throughput:      {:.0} rows/s (aggregated)", rows_per_sec);
    if args.report_latency && !latencies_us.is_empty() {
        latencies_us.sort();
        let p50 = latencies_us[latencies_us.len() * 50 / 100];
        let p95 = latencies_us[latencies_us.len() * 95 / 100];
        let p99 = latencies_us[latencies_us.len() * 99 / 100];
        println!("  latency (ms):    p50={:.2}  p95={:.2}  p99={:.2}", p50 as f64 / 1000., p95 as f64 / 1000., p99 as f64 / 1000.);
    }
    println!();
    println!("Compare with SingleStore, StarRocks, Snowflake, Databricks (see docs/benchmark-olap.md).");
    Ok(())
}

fn run_tpch(args: TpchArgs) -> anyhow::Result<()> {
    // TPC-H lineitem row count ≈ 6,001,217 * SF. SF 100 ≈ 600M rows (~32+ GB RAM).
    let num_rows = (6_001_217.0 * args.scale).max(1.0) as usize;
    println!("RustafariDB TPC-H (lineitem + Q1)");
    println!("  scale: {}  lineitem rows: {}  Q1 runs: {}", args.scale, num_rows, args.runs);
    if num_rows > 10_000_000 {
        println!("  Note: SF 100 (~600M rows) requires ~32+ GB RAM; load may take 30–60+ min.");
    }
    println!();

    let state = Arc::new(SessionState::new());

    SqlExecutor::execute(state.as_ref(),
        "CREATE TABLE lineitem (l_orderkey BIGINT, l_returnflag TEXT, l_linestatus TEXT, l_quantity BIGINT, l_extendedprice DOUBLE PRECISION, l_discount DOUBLE PRECISION, l_tax DOUBLE PRECISION, l_shipdate TEXT)")?;

    const RETURNFLAGS: [&str; 3] = ["A", "N", "R"];
    const LINESTATUSES: [&str; 2] = ["F", "O"];

    // Larger batches for SF 10+ to speed up load (fewer executor round-trips).
    let batch = if num_rows > 1_000_000 { 100_000 } else { 10_000 };
    let progress_every = if num_rows > 10_000_000 { 5_000_000 } else { 500_000 };

    println!("Loading {} lineitem rows (batch {}...)", num_rows, batch);
    let load_start = Instant::now();
    for start in (0..num_rows).step_by(batch) {
        let end = (start + batch).min(num_rows);
        let mut values: Vec<String> = Vec::with_capacity(batch * 8);
        for i in start..end {
            let orderkey = (i + 1) as i64;
            let rf = RETURNFLAGS[i % 3];
            let ls = LINESTATUSES[i % 2];
            let qty = (i % 50 + 1) as i64;
            let ext_price = 100.0 + (i % 99900) as f64;
            let disc = 0.01 * ((i % 10) as f64);
            let tax = 0.01 * ((i % 8) as f64);
            let shipdate = if i % 3 != 0 { "'1998-08-15'" } else { "'1998-10-01'" };
            values.push(format!("({}, '{}', '{}', {}, {}, {}, {}, {})", orderkey, rf, ls, qty, ext_price, disc, tax, shipdate));
        }
        let sql = format!("INSERT INTO lineitem VALUES {}", values.join(", "));
        SqlExecutor::execute(state.as_ref(), &sql)?;
        if end % progress_every == 0 || end == num_rows {
            let elapsed = load_start.elapsed().as_secs_f64();
            let rate = if elapsed > 0.0 { end as f64 / elapsed } else { 0.0 };
            println!("  ... {} rows ({:.0} rows/s)", end, rate);
        }
    }
    let load_elapsed = load_start.elapsed();
    println!("  Loaded in {:.2?} ({:.0} rows/s)\n", load_elapsed, num_rows as f64 / load_elapsed.as_secs_f64());

    // TPC-H Q1: Pricing Summary Report
    let q1 = "SELECT l_returnflag, l_linestatus, SUM(l_quantity), SUM(l_extendedprice), AVG(l_quantity), AVG(l_extendedprice), AVG(l_discount), COUNT(*) FROM lineitem WHERE l_shipdate <= '1998-09-02' GROUP BY l_returnflag, l_linestatus ORDER BY l_returnflag, l_linestatus";

    let mut latencies_us: Vec<u64> = Vec::with_capacity(args.runs);
    let start = Instant::now();
    for i in 0..args.runs {
        let t0 = Instant::now();
        let result = SqlExecutor::execute(state.as_ref(), q1)?;
        latencies_us.push(t0.elapsed().as_micros() as u64);
        if let rustafari_executor::ExecutionResult::Rows(rows) = result {
            if i == 0 {
                println!("  Q1 result: {} row(s)", rows.len());
                for (j, row) in rows.iter().take(3).enumerate() {
                    println!("    [{}] {:?}", j, row.values);
                }
            }
        }
    }
    let elapsed = start.elapsed();

    println!("Results (TPC-H Q1):");
    println!("  lineitem rows:   {}", num_rows);
    println!("  Q1 runs:         {}  ({:.1} runs/s)", args.runs, args.runs as f64 / elapsed.as_secs_f64());
    if !latencies_us.is_empty() {
        latencies_us.sort();
        let p50 = latencies_us[latencies_us.len() * 50 / 100];
        let p95_idx = (latencies_us.len() * 95 / 100).min(latencies_us.len().saturating_sub(1));
        let p99_idx = (latencies_us.len() * 99 / 100).min(latencies_us.len().saturating_sub(1));
        let p95 = latencies_us[p95_idx];
        let p99 = latencies_us[p99_idx];
        println!("  Q1 latency (ms): p50={:.2}  p95={:.2}  p99={:.2}", p50 as f64 / 1000., p95 as f64 / 1000., p99 as f64 / 1000.);
    }
    println!();
    println!("Compare with SingleStore, StarRocks, Snowflake, Databricks TPC-H (see docs/benchmark-olap.md).");
    Ok(())
}
