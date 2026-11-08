//! CLI bulk importer for HMS desktop: load legacy data files into the local
//! SQLite mirror at `%APPDATA%\com.hms.desktop\hms.sqlite` (Windows DPAPI
//! works for the same user session).
//!
//! Usage:
//!   hms-import users userpass.md
//!   hms-import guests guest-daily_2026-06-01_2028-11-30.csv
//!   hms-import collections collection_2026-06-01_2028-11-01.csv
//!   hms-import reservations Reservation.xlsx [--anchor 2026-06-01]

use std::path::PathBuf;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 3 {
        eprintln!("Usage: {} <kind> <path> [--anchor YYYY-MM-DD]", args[0]);
        eprintln!("  kind: users | guests | collections | reservations");
        std::process::exit(1);
    }

    let kind = &args[1];
    let path = PathBuf::from(&args[2]);
    let anchor = args.iter()
        .position(|a| a == "--anchor")
        .and_then(|i| args.get(i + 1))
        .map(|s| s.to_string())
        .unwrap_or_else(|| "2026-06-01".into());

    if !path.exists() {
        eprintln!("Error: file not found: {}", path.display());
        std::process::exit(1);
    }

    // Windows APPDATA path: %APPDATA%\com.hms.desktop\hms.sqlite
    let db_path = if cfg!(windows) {
        let appdata = std::env::var("APPDATA").unwrap_or_else(|_| {
            eprintln!("Error: %APPDATA% not set");
            std::process::exit(1);
        });
        PathBuf::from(appdata).join("com.hms.desktop").join("hms.sqlite")
    } else {
        eprintln!("Error: CLI import only supports Windows (DPAPI-protected credentials)");
        std::process::exit(1);
    };

    if !db_path.exists() {
        eprintln!("Error: database not found at {}", db_path.display());
        eprintln!("Run the desktop app once to create the database first.");
        std::process::exit(1);
    }

    // Reuse the desktop's LocalStore + importer modules
    let mut store = match hms_desktop::local_store::LocalStore::open(&db_path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Error opening database: {e}");
            std::process::exit(1);
        }
    };

    let result = match kind.as_str() {
        "users" => hms_desktop::importer::import_users(&mut store, &path),
        "guests" => hms_desktop::importer::import_guest_register(&mut store, &path),
        "collections" => hms_desktop::importer::import_collection_ledger(&mut store, &path),
        "reservations" => hms_desktop::importer::import_reservation_workbook(&mut store, &path, &anchor),
        other => {
            eprintln!("Error: unknown import kind '{other}' (users|guests|collections|reservations)");
            std::process::exit(1);
        }
    };

    match result {
        Ok(summary) => {
            println!("{}", serde_json::to_string_pretty(&summary).unwrap());
        }
        Err(e) => {
            eprintln!("Import failed: {e}");
            std::process::exit(1);
        }
    }
}
