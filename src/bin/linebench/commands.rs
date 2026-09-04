use std::env;
use std::fs;
use std::io::Write;
use std::path::Path;

use serde_json::Value;

use linebench::corpus::{
    Counted, check_commit, count_reference_files, format_percent, judge_parity, read_git_state,
    setup_corpus,
};
use linebench::counters::Definition;
use linebench::defender::{
    CounterBinary, DefenderState, explain_unequal_exclusions, find_unequal_exclusions,
    read_defender_state,
};
use linebench::fetch::{fetch_counter, read_manifest};
use linebench::machine::{
    Platform, collect_machine, detect_arch, plan_prep, sample_background_busy,
};
use linebench::measure::{
    Instance, OUT_DIR, Runner, Settings, TABLES, Table, build_command, run_phases,
};
use linebench::os::is_privileged;
use linebench::read::read_counts;
use linebench::record::{
    CorpusRecord, CountRecord, InstanceRecord, RECORD_FORMAT, Record, RunSettings, calculate_drift,
    collect_measurements, format_summary_tables, format_thousands, format_utc_date,
    format_utc_stamp, read_seconds_since_epoch, write_csvs, write_notes, write_record,
};

use crate::config::{Locations, Options};
use crate::instances::build_instances;
use crate::output::{Color, Output, paint, print_header, print_line, print_warning};
use crate::page::{LOCAL_DIR, collect_records, format_since, write_results_page};
use crate::prep;

const TRANSCRIPT_FILE: &str = "transcript.txt";
const NOISE_RUNS: u32 = 5;
const BACKGROUND_CORE_STEPS: [f64; 3] = [0.75, 1.5, 3.0];
const SPREAD_STEPS: [f64; 3] = [5.0, 10.0, 15.0];
const VERDICTS: [&str; 4] = [
    "steady",
    "relatively steady",
    "somewhat unsteady",
    "not steady",
];
const COLD_CACHE_RATIO: f64 = 1.5;

pub fn run_setup(
    out: &mut dyn Write,
    options: &Options,
    locations: &Locations,
    definitions: &[Definition],
    platform: Platform,
) -> Result<i32, String> {
    if is_privileged(platform) {
        return Err(format!(
            "setup writes files as you, so it does not run as {}; run it from an ordinary terminal",
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
        if let Err(refused) = fetch_counter(
            out,
            definition,
            platform,
            &arch,
            &locations.counters_dir,
            &mut manifest,
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
    let privileged = is_privileged(platform);
    prep::restore_after_interrupted_run(out, &locations.counters_dir, privileged, platform)?;
    check_commit(&locations.corpus, &locations.checkout)?;
    let (instances, control) = build_instances(definitions, locations, options, platform)?;
    print_header(
        out,
        &format!(
            "== check: {} at {}",
            locations.corpus.name,
            locations.checkout.display()
        ),
    )?;
    let scratch = env::temp_dir().join(format!(
        "linebench-check-{}",
        format_utc_stamp(read_seconds_since_epoch())
    ));
    fs::create_dir_all(scratch.join(OUT_DIR))
        .map_err(|error| format!("{} could not be created: {error}", scratch.display()))?;
    let mut runner = Runner::new(
        &scratch,
        Settings {
            warmup: 0,
            runs: 2,
            settle: 0,
        },
        platform,
        collect_scrub(&instances),
    );
    let mut bad = Vec::new();
    let mut counted = Vec::new();
    let extensions = &locations.corpus.extensions;
    for table in TABLES {
        for instance in &instances {
            let label = format!("   {:<14} {}  ", instance.name(), table.as_str());
            let args = instance.definition.build_args(
                &locations.checkout,
                extensions,
                table.uses_same_work(),
                true,
            )?;
            let failures_before = runner.failures.len();
            let path = runner.capture_output(
                out,
                &format!("{}-{}", table.as_str(), instance.name()),
                &instance.identity.binary,
                &args,
                true,
            )?;
            if runner.failures.len() > failures_before {
                print_line(out, &format!("{label}{}", paint(Color::Red, "FAILED")))?;
                bad.push(format!("{} {}", instance.name(), table.as_str()));
                continue;
            }
            let text = fs::read_to_string(&path).unwrap_or_default();
            match read_counts(&instance.definition, &text) {
                Ok(counts) if counts.files == 0 => {
                    print_line(
                        out,
                        &format!(
                            "{label}{}",
                            paint(
                                Color::Red,
                                &format!(
                                    "counted nothing at all, so its share of the {} definition names no language this tree has",
                                    locations.corpus.name
                                )
                            )
                        ),
                    )?;
                    bad.push(format!("{} {}", instance.name(), table.as_str()));
                }
                Ok(counts) => {
                    print_line(
                        out,
                        &format!(
                            "{label}{}   {:>10} files  {:>14} lines",
                            paint(Color::Green, "ok"),
                            format_thousands(counts.files),
                            format_thousands(counts.lines)
                        ),
                    )?;
                    if table == Table::SameWork {
                        counted.push(Counted {
                            instance: instance.name().to_string(),
                            files: counts.files,
                            lines: counts.lines,
                        });
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
                    bad.push(format!("{} {}", instance.name(), table.as_str()));
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
    let state = read_defender_state(
        platform,
        privileged,
        &locations.checkout,
        &collect_binaries(&instances, platform)?,
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
    let reference = count_reference_files(&locations.checkout, extensions);
    let parity = judge_parity(reference, &counted, locations.corpus.tolerance);
    print_line(out, "")?;
    let files: Vec<String> = counted
        .iter()
        .map(|c| format!("{} {}", c.instance, format_thousands(c.files)))
        .collect();
    print_line(
        out,
        &format!(
            "   files   {}{}",
            reference.map_or("git none   ".to_string(), |r| format!(
                "git {}   ",
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
    if parity.problems.is_empty() {
        print_line(
            out,
            &format!(
                "   {}",
                paint(
                    Color::Green,
                    &format!("within {}", format_percent(parity.tolerance))
                )
            ),
        )?;
    } else {
        for problem in &parity.problems {
            print_line(out, &format!("   {}", paint(Color::Yellow, problem)))?;
        }
    }
    let _ = fs::remove_dir_all(&scratch);
    print_line(out, "")?;
    if bad.is_empty() {
        print_line(out, &paint(Color::Green, "all good.").to_string())?;
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
    let privileged = is_privileged(platform);
    prep::restore_after_interrupted_run(out, &locations.counters_dir, privileged, platform)?;
    check_commit(&locations.corpus, &locations.checkout)?;
    let (instances, control) = build_instances(definitions, locations, options, platform)?;
    let cores = std::thread::available_parallelism().map_or(1, |c| c.get());
    print_header(out, "== noise")?;
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
    let scratch = env::temp_dir().join(format!(
        "linebench-noise-{}",
        format_utc_stamp(read_seconds_since_epoch())
    ));
    fs::create_dir_all(scratch.join(OUT_DIR))
        .map_err(|error| format!("{} could not be created: {error}", scratch.display()))?;
    let mut runner = Runner::new(
        &scratch,
        Settings {
            warmup: 0,
            runs: NOISE_RUNS,
            settle: 0,
        },
        platform,
        collect_scrub(&instances),
    );
    let bare = build_command(
        &instances[control],
        &locations.checkout,
        &locations.corpus.extensions,
        Table::OutOfTheBox,
    )?;
    runner.run_hyperfine(out, "noise", std::slice::from_ref(&bare))?;
    let export = fs::read_to_string(scratch.join("noise.json"))
        .ok()
        .and_then(|text| serde_json::from_str::<Value>(&text).ok());
    let _ = fs::remove_dir_all(&scratch);
    let Some(result) = export.as_ref().and_then(|v| v["results"].get(0)) else {
        print_line(out, "   workload      hyperfine failed")?;
        return Ok(1);
    };
    let times: Vec<f64> = result["times"]
        .as_array()
        .map(|a| a.iter().filter_map(Value::as_f64).collect())
        .unwrap_or_default();
    if times.len() < 2 {
        print_line(out, "   workload      hyperfine gave too few runs")?;
        return Ok(1);
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
            instances[control].name(),
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
    Ok(if worst >= 2 { 1 } else { 0 })
}

pub fn run_benchmark(
    out: &mut Output,
    options: &Options,
    locations: &Locations,
    definitions: &[Definition],
    platform: Platform,
) -> Result<i32, String> {
    let privileged = is_privileged(platform);
    prep::restore_after_interrupted_run(out, &locations.counters_dir, privileged, platform)?;
    check_commit(&locations.corpus, &locations.checkout)?;
    let (instances, control) = build_instances(definitions, locations, options, platform)?;
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
        prep::apply_prep(out, plan, &locations.counters_dir, platform)?
    } else {
        None
    };
    let prepared: Vec<String> = applied
        .as_ref()
        .map(|_| describe_plan(platform, options.no_prep))
        .unwrap_or_default();

    let settings = Settings {
        warmup: options.warmup.unwrap_or(Settings::default().warmup),
        runs: options.runs.unwrap_or(Settings::default().runs),
        settle: options.settle.unwrap_or(Settings::default().settle),
    };
    let now = read_seconds_since_epoch();
    let stamp = format_utc_stamp(now);
    let is_local = instances
        .iter()
        .any(|i| matches!(i.identity.origin, linebench::fetch::Origin::Given { .. }));
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
        defender: &defender,
        unequal,
        prepared,
        settings,
        stamp: &stamp,
        now,
        res: &res,
        is_local,
    };
    let outcome = measure_and_record(out, &context);
    out.stop_transcript();
    drop(applied);
    outcome
}

struct RunContext<'a> {
    options: &'a Options,
    locations: &'a Locations,
    instances: &'a [Instance],
    control: usize,
    platform: Platform,
    defender: &'a DefenderState,
    unequal: Option<String>,
    prepared: Vec<String>,
    settings: Settings,
    stamp: &'a str,
    now: u64,
    res: &'a Path,
    is_local: bool,
}

fn measure_and_record(out: &mut dyn Write, context: &RunContext) -> Result<i32, String> {
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
    } = context;
    let (control, platform, settings, now, is_local) =
        (*control, *platform, *settings, *now, *is_local);
    let instances: &[Instance] = instances;
    print_header(out, "== phase 0: machine state")?;
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
        print_line(
            out,
            &format!(
                "   {}: {} ({})",
                instance.name(),
                instance.identity.version,
                describe_origin(&instance.identity.origin)
            ),
        )?;
    }
    let background = sample_background_busy(platform);
    print_line(
        out,
        &format!(
            "   background: {}",
            background.map_or("not sampled".to_string(), |b| format!("{b}% busy"))
        ),
    )?;
    let extensions = &locations.corpus.extensions;
    let mut runner = Runner::new(res, settings, platform, collect_scrub(instances));
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
            let path =
                res.join(OUT_DIR)
                    .join(format!("{}-{}.json", table.as_str(), instance.name()));
            let text = fs::read_to_string(&path).unwrap_or_default();
            match read_counts(&instance.definition, &text) {
                Ok(c) => counts.push(CountRecord::of(table, instance.name(), &c)),
                Err(refused) => print_warning(
                    out,
                    &format!(
                        "no counts for {} {}: {refused}",
                        instance.name(),
                        table.as_str()
                    ),
                )?,
            }
        }
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
    let parity = judge_parity(
        count_reference_files(&locations.checkout, extensions),
        &counted,
        locations.corpus.tolerance,
    );
    let git = read_git_state(&locations.checkout, true);
    let record = Record {
        format: RECORD_FORMAT,
        stamp: stamp.to_string(),
        date: format_utc_date(now),
        machine,
        defender: (*defender).clone(),
        background_busy_percent: background,
        prepared: prepared.clone(),
        corpus: CorpusRecord {
            name: locations.corpus.name.clone(),
            checkout: locations.checkout.clone(),
            commit: locations.corpus.commit.clone(),
            pinned: locations.corpus.is_pinned()
                && git.head.as_deref() == Some(locations.corpus.commit.as_str()),
            head: git.head,
            clean: git.clean,
        },
        settings: RunSettings {
            warmup: settings.warmup,
            runs: settings.runs,
            settle: settings.settle,
            instances: instances.iter().map(|i| i.name().to_string()).collect(),
            control: instances[control].name().to_string(),
            unequal_exclusions: unequal.clone(),
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
    };
    let written = write_record(res, &record)
        .and_then(|_| write_notes(res, &record))
        .and_then(|_| write_csvs(res, &record))
        .and_then(|_| write_results_page(&locations.out));
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
        if parity.problems.is_empty() {
            print_line(
                out,
                &format!(
                    "   equal work    {}",
                    paint(
                        Color::Green,
                        &format!("within {}", format_percent(parity.tolerance))
                    )
                ),
            )?;
        } else {
            for problem in &parity.problems {
                print_line(
                    out,
                    &format!("   equal work    {}", paint(Color::Yellow, problem)),
                )?;
            }
        }
    }
    let earlier: Vec<Record> = collect_records(&locations.out)
        .into_iter()
        .map(|f| f.record)
        .filter(|r| r.stamp != record.stamp)
        .collect();
    let earlier: Vec<&Record> = earlier.iter().collect();
    print_line(out, "")?;
    for line in format_since(&record, &earlier) {
        print_line(out, &format!("   {line}"))?;
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
    if !options.keep_raw {
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
    Ok(0)
}

pub fn run_report(out: &mut dyn Write, locations: &Locations) -> Result<i32, String> {
    write_results_page(&locations.out)?;
    print_line(
        out,
        &format!(
            "wrote {}",
            locations.out.join(crate::page::PAGE_FILE).display()
        ),
    )?;
    Ok(0)
}

fn describe_plan(platform: Platform, no_prep: bool) -> Vec<String> {
    if no_prep {
        return Vec::new();
    }
    plan_prep(platform)
        .iter()
        .map(|step| step.describe())
        .collect()
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
                name: i.name().to_string(),
                path: i.identity.binary.clone(),
                process: i.definition.get_process_name(platform.as_system())?,
            })
        })
        .collect()
}

fn describe_exclusions(state: &DefenderState) -> String {
    let answers: Vec<&str> = state
        .counters
        .values()
        .map(|e| e.process.as_str())
        .collect();
    match answers.as_slice() {
        [first, rest @ ..] if rest.iter().all(|a| a == first) => match *first {
            "yes" => "every counter excluded".to_string(),
            "no" => paint(Color::Yellow, "none excluded").to_string(),
            other => paint(Color::Yellow, &format!("exclusions {other}")).to_string(),
        },
        _ => "mixed".to_string(),
    }
}

fn describe_origin(origin: &linebench::fetch::Origin) -> String {
    match origin {
        linebench::fetch::Origin::Fetched { source, .. } => format!("fetched, {source}"),
        linebench::fetch::Origin::Built { built_with, .. } => format!("built with {built_with}"),
        linebench::fetch::Origin::Given { label } => format!("local build {label}"),
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
