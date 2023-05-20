use chrono::{SecondsFormat, TimeZone, Utc};
use macos_unifiedlogs::dsc::SharedCacheStrings;
use macos_unifiedlogs::parser::{
    build_log, collect_shared_strings, collect_shared_strings_system, collect_strings,
    collect_strings_system, collect_timesync, collect_timesync_system, parse_log,
};
use macos_unifiedlogs::timesync::TimesyncBoot;
use macos_unifiedlogs::unified_log::{LogData, UnifiedLogData};
use macos_unifiedlogs::uuidtext::UUIDText;
use std::error::Error;
use std::fs;
use std::fs::OpenOptions;
use std::path::PathBuf;
use crate::args::LogFilter;


// Parse a provided directory path. Currently expect the path to follow macOS log collect structure
pub fn parse_log_archive(path: PathBuf, out: PathBuf, filter: LogFilter) {
    let mut archive_path = path.clone();

    // Parse all UUID files which contain strings and other metadata
    let string_results = collect_strings(&archive_path.display().to_string()).unwrap();

    archive_path.push("dsc");
    // Parse UUID cache files which also contain strings and other metadata
    let shared_strings_results =
        collect_shared_strings(&archive_path.display().to_string()).unwrap();
    archive_path.pop();

    archive_path.push("timesync");
    // Parse all timesync files
    let timesync_data = collect_timesync(&archive_path.display().to_string()).unwrap();
    archive_path.pop();

    // Keep UUID, UUID cache, timesync files in memory while we parse all tracev3 files
    // Allows for faster lookups
    parse_trace_file(
        &string_results,
        &shared_strings_results,
        &timesync_data,
        path,
        out,
        filter,
    );

    println!("\nFinished parsing Unified Log data. Saved results to: output.csv");
}

// Parse a live macOS system
pub fn parse_live_system(out: PathBuf, filter: LogFilter) {
    let strings = collect_strings_system().unwrap();
    let shared_strings = collect_shared_strings_system().unwrap();
    let timesync_data = collect_timesync_system().unwrap();

    parse_trace_file(
        &strings,
        &shared_strings,
        &timesync_data,
        PathBuf::from("/private/var/db/diagnostics"),
        out,
        filter,
    );

    println!("\nFinished parsing Unified Log data. Saved results to: output.csv");
}

// Use the provided strings, shared strings, timesync data to parse the Unified Log data at provided path.
// Currently expect the path to follow macOS log collect structure
fn parse_trace_file(
    string_results: &[UUIDText],
    shared_strings_results: &[SharedCacheStrings],
    timesync_data: &[TimesyncBoot],
    path: PathBuf,
    out: PathBuf,
    filter: LogFilter,
) {
    // We need to persist the Oversize log entries (they contain large strings that don't fit in normal log entries)
    // Some log entries have Oversize strings located in different tracev3 files.
    // This is very rare. Seen in ~20 log entries out of ~700,000. Seen in ~700 out of ~18 million
    let mut oversize_strings = UnifiedLogData {
        header: Vec::new(),
        catalog_data: Vec::new(),
        oversize: Vec::new(),
    };

    // Exclude missing data from returned output. Keep separate until we parse all oversize entries.
    // Then at end, go through all missing data and check all parsed oversize entries again
    let mut exclude_missing = true;
    let mut missing_data: Vec<UnifiedLogData> = Vec::new();

    let mut archive_path = path;
    archive_path.push("Persist");

    let mut log_count = 0;
    if archive_path.exists() {
        let paths = fs::read_dir(&archive_path).unwrap();

        // Loop through all tracev3 files in Persist directory
        for log_path in paths {
            let data = log_path.unwrap();
            let full_path = data.path().display().to_string();
            println!("Parsing: {}", full_path);

            let log_data = if data.path().exists() {
                parse_log(&full_path).unwrap()
            } else {
                println!("File {} no longer on disk", full_path);
                continue;
            };

            // Get all constructed logs and any log data that failed to get constrcuted (exclude_missing = true)
            let (results, missing_logs) = build_log(
                &log_data,
                string_results,
                shared_strings_results,
                timesync_data,
                exclude_missing,
            );
            // Track Oversize entries
            oversize_strings
                .oversize
                .append(&mut log_data.oversize.to_owned());

            // Track missing logs
            missing_data.push(missing_logs);
            log_count += results.len();
            output(&results, &out, &filter).unwrap();
        }
    }

    archive_path.pop();
    archive_path.push("Special");

    if archive_path.exists() {
        let paths = fs::read_dir(&archive_path).unwrap();

        // Loop through all tracev3 files in Special directory
        for log_path in paths {
            let data = log_path.unwrap();
            let full_path = data.path().display().to_string();
            println!("Parsing: {}", full_path);

            let mut log_data = if data.path().exists() {
                parse_log(&full_path).unwrap()
            } else {
                println!("File {} no longer on disk", full_path);
                continue;
            };

            // Append our old Oversize entries in case these logs point to other Oversize entries the previous tracev3 files
            log_data.oversize.append(&mut oversize_strings.oversize);
            let (results, missing_logs) = build_log(
                &log_data,
                string_results,
                shared_strings_results,
                timesync_data,
                exclude_missing,
            );
            // Track Oversize entries
            oversize_strings.oversize = log_data.oversize;
            // Track missing logs
            missing_data.push(missing_logs);
            log_count += results.len();

            output(&results, &out, &filter).unwrap();
        }
    }

    archive_path.pop();
    archive_path.push("Signpost");

    if archive_path.exists() {
        let paths = fs::read_dir(&archive_path).unwrap();

        // Loop through all tracev3 files in Signpost directory
        for log_path in paths {
            let data = log_path.unwrap();
            let full_path = data.path().display().to_string();
            println!("Parsing: {}", full_path);

            let log_data = if data.path().exists() {
                parse_log(&full_path).unwrap()
            } else {
                println!("File {} no longer on disk", full_path);
                continue;
            };

            let (results, missing_logs) = build_log(
                &log_data,
                string_results,
                shared_strings_results,
                timesync_data,
                exclude_missing,
            );

            // Signposts have not been seen with Oversize entries
            missing_data.push(missing_logs);
            log_count += results.len();

            output(&results, &out, &filter).unwrap();
        }
    }
    archive_path.pop();
    archive_path.push("HighVolume");

    if archive_path.exists() {
        let paths = fs::read_dir(&archive_path).unwrap();

        // Loop through all tracev3 files in HighVolume directory
        for log_path in paths {
            let data = log_path.unwrap();
            let full_path = data.path().display().to_string();
            println!("Parsing: {}", full_path);

            let log_data = if data.path().exists() {
                parse_log(&full_path).unwrap()
            } else {
                println!("File {} no longer on disk", full_path);
                continue;
            };
            let (results, missing_logs) = build_log(
                &log_data,
                string_results,
                shared_strings_results,
                timesync_data,
                exclude_missing,
            );

            // Oversize entries have not been seen in logs in HighVolume
            missing_data.push(missing_logs);
            log_count += results.len();

            output(&results, &out, &filter).unwrap();
        }
    }
    archive_path.pop();

    archive_path.push("logdata.LiveData.tracev3");

    // Check if livedata exists. We only have it if 'log collect' was used
    if archive_path.exists() {
        println!("Parsing: logdata.LiveData.tracev3");
        let mut log_data = parse_log(&archive_path.display().to_string()).unwrap();
        log_data.oversize.append(&mut oversize_strings.oversize);
        let (results, missing_logs) = build_log(
            &log_data,
            string_results,
            shared_strings_results,
            timesync_data,
            exclude_missing,
        );
        // Track missing data
        missing_data.push(missing_logs);
        log_count += results.len();

        output(&results, &out, &filter).unwrap();
        // Track oversize entries
        oversize_strings.oversize = log_data.oversize;
        archive_path.pop();
    }

    exclude_missing = false;

    // Since we have all Oversize entries now. Go through any log entries that we were not able to build before
    for mut leftover_data in missing_data {
        // Add all of our previous oversize data to logs for lookups
        leftover_data
            .oversize
            .append(&mut oversize_strings.oversize.to_owned());

        // Exclude_missing = false
        // If we fail to find any missing data its probably due to the logs rolling
        // Ex: tracev3A rolls, tracev3B references Oversize entry in tracev3A will trigger missing data since tracev3A is gone
        let (results, _) = build_log(
            &leftover_data,
            string_results,
            shared_strings_results,
            timesync_data,
            exclude_missing,
        );
        log_count += results.len();

        output(&results, &out, &filter).unwrap();
    }
    println!("Parsed {} log entries", log_count);
}

fn output(results: &Vec<LogData>, out: &PathBuf, filter: &LogFilter) -> Result<(), Box<dyn Error>> {
    let csv_file = OpenOptions::new().append(true).create(true).open(out)?;
    let mut writer = csv::Writer::from_writer(csv_file);
    let filter_str = match filter {
        LogFilter::LOGON => |l: &LogData| l.process.to_string().ends_with("logind"),
        LogFilter::SUDO =>  |l: &LogData| l.process.to_string().ends_with("sudo"),
        LogFilter::SSH => |l: &LogData| l.process.to_string().ends_with("ssh"),
        LogFilter::ALL => |l: &LogData| {
            let s = l.process.to_string();
            s.ends_with("logind") || s.ends_with("sudo")  || s.ends_with("ssh")
        }
    };
    for data in results {
        let date_time = Utc.timestamp_nanos(data.time as i64);
        if !filter_str(data) {
            continue;
        }
        writer.write_record(&[
            date_time.to_rfc3339_opts(SecondsFormat::Millis, true),
            data.event_type.to_owned(),
            data.log_type.to_owned(),
            data.subsystem.to_owned(),
            data.thread_id.to_string(),
            data.pid.to_string(),
            data.euid.to_string(),
            data.library.to_owned(),
            data.library_uuid.to_owned(),
            data.activity_id.to_string(),
            data.category.to_owned(),
            data.process.to_owned(),
            data.process_uuid.to_owned(),
            data.message.to_owned(),
            data.raw_message.to_owned(),
            data.boot_uuid.to_owned(),
            data.timezone_name.to_owned(),
        ])?;
    }
    Ok(())
}