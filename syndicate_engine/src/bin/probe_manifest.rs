use std::{env, path::PathBuf};

use syndicate_engine::engine::runtime_probe::{
    TabRuntimeProbeExecution, TabRuntimeProbeManifest, tab_runtime_probe_archive_inputs,
};

fn main() {
    let mut execute = false;
    let mut root = "../original_assets".to_string();
    for arg in env::args().skip(1) {
        if arg == "--execute" {
            execute = true;
        } else {
            root = arg;
        }
    }

    let inputs = tab_runtime_probe_archive_inputs(PathBuf::from(&root));
    let manifest = TabRuntimeProbeManifest::from_archive_inputs(inputs.clone());

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

    if execute {
        let execution =
            TabRuntimeProbeExecution::from_manifest_and_archive_inputs(manifest, inputs);
        println!("execution:");
        println!("{}", execution.compact_status());
        println!("readiness: {}", execution.readiness_summary());
        println!("execution phases: {}", execution.phase_summary());
        for phase in &execution.phase_results {
            println!(
                "- {}. {} | selectors {} | readiness [{}] | groups {} | units {} | stop: {}",
                phase.phase.order(),
                phase.phase.label(),
                phase.selector_ids_summary(),
                phase.readiness_summary(),
                phase.aggregate_group_count,
                phase.aggregate_unit_count,
                phase.stop_condition
            );
        }
        println!("execution selectors:");
        for result in &execution.selector_results {
            println!(
                "- #{:02} {} | {} | {} | groups {} | units {} | strongest {} | {}",
                result.rank,
                result.selector_id,
                result.family,
                result.readiness.label(),
                result.aggregate_group_count,
                result.aggregate_unit_count,
                result.strongest_group,
                result.conservative_limitation
            );
        }
    }
}
