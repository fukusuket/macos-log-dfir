use macos_unifiedlogs::dsc::SharedCacheStrings;
use macos_unifiedlogs::parser::{
    build_log, collect_shared_strings, collect_shared_strings_system, collect_strings,
    collect_strings_system, collect_timesync, collect_timesync_system, parse_log,
};
use macos_unifiedlogs::timesync::TimesyncBoot;
use macos_unifiedlogs::unified_log::UnifiedLogData;
use macos_unifiedlogs::uuidtext::UUIDText;
use std::fs;
use std::path::PathBuf;
use crate::output::output;

// Parse a provided directory path. Currently expect the path to follow macOS log collect structure
pub fn parse_log_archive(path: PathBuf, out: PathBuf) {
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
    );

    println!("\nFinished parsing Unified Log data. Saved results to: output.csv");
}

// Parse a live macOS system
pub fn parse_live_system(out: PathBuf) {
    let strings = collect_strings_system().unwrap();
    let shared_strings = collect_shared_strings_system().unwrap();
    let timesync_data = collect_timesync_system().unwrap();

    parse_trace_file(
        &strings,
        &shared_strings,
        &timesync_data,
        PathBuf::from("/private/var/db/diagnostics"),
        out,
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
    let archive_path = path;
    let archive_paths = vec![
        archive_path.join("Persist"),
        archive_path.join("Special"),
        archive_path.join("Signpost"),
        archive_path.join("HighVolume"),
    ];

    let mut missing_data: Vec<UnifiedLogData> = Vec::new();
    let mut log_count = 0;
    for path in archive_paths {
        if !path.exists() {
            continue;
        }
        dump_logs(
            string_results,
            shared_strings_results,
            timesync_data,
            &path,
            &out,
            &mut oversize_strings,
            true,
            &mut missing_data,
            &mut log_count,
        )
    }

    // Check if livedata exists. We only have it if 'log collect' was used
    if archive_path.join("logdata.LiveData.tracev3").exists() {
        println!("Parsing: logdata.LiveData.tracev3");
        let mut log_data = parse_log(&archive_path.display().to_string()).unwrap();
        log_data.oversize.append(&mut oversize_strings.oversize);
        let (results, missing_logs) = build_log(
            &log_data,
            string_results,
            shared_strings_results,
            timesync_data,
            true,
        );
        // Track missing data
        missing_data.push(missing_logs);
        log_count += results.len();

        output(&results, &out).unwrap();
        // Track oversize entries
        oversize_strings.oversize = log_data.oversize;
    }

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
            false,
        );
        log_count += results.len();

        output(&results, &out).unwrap();
    }
    println!("Parsed {} log entries", log_count);
}


fn dump_logs(
    string_results: &[UUIDText],
    shared_strings_results: &[SharedCacheStrings],
    timesync_data: &[TimesyncBoot],
    archive_path: &PathBuf,
    out: &PathBuf,
    oversize_strings: &mut UnifiedLogData,
    exclude_missing: bool,
    missing_data: &mut Vec<UnifiedLogData>,
    log_count: &mut usize,
) {
    let paths = fs::read_dir(archive_path).unwrap();

    // Loop through all tracev3 files in Persist directory
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

        // Get all constructed logs and any log data that failed to get constrcuted (exclude_missing = true)
        log_data.oversize.append(&mut oversize_strings.oversize);
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
        *log_count += results.len();
        output(&results, out).unwrap();
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}