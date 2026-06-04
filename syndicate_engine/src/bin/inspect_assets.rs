use std::{env, path::PathBuf};

use syndicate_engine::engine::report::write_report;

fn main() -> std::io::Result<()> {
    let mut args = env::args().skip(1);
    let root = args
        .next()
        .unwrap_or_else(|| "../original_assets".to_string());
    let output = args
        .next()
        .unwrap_or_else(|| "../docs/generated/asset-report.md".to_string());

    let root = PathBuf::from(root);
    let output = PathBuf::from(output);
    write_report(&root, &output)?;
    println!("Wrote asset inspection report to {}", output.display());
    Ok(())
}
