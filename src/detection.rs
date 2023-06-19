use crate::RuleNode;
use macos_unifiedlogs::unified_log::LogData;

#[derive(Debug)]
pub struct DetectInfo {
    pub rulepath: String,
    pub ruletitle: String,
    pub level: String,
    pub logdata: LogData,
}

pub fn detect(results: &Vec<LogData>, rulenode: &Vec<RuleNode>) -> Vec<DetectInfo> {
    vec![]
}

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
