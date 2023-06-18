use args::{Action, AppArg};
use clap::Parser;
use libmimalloc_sys::mi_stats_print_out;
use mimalloc::MiMalloc;
use parser::{parse_live_system, parse_log_archive};
use std::ptr::null_mut;

mod args;
mod parser;
mod output;
mod yml;

#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

fn main() {
    let cli = AppArg::parse();
    match cli.action {
        Action::CsvTimeline(opt) => {
            if opt.live_analysis {
                parse_live_system(opt.output)
            } else {
                parse_log_archive(opt.archive_dir.unwrap(), opt.output)
            }
        }
    }
    if cli.debug {
        println!();
        println!("Memory usage stats:");
        unsafe {
            mi_stats_print_out(None, null_mut());
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}