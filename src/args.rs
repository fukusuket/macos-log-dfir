use clap::{Args, Parser, Subcommand, ValueEnum};
use std::path::PathBuf;

#[derive(Parser)]
pub struct AppArg {
    #[clap(subcommand)]
    pub action: Action,
}

#[derive(Clone, Debug, ValueEnum)]
pub enum LogFilter {
    FileDownload,
    Persistence,
    ProgExec,
    VolumeMount,
}

#[derive(Args, Clone, Debug)]
pub struct CsvTimelineOption {
    /// File path to one file
    #[arg(help_heading = Some("Input"), short = 'f', long = "file", value_name = "FILE", conflicts_with_all = ["live_analysis"])]
    pub filepath: Option<PathBuf>,

    /// Analyze the local Logs folder
    #[arg(help_heading = Some("Input"), short = 'l', long = "live_analysis", conflicts_with_all = ["filepath"])]
    pub live_analysis: bool,

    #[arg(value_enum, help_heading = Some("Filter"), short = 'f', long, value_name = "FILTER")]
    pub filter: LogFilter,

    #[arg(help_heading = Some("Output"), short = 'o', long, value_name = "FILE")]
    pub output: PathBuf,
}

#[derive(Subcommand)]
pub enum Action {
    CsvTimeline(CsvTimelineOption),
}
