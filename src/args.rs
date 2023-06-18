use clap::{Args, Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(version, about, long_about = None)]
pub struct AppArg {
    #[clap(subcommand)]
    pub action: Action,

    /// Print debug information (memory usage, etc...)
    #[clap(long = "debug", global = true, hide = true)]
    pub debug: bool,
}

#[derive(Args, Clone, Debug)]
pub struct CsvTimelineOption {
    /// Path to logarchive formatted directory
    #[arg(help_heading = Some("Input"), short = 'a', long = "archive_dir", value_name = "ARCHIVE", conflicts_with_all = ["live_analysis"])]
    pub archive_dir: Option<PathBuf>,
    /// Run on live system
    #[arg(help_heading = Some("Input"), short = 'l', long = "live_analysis", conflicts_with_all = ["archive_dir"])]
    pub live_analysis: bool,

    #[arg(help_heading = Some("Output"), short = 'o', long = "output", value_name = "OUTPUT")]
    pub output: PathBuf,
}

#[derive(Subcommand)]
pub enum Action {
    CsvTimeline(CsvTimelineOption),
}

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
