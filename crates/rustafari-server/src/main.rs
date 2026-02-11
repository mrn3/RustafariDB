//! RustafariDB server and CLI.
//!
//! A unified database supporting SQL, MongoDB-style documents, OLTP, OLAP, and search.

use clap::{Parser, Subcommand};
use rustafari_core::CatalogSnapshot;
use rustafari_executor::{ExecutionResult, SessionState, SqlExecutor};
use std::path::Path;
use std::sync::Arc;

#[derive(Parser)]
#[command(name = "rustafari")]
#[command(about = "RustafariDB: unified OLTP, OLAP, and search database")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Run interactive SQL shell
    Shell,
    /// Execute a single SQL statement
    Query {
        #[arg(short, long)]
        sql: String,
    },
    /// Start the database server (gRPC)
    Serve {
        #[arg(short, long, default_value = "127.0.0.1:50051")]
        addr: String,
    },
}

fn data_dir() -> anyhow::Result<std::path::PathBuf> {
    Ok(std::env::current_dir()?.join("rustafari-data"))
}

fn catalog_path() -> anyhow::Result<std::path::PathBuf> {
    Ok(data_dir()?.join("catalog.json"))
}

fn load_catalog(state: &SessionState, path: &Path) -> anyhow::Result<()> {
    let bytes = std::fs::read(path)?;
    let snap: CatalogSnapshot = serde_json::from_slice(&bytes)?;
    state.catalog.write().load_snapshot(snap);
    Ok(())
}

fn save_catalog(state: &SessionState, path: &Path) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let snap = state.catalog.read().to_snapshot();
    let f = std::fs::File::create(path)?;
    serde_json::to_writer_pretty(f, &snap)?;
    Ok(())
}

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env().add_directive("rustafari=info".parse()?),
        )
        .init();

    let cli = Cli::parse();
    let state = Arc::new(SessionState::new());
    let catalog_path = catalog_path()?;
    if catalog_path.exists() {
        if let Err(e) = load_catalog(state.as_ref(), &catalog_path) {
            tracing::warn!("Could not load catalog from {}: {}", catalog_path.display(), e);
        }
    }

    match cli.command {
        Commands::Shell => run_shell(state, &catalog_path),
        Commands::Query { sql } => {
            let result = SqlExecutor::execute(state.as_ref(), &sql)?;
            print_result(&result);
            save_catalog(state.as_ref(), &catalog_path)?;
            Ok(())
        }
        Commands::Serve { addr } => {
            tracing::info!("RustafariDB server listening on {}", addr);
            // In production: spawn gRPC server with state
            Ok(())
        }
    }
}

fn run_shell(state: Arc<SessionState>, catalog_path: &Path) -> anyhow::Result<()> {
    let mut rl = rustyline::DefaultEditor::new()
        .map_err(|e| anyhow::anyhow!("readline: {}", e))?;
    let history_path = data_dir()?.join(".history");
    if history_path.exists() {
        let _ = rl.load_history(&history_path);
    }
    println!("RustafariDB shell. Type SQL or 'exit' to quit. Type 'help' for supported commands.\n");
    loop {
        let line = match rl.readline("rustafari> ") {
            Ok(s) => s,
            Err(rustyline::error::ReadlineError::Eof) => break,
            Err(rustyline::error::ReadlineError::Interrupted) => continue,
            Err(e) => return Err(e.into()),
        };
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let _ = rl.add_history_entry(line);
        if line.eq_ignore_ascii_case("exit") || line.eq_ignore_ascii_case("quit") {
            let _ = rl.save_history(&history_path);
            break;
        }
        let line_lower = line.trim_end_matches(';').trim();
        if line_lower.eq_ignore_ascii_case("help") {
            println!("  SHOW DATABASES          - list databases (namespaces)");
            println!("  SHOW TABLES             - list tables in default database (public)");
            println!("  CREATE TABLE name (col type, ...)  - create table (use db.table for a database)");
            println!("  DESCRIBE table  - show table columns (name, type, nullable)");
            println!("  INSERT INTO table VALUES (v1, v2), ...  - insert rows");
            println!("  SELECT cols FROM table [WHERE ...] [LIMIT n]  - query");
            println!("  exit / quit             - exit shell");
            continue;
        }
        if line_lower.eq_ignore_ascii_case("show databases") {
            let dbs = state.list_databases();
            if dbs.is_empty() {
                println!("(no databases; create a table in a namespace, e.g. CREATE TABLE mydb.mytable (...))");
            } else {
                for db in &dbs {
                    println!("{}", db);
                }
                println!("({} database(s))", dbs.len());
            }
            continue;
        }
        if line_lower.eq_ignore_ascii_case("show tables") {
            let tables = state.list_tables(None);
            if tables.is_empty() {
                println!("(no tables in 'public'; use CREATE TABLE to create one)");
            } else {
                for t in &tables {
                    println!("{}", t);
                }
                println!("({} table(s))", tables.len());
            }
            continue;
        }
        // Rewrite "describe table <name>" to "DESCRIBE <name>" (parser expects one identifier)
        let sql = if line_lower.starts_with("describe table ")
            || line_lower.starts_with("desc table ")
        {
            let rest = line_lower
                .strip_prefix("describe table ")
                .or_else(|| line_lower.strip_prefix("desc table "))
                .unwrap()
                .trim_end_matches(';')
                .trim();
            format!("DESCRIBE {}", rest)
        } else {
            line.to_string()
        };
        match SqlExecutor::execute(state.as_ref(), &sql) {
            Ok(r) => {
                print_result(&r);
                if let Err(e) = save_catalog(state.as_ref(), catalog_path) {
                    println!("Warning: could not persist catalog: {}", e);
                }
            }
            Err(e) => println!("Error: {}", e),
        }
    }
    Ok(())
}

fn print_result(r: &ExecutionResult) {
    match r {
        ExecutionResult::Rows(rows) => {
            for row in rows {
                println!("{:?}", row.values);
            }
            println!("({} row(s))", rows.len());
        }
        ExecutionResult::RowsAffected(n) => println!("Rows affected: {}", n),
    }
}
