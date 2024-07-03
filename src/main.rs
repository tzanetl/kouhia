use std::{num::NonZeroUsize, path::PathBuf};

use anyhow::{anyhow, Result};
use clap::{Args, Parser, Subcommand};
use home::home_dir;
use include_dir::{include_dir, Dir};
use lazy_static::lazy_static;
use rusqlite::Connection;
use rusqlite_migration::{Migrations, SchemaVersion};
use rust_decimal::Decimal;

const MIGRATIONS_VERSION: NonZeroUsize = unsafe { NonZeroUsize::new_unchecked(1) };
static MIGRATIONS_DIR: Dir = include_dir!("$CARGO_MANIFEST_DIR/migrations");
lazy_static! {
    static ref MIGRATIONS: Migrations<'static> =
        Migrations::from_directory(&MIGRATIONS_DIR).unwrap();
}

fn default_db_path() -> PathBuf {
    let mut path = home_dir().expect("unable to get home directory");
    path.push(".kouhia");
    if !path.exists() {
        std::fs::create_dir(&path).expect("unable to create directory");
    }
    path.push("db.sqlite3");
    path
}

#[derive(Parser)]
#[command(
    version,
    about = "Log your work hour balance.",
    arg_required_else_help = true
)]
struct Cli {
    /// Path to database
    #[arg(long, default_value = default_db_path().into_os_string())]
    database: PathBuf,
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, PartialEq)]
enum Commands {
    /// Migrate database to the latest version
    Migrate,
    /// Display database schema information
    Schema,
    /// Add new entry
    Add {
        /// Entry date, "now" or YYYY-MM-DD
        date: String,
        /// Hour amount
        time: Decimal,
    },
    /// Tail latest database entries
    Tail(TailArgs),
    /// Print out current hour balance
    Balance,
}

#[derive(Subcommand, PartialEq)]
enum TailCommands {
    /// Database entries
    Entry,
    // /// Concatenated dates
    // Date,
}

#[derive(Args, PartialEq)]
struct TailArgs {
    /// Maximum number of entries to show
    #[arg(default_value = "10", short)]
    n: usize,
    #[command(subcommand)]
    command: TailCommands,
}

fn migrate(conn: &mut Connection) -> Result<()> {
    MIGRATIONS.to_latest(conn)?;
    Ok(())
}

fn schema(conn: &Connection) -> Result<()> {
    let schema_version = MIGRATIONS.current_version(conn)?;
    let version_str: String = match schema_version {
        SchemaVersion::NoneSet => "Not set".to_string(),
        SchemaVersion::Inside(v) => v.to_string(),
        SchemaVersion::Outside(v) => v.to_string(),
    };
    println!("Database schema version:  {}", &version_str);
    println!("Latest available version: {}", &MIGRATIONS_VERSION);
    Ok(())
}

fn add(conn: &Connection, date: &str, time: Decimal) -> Result<()> {
    let date_naive = if date == "now" {
        chrono::offset::Local::now().date_naive()
    } else {
        chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d")?
    };

    let date_string = date_naive.format("%Y-%m-%d").to_string();
    conn.execute(
        "INSERT INTO hours (date, time, deleted) VALUES (?1, ?2, 0)",
        (&date_string, time.to_string()),
    )?;
    Ok(())
}

fn tail_entry(n: usize) -> Result<()> {
    todo!()
}

fn tail(tail_args: TailArgs) -> Result<()> {
    match tail_args.command {
        TailCommands::Entry => tail_entry(tail_args.n),
    }
}

fn balance(conn: &Connection) -> Result<()> {
    let time: f64 = conn.query_row("SELECT TOTAL(time) FROM hours WHERE deleted = 0", (), |r| {
        r.get(0)
    })?;
    println!("Total hour balance: {:.1}", time);
    Ok(())
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    let mut conn = Connection::open(&cli.database)?;

    if cli.command == Commands::Migrate {
        migrate(&mut conn)?;
        return Ok(());
    } else if cli.command == Commands::Schema {
        schema(&conn)?;
        return Ok(());
    }

    if MIGRATIONS.current_version(&conn)? != SchemaVersion::Inside(MIGRATIONS_VERSION) {
        return Err(anyhow!("Database is not up to date with the latest schema"));
    }

    match cli.command {
        Commands::Add { date, time } => add(&conn, &date, time)?,
        Commands::Tail(tail_args) => tail(tail_args)?,
        Commands::Balance => balance(&conn)?,
        _ => (),
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrations_test() {
        assert!(MIGRATIONS.validate().is_ok());
    }
}
