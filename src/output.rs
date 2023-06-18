use chrono::{SecondsFormat, TimeZone, Utc};
use macos_unifiedlogs::unified_log::LogData;
use std::error::Error;
use std::fs::OpenOptions;
use std::path::PathBuf;

pub fn output(results: &Vec<LogData>, out: &PathBuf) -> Result<(), Box<dyn Error>> {
    let csv_file = OpenOptions::new().append(true).create(true).open(out)?;
    let mut writer = csv::Writer::from_writer(csv_file);
    for data in results {
        let date_time = Utc.timestamp_nanos(data.time as i64);
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

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
