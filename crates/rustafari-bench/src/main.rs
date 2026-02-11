//! Sysbench-style OLTP benchmark for RustafariDB.
//!
//! Uses the same logical workload as sysbench's oltp_read_only:
//! - Table: sbtest (id, k, c, pad) matching sysbench schema
//! - Point SELECT by id
//! - Range SELECT by id (id >= ? AND id <= ? LIMIT N)
//!
//! Run MySQL/PostgreSQL with sysbench (see docs/benchmark.md) and compare results.

use clap::Parser;
use rand::Rng;
use rustafari_executor::{SessionState, SqlExecutor};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Sysbench-style OLTP read-only benchmark for RustafariDB
#[derive(Parser)]
#[command(name = "rustafari-bench")]
struct Args {
    /// Number of rows in the sbtest table (like sysbench --table-size)
    #[arg(long, default_value = "10000")]
    table_size: usize,

    /// Number of point SELECTs per "transaction" (like sysbench --point-selects)
    #[arg(long, default_value = "10")]
    point_selects: usize,

    /// Number of range SELECTs per "transaction" (like sysbench --simple-ranges)
    #[arg(long, default_value = "1")]
    simple_ranges: usize,

    /// Range size for range SELECT (id BETWEEN x AND x+range_size-1)
    #[arg(long, default_value = "100")]
    range_size: usize,

    /// Run benchmark for this many seconds (if 0, run total_events instead)
    #[arg(long, default_value = "10")]
    time: u64,

    /// Total number of "transactions" (each = point_selects + simple_ranges queries). Ignored if time > 0.
    #[arg(long, default_value = "0")]
    total_events: u64,

    /// Report latency percentiles (p50, p95, p99)
    #[arg(long, default_value = "true")]
    report_latency: bool,
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    println!("RustafariDB sysbench-style OLTP read-only benchmark");
    println!("  table_size: {}  point_selects: {}  simple_ranges: {}  range_size: {}",
        args.table_size, args.point_selects, args.simple_ranges, args.range_size);
    println!();

    let state = Arc::new(SessionState::new());

    // Create table (sysbench schema: id, k, c, pad)
    SqlExecutor::execute(state.as_ref(), 
        "CREATE TABLE sbtest (id BIGINT, k BIGINT, c TEXT, pad TEXT)")?;

    // Load data
    println!("Loading {} rows...", args.table_size);
    let load_start = Instant::now();
    const BATCH: usize = 500;
    for start in (0..args.table_size).step_by(BATCH) {
        let end = (start + BATCH).min(args.table_size);
        let mut values: Vec<String> = Vec::with_capacity(BATCH * 4);
        for i in start..end {
            let k = (i % args.table_size).max(1) as i64;
            let c = format!("{:0>119}-", i); // ~120 chars
            let pad = format!("{:0>59}-", i); // ~60 chars
            values.push(format!("({}, {}, '{}', '{}')", i + 1, k, c.replace('\'', "''"), pad.replace('\'', "''")));
        }
        let sql = format!("INSERT INTO sbtest VALUES {}", values.join(", "));
        SqlExecutor::execute(state.as_ref(), &sql)?;
    }
    let load_elapsed = load_start.elapsed();
    println!("  Loaded in {:.2?} ({:.0} rows/s)\n", load_elapsed, args.table_size as f64 / load_elapsed.as_secs_f64());

    // RNG for workload
    let mut rng = rand::thread_rng();
    let table_size = args.table_size as i64;

    let run_duration = if args.time > 0 {
        Duration::from_secs(args.time)
    } else {
        Duration::from_secs(u64::MAX)
    };

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
            let sql = format!("SELECT c FROM sbtest WHERE id = {} LIMIT 1", id);
            let _ = SqlExecutor::execute(state.as_ref(), &sql)?;
            queries += 1;
        }

        for _ in 0..args.simple_ranges {
            let id = rng.gen_range(1..=(table_size - args.range_size as i64).max(1));
            let id_end = id + args.range_size as i64;
            let sql = format!(
                "SELECT c FROM sbtest WHERE id >= {} AND id <= {} LIMIT {}",
                id, id_end, args.range_size
            );
            let _ = SqlExecutor::execute(state.as_ref(), &sql)?;
            queries += 1;
        }

        let elapsed = t_start.elapsed();
        if args.report_latency && latencies_us.len() < 500_000 {
            latencies_us.push(elapsed.as_micros() as u64);
        }
        events += 1;

        if args.time > 0 && Instant::now() >= deadline {
            break;
        }
    }

    let elapsed = start.elapsed();
    let qps = queries as f64 / elapsed.as_secs_f64();
    let tps = events as f64 / elapsed.as_secs_f64();

    println!("Results:");
    println!("  duration:     {:.2?}", elapsed);
    println!("  transactions: {} ({:.1} tps)", events, tps);
    println!("  queries:      {} ({:.1} qps)", queries, qps);

    if args.report_latency && !latencies_us.is_empty() {
        latencies_us.sort();
        let p50 = latencies_us[latencies_us.len() * 50 / 100];
        let p95 = latencies_us[latencies_us.len() * 95 / 100];
        let p99 = latencies_us[latencies_us.len() * 99 / 100];
        println!("  latency (ms): p50={:.2}  p95={:.2}  p99={:.2}",
            p50 as f64 / 1000., p95 as f64 / 1000., p99 as f64 / 1000.);
    }

    println!();
    println!("Compare with MySQL/PostgreSQL using sysbench (see docs/benchmark.md).");
    Ok(())
}
