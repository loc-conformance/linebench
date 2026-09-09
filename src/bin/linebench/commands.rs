use std::env;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;

use serde_json::Value;

use linebench::corpus::{
    Counted, Parity, Verdict, check_commit, check_declares_files, count_tracked_files,
    describe_empty_count, judge_parity, read_git_state, setup_corpus,
};
use linebench::counters::{Acquisition, Definition};
use linebench::defender::{
    CounterBinary, DefenderState, ProcessExclusions, explain_unequal_exclusions,
    find_unequal_exclusions, judge_process_exclusions, read_defender_state,
};
use linebench::fetch::{fetch_counter, read_manifest};
use linebench::insight::{FLOOR_RUNS, FLOOR_WARMUP, VERSION_SET};
use linebench::insight::{format_floor, get_floor_set_name};
use linebench::machine::{
    Platform, collect_machine, detect_arch, plan_prep, sample_background_busy,
};
use linebench::measure::{
    Instance, Runner, Settings, Style, Table, build_args, build_command, capture_json_outputs,
    capture_plain_output, get_capture_name, join_command, run_phases,
};
use linebench::measure::{OUT_DIR, TABLES};
use linebench::newest::{
    Lookup, Standing, choose_counters_to_look_up, collect_newest_releases, describe_newest,
    describe_pin_origin, describe_pinned_version, find_newest_release, judge_newest,
    remember_newest,
};
use linebench::os::{capture_with_status, is_privileged};
use linebench::read::{compare_documents, find_absent_volatile_paths, read_counts};
use linebench::record::RECORD_FORMAT;
use linebench::record::{
    CorpusRecord, CountRecord, InstanceRecord, Record, RunSettings, append_to_notes,
    calculate_drift, collect_measurements, describe_empty_bare_counts, format_busy,
    format_summary_tables, format_thousands, format_utc_date, format_utc_stamp,
    read_seconds_since_epoch, shorten_version, write_csvs, write_notes, write_record,
};

use crate::config::{Locations, Options};
use crate::instances::build_instances;
use crate::output::{
    Color, Output, get_report_style, paint, print_header, print_line, print_warning,
};
use crate::page::{
    Collected, FoundRun, collect_records, find_named_run, format_against, format_since,
    write_results_page,
};
use crate::page::{LOCAL_DIR, PAGE_FILE};
use crate::prep::{self, AppliedPrep};

const TRANSCRIPT_FILE: &str = "transcript.txt";
const FLOOR_TARGET: &str = "floor";
const NOISE_RUNS: u32 = 5;
const NOISE_RETRY_SECONDS: u64 = 5;
const UNSTEADY_STEP: usize = 2;
const UNREADABLE_OUTPUT_LINES: usize = 10;
const BACKGROUND_CORE_STEPS: [f64; 3] = [0.75, 1.5, 3.0];
const SPREAD_STEPS: [f64; 3] = [5.0, 10.0, 15.0];
const VERDICTS: [&str; 4] = [
    "steady",
    "relatively steady",
    "somewhat unsteady",
    "not steady",
];
const COLD_CACHE_RATIO: f64 = 1.5;
const IDENTICAL: &str = "identical";
const DIFFER: &str = "differ";

pub fn run_setup(
    out: &mut dyn Write,
    options: &Options,
    locations: &Locations,
    definitions: &[Definition],
    platform: Platform,
) -> Result<i32, String> {
    if is_privileged(platform) && !options.allow_elevated {
        return Err(format!(
            "setup writes files as you, so it does not run as {}; run it from an ordinary \
             terminal, or pass --allow-elevated where there is no ordinary user, as on a CI \
             runner",
            if platform == Platform::Windows {
                "administrator"
            } else {
                "root"
            }
        ));
    }
    fs::create_dir_all(&locations.counters_dir).map_err(|error| {
        format!(
            "{} could not be created: {error}",
            locations.counters_dir.display()
        )
    })?;
    if let Some(named) = &options.counters {
        let unknown: Vec<&str> = named
            .iter()
            .filter(|name| !definitions.iter().any(|d| &d.name == *name))
            .map(String::as_str)
            .collect();
        if !unknown.is_empty() {
            return Err(format!(
                "no counter definition is named {}; setup fetches counters by the name of \
                 their definition, so a given instance is not set up here",
                unknown.join(", ")
            ));
        }
    }
    let wanted: Vec<&Definition> = match &options.counters {
        Some(named) => definitions
            .iter()
            .filter(|d| named.contains(&d.name))
            .collect(),
        None => definitions.iter().collect(),
    };
    let mut manifest = read_manifest(&locations.counters_dir)?;
    let arch = detect_arch();
    let mut failed = Vec::new();
    print_header(out, "== counters")?;
    for definition in wanted {
        print_line(out, &paint(Color::Bold, &definition.name).to_string())?;
        if options.counters.is_none() && locations.skip.contains(&definition.name) {
            print_line(out, "  linebench.conf leaves it out")?;
            continue;
        }
        if definition.acquisition.is_none() && options.counters.is_none() {
            print_line(
                out,
                &format!(
                    "  no release to fetch; a build of it is measured with --given {}=<path>",
                    definition.name
                ),
            )?;
            continue;
        }
        let mut to_fetch = None;
        let mut newest = definition.shipped_version.is_some();
        if options.newest
            && let Some(how) = &definition.acquisition
        {
            if definition.added {
                print_line(
                    out,
                    &format!(
                        "  --newest leaves it alone, since the definition comes from {}; change \
                         the version there",
                        definition.path.display()
                    ),
                )?;
            } else {
                match find_newest_release(definition) {
                    Ok(version) => {
                        if let Err(refused) = remember_newest(
                            &locations.counters_dir,
                            &definition.name,
                            how,
                            &version,
                            read_seconds_since_epoch(),
                        ) {
                            print_warning(out, &refused)?;
                        }
                        let shipped = definition.shipped_version.as_deref();
                        match judge_newest(how, &version) {
                            Standing::Newer => {
                                print_line(
                                    out,
                                    &format!(
                                        "  the newest release is {version}; {}",
                                        describe_pinned_version(how, shipped)
                                    ),
                                )?;
                                let mut chosen = definition.clone();
                                chosen.acquisition = Some(Acquisition {
                                    version,
                                    ..how.clone()
                                });
                                to_fetch = Some(chosen);
                                newest = true;
                            }
                            Standing::Same | Standing::Behind | Standing::Differs => print_line(
                                out,
                                &format!(
                                    "  {}",
                                    describe_newest(&definition.name, how, shipped, &version)
                                ),
                            )?,
                        }
                    }
                    Err(refused) => {
                        print_line(out, &format!("  {}", paint(Color::Red, &refused)))?;
                        failed.push(definition.name.clone());
                        continue;
                    }
                }
            }
        }
        if let Err(refused) = fetch_counter(
            out,
            to_fetch.as_ref().unwrap_or(definition),
            platform,
            &arch,
            &locations.counters_dir,
            &mut manifest,
            newest,
        ) {
            print_line(out, &format!("  {}", paint(Color::Red, &refused)))?;
            failed.push(definition.name.clone());
        }
    }
    print_header(out, "== corpus")?;
    if let Err(refused) = setup_corpus(out, &locations.corpus, &locations.checkout) {
        print_line(out, &format!("  {}", paint(Color::Red, &refused)))?;
        failed.push(locations.corpus.name.clone());
    }
    print_line(out, "")?;
    if failed.is_empty() {
        print_line(out, &paint(Color::Green, "all set.").to_string())?;
        return Ok(0);
    }
    print_line(
        out,
        &paint(Color::Red, &format!("not ready: {}", failed.join(", "))).to_string(),
    )?;
    Ok(1)
}

pub fn run_check(
    out: &mut dyn Write,
    options: &Options,
    locations: &Locations,
    definitions: &[Definition],
    platform: Platform,
) -> Result<i32, String> {
    check_commit(&locations.corpus, &locations.checkout)?;
    let chosen = build_instances(out, definitions, locations, options, platform)?;
    let (instances, control) = (chosen.instances, chosen.control);
    print_header(
        out,
        &format!(
            "== check: {} at {}",
            locations.corpus.name,
            locations.checkout.display()
        ),
    )?;
    let scratch = Scratch::create("check")?;
    let counters = choose_counters_to_look_up(instances.iter().map(|i| &i.definition));
    let now = read_seconds_since_epoch();
    let (checked, looked_up) = thread::scope(|scope| {
        let lookups = scope.spawn(|| {
            collect_newest_releases(&counters, &locations.counters_dir, now, find_newest_release)
        });
        let checked = check_everything(
            out,
            options,
            locations,
            &instances,
            control,
            platform,
            scratch.get_path(),
        );
        (checked, lookups.join())
    });
    let (bad, nothing_compared) = checked?;
    let (lookups, warning) =
        looked_up.map_err(|_| "the release lookups stopped short".to_string())?;
    print_newest_releases(out, &lookups)?;
    if let Some(warning) = warning {
        print_warning(out, &warning)?;
    }
    print_line(out, "")?;
    if bad.is_empty() {
        let verdict = if nothing_compared {
            "all good, though equal work was not compared."
        } else {
            "all good."
        };
        print_line(out, &paint(Color::Green, verdict).to_string())?;
        return Ok(0);
    }
    print_line(
        out,
        &paint(
            Color::Red,
            &format!("{} checks failed: {}", bad.len(), bad.join(", ")),
        )
        .to_string(),
    )?;
    Ok(1)
}

pub fn run_noise(
    out: &mut dyn Write,
    options: &Options,
    locations: &Locations,
    definitions: &[Definition],
    platform: Platform,
) -> Result<i32, String> {
    check_commit(&locations.corpus, &locations.checkout)?;
    let chosen = build_instances(out, definitions, locations, options, platform)?;
    let (instances, control) = (chosen.instances, chosen.control);
    let scratch = Scratch::create("noise")?;
    print_header(out, "== noise")?;
    let Some(worst) = judge_noise(out, locations, &instances, control, platform, &scratch)? else {
        return Ok(1);
    };
    if worst < UNSTEADY_STEP {
        return Ok(0);
    }
    print_line(out, "")?;
    print_line(
        out,
        &format!("unsteady, so measuring again in {NOISE_RETRY_SECONDS} s"),
    )?;
    thread::sleep(Duration::from_secs(NOISE_RETRY_SECONDS));
    print_header(out, "== noise, measured again")?;
    match judge_noise(out, locations, &instances, control, platform, &scratch)? {
        Some(worst) if worst < UNSTEADY_STEP => Ok(0),
        _ => Ok(1),
    }
}

fn judge_noise(
    out: &mut dyn Write,
    locations: &Locations,
    instances: &[Instance],
    control: usize,
    platform: Platform,
    scratch: &Scratch,
) -> Result<Option<usize>, String> {
    let cores = thread::available_parallelism().map_or(1, |c| c.get());
    let busy = sample_background_busy(platform);
    let others = busy.map(|b| (b * cores as f64 / 100.0 * 10.0).round() / 10.0);
    match (busy, others) {
        (Some(busy), Some(others)) => print_line(
            out,
            &format!(
                "   background    {busy}% busy, {}",
                paint_by_steps(
                    others,
                    &BACKGROUND_CORE_STEPS,
                    &format!("~{others} of {cores} cores")
                )
            ),
        )?,
        _ => print_line(out, "   background    not sampled on this platform")?,
    }
    let export = time_the_control(
        out,
        locations,
        instances,
        control,
        platform,
        scratch.get_path(),
    )?;
    let Some(result) = export.as_ref().and_then(|v| v["results"].get(0)) else {
        print_line(out, "   workload      hyperfine failed")?;
        return Ok(None);
    };
    let times: Vec<f64> = result["times"]
        .as_array()
        .map(|a| a.iter().filter_map(Value::as_f64).collect())
        .unwrap_or_default();
    if times.len() < 2 {
        print_line(out, "   workload      hyperfine gave too few runs")?;
        return Ok(None);
    }
    let warm = &times[1..];
    let warm_mean = warm.iter().sum::<f64>() / warm.len() as f64;
    let (least, most) = warm
        .iter()
        .fold((f64::MAX, 0.0f64), |(lo, hi), t| (lo.min(*t), hi.max(*t)));
    let spread = if least > 0.0 {
        ((most - least) / least * 1000.0).round() / 10.0
    } else {
        0.0
    };
    let mean = result["mean"].as_f64().unwrap_or(0.0);
    let cpu = result["user"].as_f64().unwrap_or(0.0) + result["system"].as_f64().unwrap_or(0.0);
    let reached = if mean > 0.0 {
        (cpu / mean * 10.0).round() / 10.0
    } else {
        0.0
    };
    let first = if warm_mean > 0.0 {
        (times[0] / warm_mean * 100.0).round() / 100.0
    } else {
        0.0
    };
    print_line(
        out,
        &format!(
            "   workload      {} on {}, {NOISE_RUNS} runs",
            instances[control].get_name(),
            locations.corpus.name
        ),
    )?;
    print_line(
        out,
        &format!(
            "   spread        {} ({} to {} ms)",
            paint_by_steps(spread, &SPREAD_STEPS, &format!("{spread}%")),
            (least * 1000.0).round(),
            (most * 1000.0).round()
        ),
    )?;
    print_line(out, &format!("   parallelism   {reached} of {cores} cores"))?;
    print_line(
        out,
        &format!(
            "   cache         {}, first run {first}x the warm mean",
            if first >= COLD_CACHE_RATIO {
                "cold"
            } else {
                "warm"
            }
        ),
    )?;
    let mut zones = vec![(
        find_step(spread, &SPREAD_STEPS),
        format!("spread at {spread}%"),
    )];
    if let Some(others) = others {
        zones.push((
            find_step(others, &BACKGROUND_CORE_STEPS),
            format!("background at {others} of {cores} cores"),
        ));
    }
    let worst = zones.iter().map(|(zone, _)| *zone).max().unwrap_or(0);
    let blamed: Vec<&str> = zones
        .iter()
        .filter(|(zone, _)| *zone == worst)
        .map(|(_, reason)| reason.as_str())
        .collect();
    print_line(out, "")?;
    let verdict = if worst == 0 {
        format!("{}.", VERDICTS[worst])
    } else {
        format!("{}: {}.", VERDICTS[worst], blamed.join(" and "))
    };
    print_line(out, &paint_step(worst, &verdict))?;
    Ok(Some(worst))
}

pub fn run_benchmark(
    out: &mut Output,
    options: &Options,
    locations: &Locations,
    definitions: &[Definition],
    platform: Platform,
) -> Result<i32, String> {
    let privileged = is_privileged(platform);
    check_commit(&locations.corpus, &locations.checkout)?;
    check_declares_files(&locations.corpus)?;
    let chosen = build_instances(out, definitions, locations, options, platform)?;
    let (instances, control) = (chosen.instances, chosen.control);
    check_identical_pairs(&options.expect_identical, &instances)?;
    let is_local = instances.iter().any(Instance::is_an_experiment);
    let collected = collect_records(&locations.out);
    let against = match &options.against {
        Some(stamp) => Some(find_named_run(&collected, stamp, &locations.out, is_local)?),
        None => None,
    };
    let binaries = collect_binaries(&instances, platform)?;
    let defender = read_defender_state(platform, privileged, &locations.checkout, &binaries);
    let unequal = match find_unequal_exclusions(&defender) {
        Some(unequal) if !options.allow_unequal => {
            return Err(explain_unequal_exclusions(&unequal));
        }
        Some(unequal) => {
            print_warning(
                out,
                &format!("measuring with unequal MS Defender exclusions: {unequal}"),
            )?;
            print_warning(
                out,
                "the results may not be representative of real performance",
            )?;
            Some(unequal)
        }
        None => None,
    };
    let plan = if options.no_prep {
        Vec::new()
    } else {
        plan_prep(platform)
    };
    prep::announce_prep(out, &plan, privileged, options.yes, platform)?;
    let applied = if privileged {
        prep::apply_prep(out, plan, platform)?
    } else {
        None
    };
    let prepared = applied
        .as_ref()
        .map_or_else(Vec::new, AppliedPrep::get_applied_steps);
    let settings = Settings {
        warmup: options.warmup.unwrap_or(Settings::default().warmup),
        runs: options.runs.unwrap_or(Settings::default().runs),
        settle: options.settle.unwrap_or(Settings::default().settle),
    };
    let now = read_seconds_since_epoch();
    let stamp = format_utc_stamp(now);
    let mut res = locations.out.clone();
    if is_local {
        res = res.join(LOCAL_DIR);
    }
    let res = res
        .join(&locations.corpus.name)
        .join(platform.as_str())
        .join(&stamp);
    if res.exists() {
        return Err(format!(
            "{} is already there, refusing to write over it",
            res.display()
        ));
    }
    fs::create_dir_all(res.join(OUT_DIR))
        .map_err(|error| format!("{} could not be created: {error}", res.display()))?;
    out.start_transcript(&res.join(TRANSCRIPT_FILE))?;
    let context = RunContext {
        options,
        locations,
        instances: &instances,
        control,
        platform,
        defender,
        unequal,
        prepared,
        settings,
        stamp: &stamp,
        now,
        res: &res,
        is_local,
        left_out: chosen.left_out,
        collected,
        against,
    };
    let outcome = measure_and_record(out, context);
    if let Err(refused) = &outcome {
        out.write_to_transcript(&format!("ERROR: {refused}"));
    }
    out.stop_transcript();
    drop(applied);
    outcome
}

pub fn run_report(out: &mut dyn Write, results: &Path) -> Result<i32, String> {
    let collected = collect_records(results);
    for message in &collected.skipped {
        print_warning(out, message)?;
    }
    if write_results_page(results, &collected.found)? {
        print_line(out, &format!("wrote {}", results.join(PAGE_FILE).display()))?;
        return Ok(0);
    }
    print_line(
        out,
        &format!(
            "no run could be read under {}, so nothing was written",
            results.display()
        ),
    )?;
    Ok(1)
}

pub fn run_insights(
    out: &mut dyn Write,
    options: &Options,
    locations: &Locations,
    definitions: &[Definition],
    platform: Platform,
) -> Result<i32, String> {
    let chosen = build_instances(out, definitions, locations, options, platform)?;
    let instances = chosen.instances;
    let scratch = Scratch::create("insights")?;
    let target = create_empty_repository(scratch.get_path())?;
    print_header(out, "== floor")?;
    let mut runner = Runner::new(
        scratch.get_path(),
        Settings {
            warmup: FLOOR_WARMUP,
            runs: FLOOR_RUNS,
            settle: 0,
        },
        platform,
        collect_scrub(&instances),
        get_report_style(),
    );
    let mut distinct: Vec<(String, String)> = Vec::new();
    let mut versions: Vec<(String, String)> = Vec::new();
    for instance in &instances {
        let command = join_command(
            &instance.identity.binary,
            std::slice::from_ref(&instance.definition.version_flag),
        );
        if !distinct.iter().any(|(_, timed)| *timed == command) {
            distinct.push((instance.get_name().to_string(), command.clone()));
        }
        versions.push((instance.get_name().to_string(), command));
    }
    runner.run_hyperfine(out, VERSION_SET, &distinct)?;
    for table in TABLES {
        let commands: Vec<(String, String)> = instances
            .iter()
            .map(|instance| build_command(instance, &target, &locations.corpus.extensions, table))
            .collect::<Result<_, _>>()?;
        print_line(out, "")?;
        runner.run_hyperfine(out, &get_floor_set_name(table), &commands)?;
    }
    let (measurements, skipped) = collect_measurements(scratch.get_path(), &runner.commands, &[])?;
    for message in &skipped {
        print_warning(out, message)?;
    }
    print_line(out, "")?;
    for line in format_floor(&measurements, &versions) {
        print_line(out, &line)?;
    }
    if runner.failures.is_empty() {
        return Ok(0);
    }
    print_warning(
        out,
        &format!("hyperfine failed on {}", runner.failures.join(", ")),
    )?;
    Ok(1)
}

struct RunContext<'a> {
    options: &'a Options,
    locations: &'a Locations,
    instances: &'a [Instance],
    control: usize,
    platform: Platform,
    defender: DefenderState,
    unequal: Option<String>,
    prepared: Vec<String>,
    settings: Settings,
    stamp: &'a str,
    now: u64,
    res: &'a Path,
    is_local: bool,
    left_out: Vec<String>,
    collected: Collected,
    against: Option<String>,
}

struct Scratch(PathBuf);

impl Scratch {
    fn create(command: &str) -> Result<Scratch, String> {
        let dir = env::temp_dir().join(format!(
            "linebench-{command}-{}",
            format_utc_stamp(read_seconds_since_epoch())
        ));
        fs::create_dir_all(dir.join(OUT_DIR))
            .map_err(|error| format!("{} could not be created: {error}", dir.display()))?;
        Ok(Scratch(dir))
    }

    fn get_path(&self) -> &Path {
        &self.0
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn measure_and_record(out: &mut dyn Write, context: RunContext) -> Result<i32, String> {
    let RunContext {
        options,
        locations,
        instances,
        control,
        platform,
        defender,
        unequal,
        prepared,
        settings,
        stamp,
        now,
        res,
        is_local,
        left_out,
        mut collected,
        against,
    } = context;
    print_header(out, "== phase 0: machine state")?;
    let background = sample_background_busy(platform);
    print_line(out, &format!("   background: {}", format_busy(background)))?;
    let machine = collect_machine(platform, &locations.checkout);
    if let Ok(Value::Object(fields)) = serde_json::to_value(&machine) {
        for (key, value) in fields {
            print_line(
                out,
                &format!(
                    "   {key}: {}",
                    value.as_str().map_or(value.to_string(), str::to_string)
                ),
            )?;
        }
    }
    for instance in instances {
        let with = if instance.args.is_empty() {
            String::new()
        } else {
            format!(" with {}", instance.args.join(" "))
        };
        print_line(
            out,
            &format!(
                "   {}: {} ({}){with}",
                instance.get_name(),
                instance.identity.version,
                instance.identity.describe_origin()
            ),
        )?;
    }
    let extensions = &locations.corpus.extensions;
    let mut runner = Runner::new(
        res,
        settings,
        platform,
        collect_scrub(instances),
        get_report_style(),
    );
    capture_json_outputs(out, &mut runner, instances, &locations.checkout, extensions)?;
    let identity_checks = compare_captures(out, res, instances, &options.expect_identical)?;
    run_phases(
        out,
        &mut runner,
        instances,
        control,
        &locations.checkout,
        extensions,
    )?;

    print_header(out, "== summary")?;
    let mut counts = Vec::new();
    for table in TABLES {
        for instance in instances {
            let path = res.join(OUT_DIR).join(format!(
                "{}.json",
                get_capture_name(table, instance.get_name())
            ));
            let text = fs::read_to_string(&path).unwrap_or_default();
            match read_counts(&instance.definition, &text) {
                Ok(c) => counts.push(CountRecord::of(table, instance.get_name(), &c)),
                Err(refused) => print_warning(
                    out,
                    &format!(
                        "no counts for {} {}: {refused}",
                        instance.get_name(),
                        table.as_str()
                    ),
                )?,
            }
        }
    }
    for what in describe_empty_bare_counts(&counts) {
        print_warning(
            out,
            &format!("{what} out of the box, so that bare time is of a run that did no work"),
        )?;
    }
    let (measurements, skipped) = collect_measurements(res, &runner.commands, &counts)?;
    for message in skipped {
        print_warning(out, &message)?;
    }
    let counted: Vec<Counted> = counts
        .iter()
        .filter(|c| c.set == Table::SameWork.as_str())
        .map(|c| Counted {
            instance: c.instance.clone(),
            files: c.files,
            lines: c.lines,
        })
        .collect();
    let names: Vec<String> = instances.iter().map(|i| i.get_name().to_string()).collect();
    let parity = judge_parity(
        locations.corpus.files,
        &counted,
        &names,
        locations.corpus.tolerance,
    );
    let git = read_git_state(&locations.checkout);
    let record = Record {
        format: RECORD_FORMAT,
        stamp: stamp.to_string(),
        date: format_utc_date(now),
        machine,
        defender,
        background_busy_percent: background,
        prepared,
        corpus: CorpusRecord {
            name: locations.corpus.name.clone(),
            checkout: locations.checkout.clone(),
            commit: locations.corpus.commit.clone(),
            pinned: locations.corpus.is_pinned()
                && git.head.as_deref() == Some(locations.corpus.commit.as_str()),
            head: git.head,
            clean: git.clean,
            extensions: locations.corpus.extensions.clone(),
        },
        settings: RunSettings {
            warmup: settings.warmup,
            runs: settings.runs,
            settle: settings.settle,
            instances: names,
            control: instances[control].get_name().to_string(),
            unequal_exclusions: unequal,
            skipped: left_out,
        },
        instances: instances
            .iter()
            .map(|i| InstanceRecord::of(i, extensions))
            .collect::<Result<_, _>>()?,
        counts,
        measurements,
        parity: Some(parity),
        hyperfine_failures: runner.failures.clone(),
        hyperfine_warnings: runner.warnings.clone(),
        capture_failures: runner.capture_failures.clone(),
        identity_checks,
    };
    let written = write_record(res, &record)
        .and_then(|_| write_notes(res, &record))
        .and_then(|_| write_csvs(res, &record));
    if let Err(refused) = written {
        print_warning(out, &format!("the summary could not be written: {refused}"))?;
        print_line(
            out,
            &format!(
                "         the raw output is kept in {} and the hyperfine exports stand",
                res.join(OUT_DIR).display()
            ),
        )?;
        return Ok(1);
    }
    print_line(
        out,
        &format!(
            "   drift         {}",
            calculate_drift(&record.measurements).map_or("n/a".to_string(), |d| d.to_string())
        ),
    )?;
    for line in format_summary_tables(&record.measurements) {
        print_line(out, &paint_table_line(&line))?;
    }
    if let Some(parity) = &record.parity {
        print_line(out, "")?;
        print_parity(out, parity, "equal work    ")?;
    }
    for message in &collected.skipped {
        print_warning(out, message)?;
    }
    let relative = res
        .strip_prefix(&locations.out)
        .unwrap_or(res)
        .to_string_lossy()
        .replace('\\', "/");
    collected.found.push(FoundRun {
        record: record.clone(),
        relative,
    });
    collected
        .found
        .sort_by(|a, b| b.record.stamp.cmp(&a.record.stamp));
    if let Err(refused) = write_results_page(&locations.out, &collected.found) {
        print_warning(
            out,
            &format!("the results page could not be written: {refused}"),
        )?;
    }
    let earlier: Vec<&Record> = collected
        .found
        .iter()
        .filter(|f| is_local || !f.is_local())
        .map(|f| &f.record)
        .filter(|r| r.stamp != record.stamp)
        .collect();
    let since = format_since(&record, &earlier);
    print_line(out, "")?;
    for line in &since {
        print_line(out, &format!("   {line}"))?;
    }
    if let Err(refused) = append_to_notes(res, &since) {
        print_warning(out, &refused)?;
    }
    if let Some(named) = against
        .as_deref()
        .and_then(|stamp| collected.found.iter().find(|f| f.record.stamp == stamp))
    {
        let against = format_against(&record, &named.record, &earlier);
        print_line(out, "")?;
        for line in &against {
            print_line(out, &format!("   {line}"))?;
        }
        if let Err(refused) = append_to_notes(res, &against) {
            print_warning(out, &refused)?;
        }
    }
    if !runner.warnings.is_empty() {
        print_line(out, "")?;
        for text in &runner.warnings {
            let (name, message) = text.split_once(": ").unwrap_or((text, ""));
            print_warning(
                out,
                &format!(
                    "{}, on {name}",
                    message
                        .split(". ")
                        .next()
                        .unwrap_or(message)
                        .trim_end_matches('.')
                ),
            )?;
        }
    }
    if options.keep_raw {
        capture_plain_output(out, &mut runner, instances, &locations.checkout, extensions)?;
    } else {
        let _ = fs::remove_dir_all(res.join(OUT_DIR));
    }
    print_line(out, "")?;
    print_line(
        out,
        &format!(
            "done. everything is in {}{}",
            res.display(),
            if is_local {
                " (a local build, kept out of the published runs)"
            } else {
                ""
            }
        ),
    )?;
    if !runner.failures.is_empty() {
        print_warning(
            out,
            &format!("hyperfine had trouble with: {}", runner.failures.join(", ")),
        )?;
    }
    if !runner.capture_failures.is_empty() {
        print_warning(
            out,
            &format!(
                "counts missing or partial: {}",
                runner.capture_failures.join("; ")
            ),
        )?;
    }
    let differing: Vec<&String> = record
        .identity_checks
        .iter()
        .filter(|line| line.starts_with(DIFFER))
        .collect();
    if differing.is_empty() {
        return Ok(0);
    }
    for line in differing {
        print_warning(out, &format!("expected identical, and they {line}"))?;
    }
    Ok(1)
}

fn check_identical_pairs(pairs: &[(String, String)], instances: &[Instance]) -> Result<(), String> {
    let find = |name: &str| {
        instances
            .iter()
            .find(|instance| instance.get_name() == name)
            .ok_or_else(|| {
                format!("--expect-identical names {name}, which is not an instance of this run")
            })
    };
    for (left, right) in pairs {
        let (a, b) = (find(left)?, find(right)?);
        if left == right {
            return Err(format!(
                "--expect-identical {left}={right} compares an instance with itself"
            ));
        }
        if a.identity.counter != b.identity.counter {
            return Err(format!(
                "--expect-identical {left}={right}: the two are different counters ({} and {}), \
                 so their documents cannot be identical",
                a.identity.counter, b.identity.counter
            ));
        }
    }
    Ok(())
}

fn compare_captures(
    out: &mut dyn Write,
    res: &Path,
    instances: &[Instance],
    pairs: &[(String, String)],
) -> Result<Vec<String>, String> {
    let find = |name: &str| {
        instances
            .iter()
            .find(|instance| instance.get_name() == name)
            .expect("checked before the run")
    };
    let read_capture = |table: Table, instance: &Instance| -> Result<String, String> {
        let path = res.join(OUT_DIR).join(format!(
            "{}.json",
            get_capture_name(table, instance.get_name())
        ));
        fs::read_to_string(&path)
            .map_err(|error| format!("{} could not be read: {error}", path.display()))
    };
    let mut lines = Vec::new();
    for (left, right) in pairs {
        let (a, b) = (find(left), find(right));
        let mut verdicts = Vec::new();
        let mut identical_everywhere = true;
        for table in TABLES {
            let named = table.describe().to_lowercase();
            let compared = read_capture(table, a).and_then(|left| {
                let right = read_capture(table, b)?;
                compare_documents(&a.definition, &left, &b.definition, &right)
            });
            let comparison = match compared {
                Ok(comparison) => comparison,
                Err(reason) => {
                    identical_everywhere = false;
                    verdicts.push(format!("{named} could not be compared: {reason}"));
                    continue;
                }
            };
            match comparison.first {
                None => verdicts.push(format!("{named}: {IDENTICAL}")),
                Some(first) => {
                    identical_everywhere = false;
                    let count = match comparison.differences {
                        1 => "1 difference".to_string(),
                        n => format!("{n} differences in all"),
                    };
                    verdicts.push(format!(
                        "{named}: first at {}, {} against {}, {count}",
                        first.path, first.left, first.right
                    ));
                }
            }
        }
        let line = if identical_everywhere {
            format!("{IDENTICAL}: {left} and {right}, same work and out of the box")
        } else {
            format!("{DIFFER}: {left} and {right}, {}", verdicts.join("; "))
        };
        let color = if identical_everywhere {
            Color::Green
        } else {
            Color::Red
        };
        print_line(out, &format!("   {}", paint(color, &line)))?;
        lines.push(line);
    }
    Ok(lines)
}

fn check_everything(
    out: &mut dyn Write,
    options: &Options,
    locations: &Locations,
    instances: &[Instance],
    control: usize,
    platform: Platform,
    scratch: &Path,
) -> Result<(Vec<String>, bool), String> {
    let mut runner = Runner::new(
        scratch,
        Settings {
            warmup: 0,
            runs: 2,
            settle: 0,
        },
        platform,
        collect_scrub(instances),
        Style::Hidden,
    );
    let mut bad = Vec::new();
    let mut counted = Vec::new();
    let extensions = &locations.corpus.extensions;
    for table in TABLES {
        for instance in instances {
            let label = format!("   {:<14} {}  ", instance.get_name(), table.as_str());
            let args = build_args(instance, &locations.checkout, extensions, table, true)?;
            let failures_before = runner.capture_failures.len();
            let capture = runner.capture_output(
                out,
                &get_capture_name(table, instance.get_name()),
                &instance.identity.binary,
                &args,
                true,
            )?;
            if runner.capture_failures.len() > failures_before {
                print_line(out, &format!("{label}{}", paint(Color::Red, "FAILED")))?;
                bad.push(format!("{} {}", instance.get_name(), table.as_str()));
                continue;
            }
            let text = fs::read_to_string(&capture.path).unwrap_or_default();
            match read_counts(&instance.definition, &text) {
                Ok(counts) => {
                    let empty = describe_empty_count(counts.files, counts.lines);
                    match &empty {
                        Some(what) => {
                            print_line(out, &format!("{label}{}", paint(Color::Red, what)))?;
                        }
                        None => print_line(
                            out,
                            &format!(
                                "{label}{}   {:>10} files  {:>14} lines",
                                paint(Color::Green, "ok"),
                                format_thousands(counts.files),
                                format_thousands(counts.lines)
                            ),
                        )?,
                    }
                    if table == Table::SameWork {
                        counted.push(Counted {
                            instance: instance.get_name().to_string(),
                            files: counts.files,
                            lines: counts.lines,
                        });
                    } else if empty.is_some() {
                        bad.push(format!("{} {}", instance.get_name(), table.as_str()));
                    }
                    match find_absent_volatile_paths(&instance.definition, &text) {
                        Ok(absent) => {
                            for path in absent {
                                print_line(
                                    out,
                                    &format!(
                                        "         {}",
                                        paint(
                                            Color::Yellow,
                                            &format!(
                                                "volatile {path} sits nowhere in what {} printed",
                                                instance.get_name()
                                            )
                                        )
                                    ),
                                )?;
                            }
                        }
                        Err(refused) => {
                            print_line(out, &format!("         {}", paint(Color::Red, &refused)))?;
                            bad.push(format!("{} volatile", instance.get_name()));
                        }
                    }
                }
                Err(refused) => {
                    print_line(
                        out,
                        &format!(
                            "{label}{}",
                            paint(
                                Color::Red,
                                &format!("ran, but no counts could be read: {refused}")
                            )
                        ),
                    )?;
                    let shown =
                        print_first_lines(out, &text)? + print_first_lines(out, &capture.stderr)?;
                    if shown == 0 {
                        print_line(out, "         it printed nothing")?;
                    }
                    bad.push(format!("{} {}", instance.get_name(), table.as_str()));
                }
            }
        }
    }
    print_line(out, "")?;
    let bare = build_command(
        &instances[control],
        &locations.checkout,
        extensions,
        Table::OutOfTheBox,
    )?;
    let failures_before = runner.failures.len();
    runner.run_hyperfine(out, "check", std::slice::from_ref(&bare))?;
    if runner.failures.len() > failures_before {
        print_line(
            out,
            &format!("   hyperfine   {}", paint(Color::Red, "FAILED")),
        )?;
        bad.push("hyperfine".to_string());
    } else {
        print_line(
            out,
            &format!("   hyperfine   {}", paint(Color::Green, "ok")),
        )?;
    }
    match capture_with_status("git", &["--version"]) {
        Ok((true, version)) => print_line(
            out,
            &format!(
                "   git         {}   {}",
                paint(Color::Green, "ok"),
                shorten_version(&version)
            ),
        )?,
        _ => {
            print_line(
                out,
                &format!("   git         {}", paint(Color::Red, "FAILED")),
            )?;
            bad.push("git".to_string());
        }
    }
    let state = read_defender_state(
        platform,
        is_privileged(platform),
        &locations.checkout,
        &collect_binaries(instances, platform)?,
    );
    if platform == Platform::Windows {
        match find_unequal_exclusions(&state) {
            Some(unequal) if options.allow_unequal => print_line(
                out,
                &format!(
                    "   MS Defender {}",
                    paint(Color::Yellow, &format!("unequal: {unequal}"))
                ),
            )?,
            Some(unequal) => {
                print_line(
                    out,
                    &format!(
                        "   MS Defender {}",
                        paint(Color::Red, &format!("unequal: {unequal}"))
                    ),
                )?;
                bad.push("defender".to_string());
            }
            None => print_line(
                out,
                &format!(
                    "   MS Defender {}, {}",
                    paint(Color::Green, "ok"),
                    describe_exclusions(&state)
                ),
            )?,
        }
        print_line(
            out,
            &format!(
                "   corpus      {}",
                match state.corpus_excluded.as_str() {
                    "yes" => "under a Defender exclusion path".to_string(),
                    other => paint(
                        Color::Yellow,
                        &format!("not under any Defender exclusion path ({other})")
                    )
                    .to_string(),
                }
            ),
        )?;
    }
    let counted_by_git = match (locations.corpus.is_pinned(), locations.corpus.files) {
        (true, None) => Some(count_tracked_files(
            &locations.checkout,
            &locations.corpus.extensions,
        )?),
        _ => None,
    };
    let reference = locations.corpus.files.or(counted_by_git);
    let expected: Vec<String> = instances.iter().map(|i| i.get_name().to_string()).collect();
    let parity = judge_parity(reference, &counted, &expected, locations.corpus.tolerance);
    print_line(out, "")?;
    let files: Vec<String> = counted
        .iter()
        .map(|c| format!("{} {}", c.instance, format_thousands(c.files)))
        .collect();
    print_line(
        out,
        &format!(
            "   files   {}{}",
            reference.map_or("corpus none   ".to_string(), |r| format!(
                "corpus {}   ",
                format_thousands(r)
            )),
            files.join("   ")
        ),
    )?;
    let lines: Vec<String> = counted
        .iter()
        .map(|c| format!("{} {}", c.instance, format_thousands(c.lines)))
        .collect();
    print_line(out, &format!("   lines   {}", lines.join("   ")))?;
    print_parity(out, &parity, "")?;
    if let Some(files) = counted_by_git {
        print_line(
            out,
            &format!(
                "   files = {files}   counted from the tree of this commit; write it into {} \
                 beside the commit, and run will then accept the definition",
                locations.corpus.path.display()
            ),
        )?;
    }
    if !parity.problems.is_empty() {
        bad.push("equal work".to_string());
    }
    Ok((bad, !parity.compared))
}

fn time_the_control(
    out: &mut dyn Write,
    locations: &Locations,
    instances: &[Instance],
    control: usize,
    platform: Platform,
    scratch: &Path,
) -> Result<Option<Value>, String> {
    let mut runner = Runner::new(
        scratch,
        Settings {
            warmup: 0,
            runs: NOISE_RUNS,
            settle: 0,
        },
        platform,
        collect_scrub(instances),
        Style::Hidden,
    );
    let bare = build_command(
        &instances[control],
        &locations.checkout,
        &locations.corpus.extensions,
        Table::OutOfTheBox,
    )?;
    runner.run_hyperfine(out, "noise", std::slice::from_ref(&bare))?;
    Ok(fs::read_to_string(scratch.join("noise.json"))
        .ok()
        .and_then(|text| serde_json::from_str::<Value>(&text).ok()))
}

fn create_empty_repository(scratch: &Path) -> Result<PathBuf, String> {
    let target = scratch.join(FLOOR_TARGET);
    let path = target.to_string_lossy().into_owned();
    let (made, _) = capture_with_status("git", &["init", "-q", &path])
        .map_err(|unfinished| unfinished.describe("git init"))?;
    if !made {
        return Err(format!(
            "git init could not make the repository the floor is measured over, in {}",
            target.display()
        ));
    }
    Ok(target)
}

fn print_newest_releases(out: &mut dyn Write, lookups: &[Lookup]) -> Result<(), String> {
    if lookups.is_empty() {
        return Ok(());
    }
    print_line(out, "")?;
    print_line(out, ">> releases")?;
    for lookup in lookups {
        let Some(how) = &lookup.definition.acquisition else {
            continue;
        };
        let text = match &lookup.outcome {
            Ok(newest) => {
                let mut text = describe_newest(
                    &lookup.definition.name,
                    how,
                    lookup.definition.shipped_version.as_deref(),
                    newest,
                );
                if let Some(hours) = lookup.age_seconds.map(|age| age / 3600).filter(|h| *h > 0) {
                    text.push_str(&format!(" (looked up {hours} h ago)"));
                }
                match judge_newest(how, newest) {
                    Standing::Newer | Standing::Differs => paint(Color::Yellow, &text).to_string(),
                    Standing::Same | Standing::Behind => text,
                }
            }
            Err(reason) => {
                let mut text = format!("the newest release could not be looked up: {reason}");
                if let Some(origin) =
                    describe_pin_origin(how, lookup.definition.shipped_version.as_deref())
                {
                    text.push_str(&format!(" ({origin})"));
                }
                paint(Color::Yellow, &text).to_string()
            }
        };
        print_line(out, &format!("   {:<12}{text}", lookup.definition.name))?;
    }
    Ok(())
}

fn print_first_lines(out: &mut dyn Write, text: &str) -> Result<usize, String> {
    let mut lines = text.lines().filter(|line| !line.trim().is_empty());
    let mut shown = 0;
    for line in lines.by_ref().take(UNREADABLE_OUTPUT_LINES) {
        print_line(out, &format!("         {line}"))?;
        shown += 1;
    }
    if lines.next().is_some() {
        print_line(out, "         ...")?;
    }
    Ok(shown)
}

fn print_parity(out: &mut dyn Write, parity: &Parity, lead: &str) -> Result<(), String> {
    let verdict = parity.judge();
    match verdict {
        Verdict::NotEqual {
            problems,
            nothing_compared,
        } => {
            for problem in problems {
                print_line(out, &format!("   {lead}{}", paint(Color::Yellow, problem)))?;
            }
            if let Some(why) = nothing_compared {
                print_line(out, &format!("   {lead}{why}"))?;
            }
        }
        Verdict::NotCompared(why) => print_line(out, &format!("   {lead}{why}"))?,
        Verdict::Equal { .. } => {}
    }
    if let Some(equal) = verdict.describe_equality(parity.tolerance) {
        print_line(out, &format!("   {lead}{}", paint(Color::Green, &equal)))?;
    }
    if !parity.missing.is_empty() {
        print_line(
            out,
            &format!(
                "   {lead}{}",
                paint(
                    Color::Yellow,
                    &format!(
                        "{} gave no readable counts and is not in this check",
                        parity.missing.join(", ")
                    )
                )
            ),
        )?;
    }
    Ok(())
}

fn collect_scrub(instances: &[Instance]) -> Vec<String> {
    let mut scrub: Vec<String> = instances
        .iter()
        .flat_map(|i| i.definition.run.scrub_env.clone())
        .collect();
    scrub.sort();
    scrub.dedup();
    scrub
}

fn collect_binaries(
    instances: &[Instance],
    platform: Platform,
) -> Result<Vec<CounterBinary>, String> {
    instances
        .iter()
        .map(|i| {
            Ok(CounterBinary {
                name: i.get_name().to_string(),
                path: i.identity.binary.clone(),
                process: i.definition.get_process_name(platform.as_system())?,
            })
        })
        .collect()
}

fn describe_exclusions(state: &DefenderState) -> String {
    match judge_process_exclusions(state) {
        ProcessExclusions::NoCounters => "no counters".to_string(),
        ProcessExclusions::AllExcluded => "every counter excluded".to_string(),
        ProcessExclusions::NoneExcluded => paint(Color::Yellow, "none excluded").to_string(),
        ProcessExclusions::Unknown(answer) => {
            paint(Color::Yellow, &format!("exclusions {}", answer.as_str())).to_string()
        }
        ProcessExclusions::Unequal => "mixed".to_string(),
    }
}

fn paint_table_line(line: &str) -> String {
    let trimmed = line.trim_start();
    if trimmed.starts_with("instance") {
        return format!("   {}", paint(Color::Blue, trimmed));
    }
    if trimmed == "Same work" || trimmed == "Out of the box" {
        return format!("   {}", paint(Color::Bold, trimmed));
    }
    line.to_string()
}

fn find_step(value: f64, steps: &[f64; 3]) -> usize {
    steps.iter().filter(|step| value >= **step).count()
}

fn paint_by_steps(value: f64, steps: &[f64; 3], text: &str) -> String {
    paint_step(find_step(value, steps), text)
}

fn paint_step(step: usize, text: &str) -> String {
    match step {
        0 => paint(Color::Green, text).to_string(),
        1 => paint(Color::Yellow, text).to_string(),
        2 => paint(Color::Orange, text).to_string(),
        _ => paint(Color::Red, text).to_string(),
    }
}
