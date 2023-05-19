use clap::Parser;
use args::{Action, AppArg, };
use parser::{parse_log_archive, parse_live_system};

mod args;
mod parser;

fn main() {
    let cli = AppArg::parse();
    match cli.action {
        Action::CsvTimeline(opt) => {
            if opt.live_analysis {
                parse_live_system(opt.output)
            } else {
                parse_log_archive(opt.filepath.unwrap(), opt.output)
            }
        }
    }
}