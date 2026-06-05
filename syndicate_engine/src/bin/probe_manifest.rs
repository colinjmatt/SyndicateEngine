use std::{env, path::PathBuf};

use syndicate_engine::engine::runtime_probe::TabRuntimeProbeManifest;

fn main() {
    let root = env::args()
        .nth(1)
        .unwrap_or_else(|| "../original_assets".to_string());
    let manifest = TabRuntimeProbeManifest::from_root(PathBuf::from(&root));

    println!("{}", manifest.compact_status());
    println!("source root: {root}");
    println!(
        "archives: {} parsed TAB/DAT pairs; families: {} selected from {} candidates",
        manifest.parsed_archives, manifest.selected_families, manifest.total_candidate_families
    );
    println!("support tiers: {}", manifest.selector_tier_summary());
    println!("phases: {}", manifest.phase_summary());
    println!("preconditions: {}", manifest.preconditions_summary());
    println!("stop conditions: {}", manifest.stop_conditions_summary());

    if manifest.phases.is_empty() {
        println!("dry-run phases: none");
    } else {
        println!("dry-run phases:");
        for phase in &manifest.phases {
            println!(
                "- {}. {} | selectors {} | families [{}] | support [{}] | stop: {}",
                phase.phase.order(),
                phase.phase.label(),
                phase.selector_ids_summary(),
                phase.families_summary(),
                phase.support_tiers_summary(),
                phase.stop_condition
            );
        }
    }

    if manifest.selectors.is_empty() {
        println!("selectors: none");
    } else {
        println!("selectors:");
        for selector in &manifest.selectors {
            println!(
                "- #{:02} {} | {} | {} | {} | {}",
                selector.rank,
                selector.id,
                selector.family,
                selector.support_tier.label(),
                selector.focus,
                selector.conservative_limitation()
            );
        }
    }
}
