use std::{cmp::max, collections::HashSet, path::PathBuf, str::FromStr};

use anyhow::{anyhow, Result};
use chrono::NaiveDate;
use clap::{Args, Parser, Subcommand, ValueEnum};
use home::home_dir;
use include_dir::{include_dir, Dir};
use lazy_static::lazy_static;
use rusqlite::{functions::FunctionFlags, Connection};
use rusqlite_migration::{Migrations, SchemaVersion};
use rust_decimal::Decimal;

static MIGRATIONS_DIR: Dir = include_dir!("$CARGO_MANIFEST_DIR/migrations");
lazy_static! {
    static ref MIGRATIONS: Migrations<'static> =
        Migrations::from_directory(&MIGRATIONS_DIR).unwrap();
}

const DATE_FORMAT: &'static str = "%Y-%m-%d";

include!(concat!(env!("OUT_DIR"), "/comp_const.rs"));

fn default_db_path() -> PathBuf {
    let mut path = home_dir().expect("unable to get home directory");
    path.push(".kouhia");
    if !path.exists() {
        std::fs::create_dir(&path).expect("unable to create directory");
    }
    path.push("db.sqlite3");
    path
}

fn add_not_undo_function(conn: &Connection, not_undo: bool) -> Result<()> {
    conn.create_scalar_function("not_undo", 0, FunctionFlags::empty(), move |_| Ok(not_undo))?;
    Ok(())
}

fn parse_date(date: &str) -> Result<NaiveDate> {
    if date == "now" {
        Ok(chrono::offset::Local::now().date_naive())
    } else {
        Ok(NaiveDate::parse_from_str(date, DATE_FORMAT)?)
    }
}

fn parse_non_zero(d: &str) -> Result<Decimal> {
    let value = Decimal::from_str(d)?;
    if value == Decimal::from_str("0")? {
        return Err(anyhow!("Time must be non-zero"));
    }
    Ok(value)
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
        #[arg(value_parser = parse_date)]
        date: NaiveDate,
        /// Hour amount
        #[arg(allow_negative_numbers = true, value_parser = parse_non_zero)]
        time: Decimal,
    },
    /// Delete database items
    Delete(DeleteArgs),
    /// Tail latest database entries
    Tail(TailArgs),
    /// Print out current hour balance
    Balance,
    /// Undo previous operation
    Undo {
        /// Number of changes to undo
        #[arg(default_value = "1")]
        depth: usize,
    },
}

#[derive(Clone, ValueEnum, PartialEq)]
enum TailCommands {
    /// Database entries
    Entry,
    /// Concatenated dates
    Date,
}

#[derive(Args, PartialEq)]
struct TailArgs {
    /// Maximum number of entries to show
    #[arg(default_value = "10", short)]
    n: usize,
    #[arg(value_enum)]
    command: TailCommands,
}

#[derive(Args, Debug, PartialEq)]
struct DeleteArgs {
    #[clap(flatten)]
    select: DBSelectGroup,
}

#[derive(Args, Clone, Debug, PartialEq)]
#[group(required = true, multiple = false)]
struct DBSelectGroup {
    /// Select by database entry id
    #[arg(short, num_args = 1..)]
    entry: Option<Vec<usize>>,
    /// Select by date
    #[arg(short, num_args = 1.., value_parser = parse_date)]
    date: Option<Vec<NaiveDate>>,
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
    let indent = max(
        version_str.chars().count(),
        MIGRATIONS_VERSION.to_string().chars().count(),
    );
    println!(
        "Database schema version:  {:>indent$}",
        &version_str,
        indent = indent
    );
    println!(
        "Latest available version: {:>indent$}",
        &MIGRATIONS_VERSION,
        indent = indent
    );
    Ok(())
}

fn add(conn: &Connection, date: NaiveDate, time: Decimal) -> Result<()> {
    let date_string = date.format("%Y-%m-%d").to_string();
    conn.execute(
        "INSERT INTO hours (date, time, deleted) VALUES (?1, ?2, 0)",
        (&date_string, time.to_string()),
    )?;
    Ok(())
}

fn tail_entry(conn: &Connection, n: usize) -> Result<()> {
    let mut statement = conn.prepare(&format!(
        "SELECT entry_id, date, time FROM hours WHERE deleted = 0 ORDER BY entry_id DESC LIMIT {}",
        n
    ))?;

    let entry_iter = statement.query_map([], |row| {
        Ok((
            row.get::<usize, usize>(0)?,
            row.get::<usize, String>(1)?,
            row.get::<usize, f64>(2)?,
        ))
    })?;

    println!("{:>10} {:>10} {:>10}", "ID", "Date", "Time");
    for entry in entry_iter {
        let (entry_id, date, time) = entry.expect("failed to read row");
        println!("{:>10} {:>10} {:>10.1}", entry_id, date, time);
    }

    Ok(())
}

fn tail_date(conn: &Connection, n: usize) -> Result<()> {
    let mut statement =
        conn.prepare("SELECT date, time FROM hours WHERE deleted = 0 ORDER BY date DESC")?;
    let mut rows = statement.query([])?;

    let first_row = if let Some(row) = rows.next()? {
        row
    } else {
        return Ok(());
    };

    let mut date = NaiveDate::parse_from_str(&first_row.get::<usize, String>(0)?, DATE_FORMAT)?;
    let mut time_sum = first_row.get::<usize, f64>(1)?;

    println!("{:>10} {:>10}", "Date", "Time");

    let mut count = 1;

    while let Some(row) = rows.next()? {
        let next_date = NaiveDate::parse_from_str(&row.get::<usize, String>(0)?, DATE_FORMAT)?;
        let next_time = row.get::<usize, f64>(1)?;

        if next_date == date {
            time_sum += next_time;
            continue;
        }

        println!("{:>10} {:>10.1}", date.format(DATE_FORMAT), time_sum);
        date = next_date;
        time_sum = next_time;
        count += 1;

        if count > n {
            return Ok(());
        }
    }
    println!("{:>10} {:>10.1}", date.format(DATE_FORMAT), time_sum);

    Ok(())
}

fn tail(conn: &Connection, tail_args: TailArgs) -> Result<()> {
    match tail_args.command {
        TailCommands::Entry => tail_entry(conn, tail_args.n)?,
        TailCommands::Date => tail_date(conn, tail_args.n)?,
    };
    Ok(())
}

fn balance(conn: &Connection) -> Result<()> {
    let time: f64 = conn.query_row("SELECT TOTAL(time) FROM hours WHERE deleted = 0", (), |r| {
        r.get(0)
    })?;
    println!("Total hour balance: {:.1}", time);
    Ok(())
}

fn delete_entry(conn: &mut Connection, ids: HashSet<usize>) -> Result<()> {
    let tx = conn.transaction()?;
    {
        let mut statement = tx.prepare("UPDATE hours SET deleted = 1 WHERE entry_id = ?1")?;
        for i in ids {
            statement.execute([i])?;
        }
    }
    tx.commit()?;
    Ok(())
}

fn delete_date(conn: &mut Connection, dates: HashSet<NaiveDate>) -> Result<()> {
    let tx = conn.transaction()?;
    {
        let mut statement = tx.prepare("UPDATE hours SET deleted = 1 WHERE date = ?1")?;
        for d in dates {
            statement.execute([d.format(&DATE_FORMAT).to_string()])?;
        }
    }
    tx.commit()?;
    Ok(())
}

fn delete(conn: &mut Connection, delete_args: DeleteArgs) -> Result<()> {
    if let Some(entry_ids) = delete_args.select.entry {
        let ids_set = HashSet::from_iter(entry_ids.into_iter());
        delete_entry(conn, ids_set)?;
    } else if let Some(dates) = delete_args.select.date {
        let dates_set = HashSet::from_iter(dates.into_iter());
        delete_date(conn, dates_set)?;
    } else {
        return Err(anyhow!("no selector"));
    }
    Ok(())
}

#[derive(Debug)]
struct UndoRow {
    row_id: usize,
    entry_id: usize,
    deleted_old: bool,
    processed: bool,
}

fn undo(conn: &Connection, depth: usize) -> Result<()> {
    dbg!(&depth);
    let mut statement = conn
        .prepare("SELECT row_id, entry_id, deleted_old, processed FROM undolog ORDER BY row_id")?;
    let mut rows = statement.query_map([], |row| {
        Ok(UndoRow {
            row_id: row.get(0)?,
            entry_id: row.get(1)?,
            deleted_old: row.get(2)?,
            processed: row.get(3)?,
        })
    })?;

    let mut first_found = false;

    while let Some(row) = rows.next() {
        let row = row?;
        dbg!(&row);

        if row.processed == false {
            first_found = true
        }
    }

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

    add_not_undo_function(&conn, !(matches!(cli.command, Commands::Undo { depth: _ })))?;

    match cli.command {
        Commands::Migrate => todo!(),
        Commands::Schema => todo!(),
        Commands::Add { date, time } => add(&conn, date, time)?,
        Commands::Tail(tail_args) => tail(&conn, tail_args)?,
        Commands::Balance => balance(&conn)?,
        Commands::Delete(delete_args) => delete(&mut conn, delete_args)?,
        Commands::Undo { depth } => undo(&conn, depth)?,
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
