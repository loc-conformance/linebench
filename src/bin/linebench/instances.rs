use std::collections::BTreeSet;
use std::io::Write;

use linebench::counters::{Definition, read_definition_for};
use linebench::fetch::{INSTANCE_SEPARATOR, identify_counter, read_manifest, stage_given};
use linebench::machine::Platform;
use linebench::measure::Instance;

use crate::config::{Locations, Options};
use crate::output::print_warning;

pub fn build_instances(
    out: &mut dyn Write,
    definitions: &[Definition],
    locations: &Locations,
    options: &Options,
    platform: Platform,
) -> Result<(Vec<Instance>, usize), String> {
    let manifest = read_manifest(&locations.counters_dir)?;
    let selected: Vec<String> = match &options.counters {
        Some(named) => named.clone(),
        None => definitions
            .iter()
            .map(|d| d.name.clone())
            .chain(locations.given.keys().cloned())
            .collect(),
    };
    if selected.is_empty() {
        return Err("--counters names nothing".to_string());
    }
    let mut seen = BTreeSet::new();
    let mut instances = Vec::new();
    for name in &selected {
        if !seen.insert(name.clone()) {
            return Err(format!("{name} is named twice"));
        }
        let instance = if let Some(entry) = locations.given.get(name) {
            let (counter, tag) = name
                .split_once(INSTANCE_SEPARATOR)
                .expect("checked when resolved");
            let definition = match &entry.definition {
                Some(path) => read_definition_for(path, counter)?,
                None => find_definition(definitions, counter)?,
            };
            let identity = stage_given(
                &definition,
                tag,
                &entry.binary,
                platform,
                &locations.counters_dir,
            )?;
            Instance {
                definition,
                identity,
            }
        } else {
            let definition = find_definition(definitions, name).map_err(|_| {
                format!(
                    "{name} is neither a counter definition ({}) nor a given instance ({})",
                    definitions
                        .iter()
                        .map(|d| d.name.as_str())
                        .collect::<Vec<_>>()
                        .join(", "),
                    if locations.given.is_empty() {
                        "none".to_string()
                    } else {
                        locations
                            .given
                            .keys()
                            .cloned()
                            .collect::<Vec<_>>()
                            .join(", ")
                    }
                )
            })?;
            let identity =
                identify_counter(&definition, platform, &locations.counters_dir, &manifest)?;
            Instance {
                definition,
                identity,
            }
        };
        instances.push(instance);
    }
    let find_control = |named: &str| instances.iter().position(|i| i.get_name() == named);
    let control = match (&options.control, &locations.control) {
        (Some(named), _) => find_control(named).ok_or_else(|| {
            format!(
                "--control {named} is not among the instances of this run ({})",
                selected.join(", ")
            )
        })?,
        (None, Some(named)) => match find_control(named) {
            Some(found) => found,
            None => {
                print_warning(
                    out,
                    &format!(
                        "the control {named} from linebench.conf is not in this run, so {} stands \
                         in as the control",
                        instances[0].get_name()
                    ),
                )?;
                0
            }
        },
        (None, None) => 0,
    };
    Ok((instances, control))
}

fn find_definition(definitions: &[Definition], counter: &str) -> Result<Definition, String> {
    definitions
        .iter()
        .find(|d| d.name == counter)
        .cloned()
        .ok_or_else(|| format!("no counter definition named {counter}"))
}
