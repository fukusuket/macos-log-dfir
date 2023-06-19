use crate::rule::rulenode::RuleNode;
use crate::yml::read_yaml_files;
use args::{Action, AppArg};
use clap::Parser;
use libmimalloc_sys::mi_stats_print_out;
use mimalloc::MiMalloc;
use parser::{parse_live_system, parse_log_archive};
use std::path::Path;
use std::ptr::null_mut;

mod args;
mod detection;
mod output;
mod parser;
mod yml;
mod rule {
    pub mod condition_parser;
    pub mod rulenode;
    pub mod selectionnodes;
}

#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

fn main() {
    let cli = AppArg::parse();
    let rule_folder = if Path::new("./rules").exists() {
        Path::new("./rules")
    } else {
        Path::new("../../rules")
    };
    let yaml = read_yaml_files(rule_folder).unwrap();
    let rule_nodes: Vec<RuleNode> = yaml
        .into_iter()
        .map(|(path, yaml_data)| RuleNode::new(path, yaml_data))
        .map(|mut rule: RuleNode| {
            // TODO　エラーハンドリング
            let _ = rule.init();
            rule
        })
        .collect();

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