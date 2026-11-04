use std::ffi::OsString;
use std::io::{self, Write};
use std::path::Path;
use std::process::ExitCode;
use std::time::{SystemTime, UNIX_EPOCH};
use std::{env, fs};

use chrono::{DateTime, FixedOffset, NaiveDate};
use git_koi::behavior::{ActivityEnergy, Behavior, select_activity_energy, select_behavior};
use git_koi::coat::KoiBody;
use git_koi::collect::{
    GhCli, collect_account_snapshot, collect_public_activity, collect_repository_languages,
};
use git_koi::history::AccountActivitySnapshot;
use git_koi::koi::derive_koi_development;
use git_koi::motion::frame_count;
use git_koi::output::{encode_gif, encode_png, encode_preview_html};
use git_koi::render::{
    KoiFrame, PondLighting, render_koi, render_swim_frame_with_lighting, select_pond_lighting,
};

const USAGE: &str = "Usage: git-koi collect LOGIN [--output FILE]
       git-koi render SNAPSHOT --output-dir DIR [--body BODY] [--utc-offset OFFSET] [--random-lap]
       git-koi --help | --version
Collect LOGIN's public GitHub snapshot as JSON through `gh api`: public owned and
contributed repositories, and daily contributions from the account's creation
through yesterday (UTC), the last complete day.
Writes to standard output, or replaces FILE only after collection succeeds.
Requires an authenticated GitHub CLI; in GitHub Actions, set GH_TOKEN to github.token.
Render a collected SNAPSHOT into DIR: koi.gif swims the behavior chosen for its
observation date, koi.png is the still koi, and preview.html shows both on light and
dark backgrounds. BODY is white (the default), black, asagi, tea, gold, or platinum.
OFFSET is a signed HH:MM value such as +05:30; it defaults to +00:00 (UTC).
RANDOM-LAP chooses between Cruise and Dart for a moving loop on each render.
The files are replaced only after all of them render.
Errors go to stderr and exit 1; invalid usage exits 2.
Examples: git-koi collect octocat --output snapshot.json
          git-koi render snapshot.json --output-dir koi --body asagi --utc-offset +05:30";

/// Each body's name on the command line.
const BODY_NAMES: [(&str, KoiBody); 6] = [
    ("white", KoiBody::White),
    ("black", KoiBody::Black),
    ("asagi", KoiBody::Asagi),
    ("tea", KoiBody::Tea),
    ("gold", KoiBody::Gold),
    ("platinum", KoiBody::Platinum),
];

fn main() -> ExitCode {
    let program_arguments: Vec<OsString> = env::args_os().skip(1).collect();
    let argument_texts: Vec<Option<&str>> = program_arguments
        .iter()
        .map(|argument| argument.to_str())
        .collect();
    match argument_texts.as_slice() {
        [Some("--help")] | [Some("collect" | "render"), Some("--help")] => {
            println!("{USAGE}");
            ExitCode::SUCCESS
        }
        [Some("--version")] => {
            println!("git-koi {}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        [Some("collect"), Some(login)] if !login.starts_with('-') => run_collect(login, None),
        [Some("collect"), Some(login), Some("--output"), _] if !login.starts_with('-') => {
            run_collect(login, Some(Path::new(&program_arguments[3])))
        }
        [
            Some("render"),
            Some(snapshot_path),
            Some("--output-dir"),
            _,
            ..,
        ] if !snapshot_path.starts_with('-') => {
            let Some((koi_body, utc_offset, random_lap)) =
                parse_render_options(&argument_texts[4..])
            else {
                eprintln!("{USAGE}");
                return ExitCode::from(2);
            };
            run_render(
                Path::new(snapshot_path),
                Path::new(&program_arguments[3]),
                koi_body,
                utc_offset,
                random_lap,
            )
        }
        _ => {
            eprintln!("{USAGE}");
            ExitCode::from(2)
        }
    }
}

fn run_collect(login: &str, output_path: Option<&Path>) -> ExitCode {
    let today = current_utc_date();
    let collection_result = collect_account_snapshot(&GhCli, login, today)
        .and_then(|account_snapshot| {
            let mut public_activity = collect_public_activity(login, today)?;
            collect_repository_languages(&GhCli, &mut public_activity)?;
            Ok(AccountActivitySnapshot {
                account: account_snapshot,
                public_activity: Some(public_activity),
            })
        })
        .map_err(|collection_error| collection_error.to_string())
        .and_then(|account_snapshot| {
            serde_json::to_string_pretty(&account_snapshot)
                .map_err(|serialize_error| serialize_error.to_string())
        })
        .and_then(|snapshot_json| write_snapshot(&format!("{snapshot_json}\n"), output_path));
    match collection_result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error_message) => {
            eprintln!("git-koi: {error_message}");
            ExitCode::FAILURE
        }
    }
}

fn run_render(
    snapshot_path: &Path,
    output_directory: &Path,
    koi_body: KoiBody,
    utc_offset: FixedOffset,
    random_lap: bool,
) -> ExitCode {
    let render_result =
        render_day(snapshot_path, koi_body, utc_offset, random_lap).and_then(|rendered_day| {
            fs::create_dir_all(output_directory).map_err(|create_error| {
                format!("{}: {create_error}", output_directory.display())
            })?;
            let mut written_paths = Vec::new();
            for (file_name, file_bytes) in rendered_day.output_files {
                let output_path = output_directory.join(file_name);
                replace_file(&output_path, &file_bytes)?;
                written_paths.push(output_path.display().to_string());
            }
            println!(
                "{}: wrote {}",
                rendered_day.caption,
                written_paths.join(", ")
            );
            Ok(())
        });
    match render_result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error_message) => {
            eprintln!("git-koi: {error_message}");
            ExitCode::FAILURE
        }
    }
}

/// The day's files, by name, with a caption naming the behavior, observation date, and body.
struct RenderedDay {
    caption: String,
    output_files: [(&'static str, Vec<u8>); 3],
}

/// Render and encode the day's files in memory, so none is written unless all succeed.
fn render_day(
    snapshot_path: &Path,
    koi_body: KoiBody,
    utc_offset: FixedOffset,
    random_lap: bool,
) -> Result<RenderedDay, String> {
    let snapshot_json = fs::read_to_string(snapshot_path)
        .map_err(|read_error| format!("{}: {read_error}", snapshot_path.display()))?;
    let account_snapshot: AccountActivitySnapshot = serde_json::from_str(&snapshot_json)
        .map_err(|parse_error| format!("{}: {parse_error}", snapshot_path.display()))?;
    let koi_development = derive_koi_development(&account_snapshot.account)
        .map_err(|development_error| development_error.to_string())?;
    let behavior = if random_lap {
        select_random_lap_behavior()
    } else {
        select_behavior(&koi_development)
    };
    let activity_energy = select_activity_energy(&koi_development.recent_activity);
    let public_activity = account_snapshot
        .public_activity
        .as_deref()
        .unwrap_or_default();
    let pond_lighting = select_pond_lighting(public_activity, utc_offset);
    let swim_frames: Vec<KoiFrame> = (0..frame_count(behavior))
        .map(|frame_index| {
            render_swim_frame_with_lighting(
                &koi_development,
                koi_body,
                behavior,
                activity_energy,
                public_activity,
                pond_lighting,
                frame_index,
            )
        })
        .collect();
    let gif_bytes =
        encode_gif(&swim_frames).map_err(|encoding_error| encoding_error.to_string())?;
    let png_bytes = encode_png(&render_koi(&koi_development, koi_body))
        .map_err(|encoding_error| encoding_error.to_string())?;
    let caption = format!(
        "{} swim, {} pace, {} pond for {}, {} body",
        describe_behavior(behavior),
        describe_activity_energy(activity_energy),
        describe_pond_lighting(pond_lighting),
        koi_development.observed_on,
        name_body(koi_body)
    );
    let preview_html = encode_preview_html(&caption);
    Ok(RenderedDay {
        caption,
        output_files: [
            ("koi.gif", gif_bytes),
            ("koi.png", png_bytes),
            ("preview.html", preview_html.into_bytes()),
        ],
    })
}

fn select_random_lap_behavior() -> Behavior {
    let time_bits = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos() as u64;
    if (time_bits ^ time_bits.rotate_left(17) ^ u64::from(std::process::id())) & 1 == 0 {
        Behavior::Cruise
    } else {
        Behavior::Dart
    }
}

fn parse_body(body_name: &str) -> Option<KoiBody> {
    BODY_NAMES
        .iter()
        .find(|(named_body_name, _)| *named_body_name == body_name)
        .map(|(_, koi_body)| *koi_body)
}

fn parse_render_options(arguments: &[Option<&str>]) -> Option<(KoiBody, FixedOffset, bool)> {
    let mut koi_body = KoiBody::default();
    let mut utc_offset = FixedOffset::east_opt(0)?;
    let mut seen_body = false;
    let mut seen_utc_offset = false;
    let mut random_lap = false;
    let mut argument_index = 0;
    while argument_index < arguments.len() {
        match arguments[argument_index] {
            Some("--body") if !seen_body => {
                koi_body = parse_body(arguments.get(argument_index + 1).copied()??)?;
                seen_body = true;
                argument_index += 2;
            }
            Some("--utc-offset") if !seen_utc_offset => {
                utc_offset = parse_utc_offset(arguments.get(argument_index + 1).copied()??)?;
                seen_utc_offset = true;
                argument_index += 2;
            }
            Some("--random-lap") if !random_lap => {
                random_lap = true;
                argument_index += 1;
            }
            _ => return None,
        }
    }
    Some((koi_body, utc_offset, random_lap))
}

fn parse_utc_offset(offset_text: &str) -> Option<FixedOffset> {
    let offset_bytes = offset_text.as_bytes();
    if offset_bytes.len() != 6
        || !matches!(offset_bytes[0], b'+' | b'-')
        || offset_bytes[3] != b':'
        || !offset_bytes[1..3]
            .iter()
            .chain(&offset_bytes[4..6])
            .all(u8::is_ascii_digit)
    {
        return None;
    }
    let hours = i32::from(offset_text[1..3].parse::<u8>().ok()?);
    let minutes = i32::from(offset_text[4..6].parse::<u8>().ok()?);
    if hours > 23 || minutes > 59 {
        return None;
    }
    let direction = if offset_bytes[0] == b'-' { -1 } else { 1 };
    FixedOffset::east_opt(direction * (hours * 3600 + minutes * 60))
}

fn name_body(koi_body: KoiBody) -> &'static str {
    BODY_NAMES
        .iter()
        .find(|(_, named_body)| *named_body == koi_body)
        .map(|(body_name, _)| *body_name)
        .expect("every body has a name")
}

fn describe_behavior(behavior: Behavior) -> &'static str {
    match behavior {
        Behavior::DeepRest => "deep rest",
        Behavior::Rest => "rest",
        Behavior::SurfaceFeeding => "surface feeding",
        Behavior::Dart => "dart",
        Behavior::Cruise => "cruise",
    }
}

fn describe_activity_energy(activity_energy: ActivityEnergy) -> &'static str {
    match activity_energy {
        ActivityEnergy::Calm => "calm",
        ActivityEnergy::Steady => "steady",
        ActivityEnergy::Lively => "lively",
        ActivityEnergy::Surge => "surge",
    }
}

fn describe_pond_lighting(pond_lighting: PondLighting) -> &'static str {
    match pond_lighting {
        PondLighting::Day => "day-lit",
        PondLighting::Dusk => "dusk-lit",
        PondLighting::Night => "night-lit",
    }
}

// GitHub dates its contribution calendar in UTC.
fn current_utc_date() -> NaiveDate {
    let seconds_since_epoch = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("the system clock is after 1970")
        .as_secs();
    DateTime::from_timestamp(
        i64::try_from(seconds_since_epoch).expect("the system clock is in range"),
        0,
    )
    .expect("the system clock is in range")
    .date_naive()
}

/// Write to standard output, or replace the file.
fn write_snapshot(snapshot_json: &str, output_path: Option<&Path>) -> Result<(), String> {
    let Some(output_path) = output_path else {
        return io::stdout()
            .write_all(snapshot_json.as_bytes())
            .map_err(|write_error| format!("standard output: {write_error}"));
    };
    replace_file(output_path, snapshot_json.as_bytes())
}

/// Replace the file through a sibling temporary file named after it, so a failed write
/// never leaves a partial file in place.
fn replace_file(output_path: &Path, file_bytes: &[u8]) -> Result<(), String> {
    let mut temporary_name = output_path.file_name().unwrap_or_default().to_owned();
    temporary_name.push(".tmp");
    let temporary_path = output_path.with_file_name(temporary_name);
    fs::write(&temporary_path, file_bytes)
        .and_then(|()| fs::rename(&temporary_path, output_path))
        .map_err(|write_error| {
            let _ = fs::remove_file(&temporary_path);
            format!("{}: {write_error}", output_path.display())
        })
}
