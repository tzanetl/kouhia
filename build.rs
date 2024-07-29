use std::{env::var, fs, path::Path, str::FromStr};

use anyhow::Result;

fn main() -> Result<()> {
    println!("cargo:rerun-if-changed=migrations");

    let cargo_manifest_dir = &var("CARGO_MANIFEST_DIR")?;
    let migrations_dir = Path::new(cargo_manifest_dir).join("migrations");

    let mut latest_migration: usize = 0;
    for entry in std::fs::read_dir(migrations_dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            let dir_id = path
                .components()
                .last()
                .expect("no components found")
                .as_os_str()
                .to_str()
                .expect("cannot convect to str")
                .split("-")
                .next()
                .expect("invalid folder format");
            let migration_id = usize::from_str(dir_id).expect("invalid migrations id format");
            if migration_id > latest_migration {
                latest_migration = migration_id;
            }
        }
    }

    let out_dir = var("OUT_DIR").unwrap();
    let dest_path = Path::new(&out_dir).join("comp_const.rs");
    fs::write(
        &dest_path,
        format!(
            "use std::num::NonZeroUsize;

            const MIGRATIONS_VERSION: NonZeroUsize = unsafe {{ NonZeroUsize::new_unchecked({}) }};
            ",
            latest_migration
        ),
    )
    .unwrap();

    Ok(())
}
