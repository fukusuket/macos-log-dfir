use crate::rule::selectionnodes::SelectionNode;
use crate::rule::{condition_parser, selectionnodes};
use hashbrown::HashMap;
use nested::Nested;
use std::fmt::Debug;
use std::sync::Arc;
use macos_unifiedlogs::unified_log::LogData;
use yaml_rust::Yaml;

pub struct RuleNode {
    pub rulepath: String,
    pub yaml: Yaml,
    detection: DetectionNode,
}

struct DetectionNode {
    pub name_to_selection: HashMap<String, Arc<Box<dyn SelectionNode>>>,
    pub condition: Option<Box<dyn SelectionNode>>,
}

impl RuleNode {
    pub fn new(rule_path: String, yaml_data: Yaml) -> RuleNode {
        RuleNode {
            rulepath: rule_path,
            yaml: yaml_data,
            detection: DetectionNode::new(),
        }
    }

    pub fn init(&mut self) -> Result<(), Vec<String>> {
        let mut errmsgs: Vec<String> = vec![];

        // detection node initialization
        let detection_result = self.detection.init(&self.yaml["detection"]);
        if let Err(err_detail) = detection_result {
            errmsgs.extend(err_detail);
        }

        if errmsgs.is_empty() {
            Ok(())
        } else {
            Err(errmsgs)
        }
    }

    pub fn select(&mut self, event_record: &LogData) -> bool {
        self.detection.select(event_record)
    }
}

impl DetectionNode {
    fn new() -> DetectionNode {
        DetectionNode {
            name_to_selection: HashMap::new(),
            condition: None,
        }
    }

    fn init(&mut self, detection_yaml: &Yaml) -> Result<(), Vec<String>> {
        // selection nodeの初期化
        self.parse_name_to_selection(detection_yaml)?;

        // conditionに指定されている式を取得
        let condition = &detection_yaml["condition"].as_str();
        let condition_str = if let Some(cond_str) = condition {
            *cond_str
        } else {
            // conditionが指定されていない場合、selectionが一つだけならそのselectionを採用することにする。
            let mut keys = self.name_to_selection.keys();
            if keys.len() >= 2 {
                return Err(vec![
                    "There is no condition node under detection.".to_string()
                ]);
            }

            keys.next().unwrap()
        };

        // conditionをパースして、SelectionNodeに変換する
        let mut err_msgs = vec![];
        let compiler = condition_parser::ConditionCompiler::new();
        let compile_result = compiler.compile_condition(condition_str, &self.name_to_selection);
        if let Err(err_msg) = compile_result {
            err_msgs.extend(vec![err_msg]);
        } else {
            self.condition = Some(compile_result.unwrap());
        }

        if err_msgs.is_empty() {
            Ok(())
        } else {
            Err(err_msgs)
        }
    }

    pub fn select(&self, event_record: &LogData) -> bool {
        if self.condition.is_none() {
            return false;
        }

        let condition = &self.condition.as_ref().unwrap();
        condition.select(event_record)
    }

    /// selectionノードをパースします。
    fn parse_name_to_selection(&mut self, detection_yaml: &Yaml) -> Result<(), Vec<String>> {
        let detection_hash = detection_yaml.as_hash();
        if detection_hash.is_none() {
            return Err(vec!["Detection node was not found.".to_string()]);
        }

        // selectionをパースする。
        let detection_hash = detection_hash.unwrap();
        let keys = detection_hash.keys();
        let mut err_msgs = vec![];
        for key in keys {
            let name = key.as_str().unwrap_or("");
            if name.is_empty() {
                continue;
            }
            // condition等、特殊なキーワードを無視する。
            if name == "condition" || name == "timeframe" {
                continue;
            }

            // パースして、エラーメッセージがあれば配列にためて、戻り値で返す。
            let selection_node = self.parse_selection(&detection_hash[key]);
            if let Some(node) = selection_node {
                let mut selection_node = node;
                let init_result = selection_node.init();
                if let Err(err_detail) = init_result {
                    err_msgs.extend(err_detail);
                } else {
                    let rc_selection = Arc::new(selection_node);
                    self.name_to_selection
                        .insert(name.to_string(), rc_selection);
                }
            }
        }
        if !err_msgs.is_empty() {
            return Err(err_msgs);
        }

        // selectionノードが無いのはエラー
        if self.name_to_selection.is_empty() {
            return Err(vec![
                "There is no selection node under detection.".to_string()
            ]);
        }

        Ok(())
    }

    /// selectionをパースします。
    fn parse_selection(&self, selection_yaml: &Yaml) -> Option<Box<dyn SelectionNode>> {
        Some(Self::parse_selection_recursively(
            &Nested::<String>::new(),
            selection_yaml,
        ))
    }

    /// selectionをパースします。
    fn parse_selection_recursively(
        key_list: &Nested<String>,
        yaml: &Yaml,
    ) -> Box<dyn SelectionNode> {
        if yaml.as_hash().is_some() {
            // 連想配列はAND条件と解釈する
            let yaml_hash = yaml.as_hash().unwrap();
            let mut and_node = selectionnodes::AndSelectionNode::new();

            yaml_hash.keys().for_each(|hash_key| {
                let child_yaml = yaml_hash.get(hash_key).unwrap();
                let mut child_key_list = key_list.clone();
                child_key_list.push(hash_key.as_str().unwrap());
                let child_node = Self::parse_selection_recursively(&child_key_list, child_yaml);
                and_node.child_nodes.push(child_node);
            });
            Box::new(and_node)
        } else if yaml.as_vec().is_some()
            && !key_list.is_empty()
            && key_list[0].ends_with("|all")
            && !key_list[0].eq("|all")
        {
            //key_listにallが入っていた場合は子要素の配列はAND条件と解釈する。
            let mut and_node = selectionnodes::AndSelectionNode::new();
            yaml.as_vec().unwrap().iter().for_each(|child_yaml| {
                let child_node = Self::parse_selection_recursively(key_list, child_yaml);
                and_node.child_nodes.push(child_node);
            });
            Box::new(and_node)
        } else if yaml.as_vec().is_some() && !key_list.is_empty() && key_list[0].eq("|all") {
            // |all だけの場合、
            let mut or_node = selectionnodes::AllSelectionNode::new();
            yaml.as_vec().unwrap().iter().for_each(|child_yaml| {
                let child_node = Self::parse_selection_recursively(key_list, child_yaml);
                or_node.child_nodes.push(child_node);
            });
            Box::new(or_node)
        } else if yaml.as_vec().is_some() {
            // 配列はOR条件と解釈する。
            let mut or_node = selectionnodes::OrSelectionNode::new();
            yaml.as_vec().unwrap().iter().for_each(|child_yaml| {
                let child_node = Self::parse_selection_recursively(key_list, child_yaml);
                or_node.child_nodes.push(child_node);
            });
            Box::new(or_node)
        } else {
            // 連想配列と配列以外は末端ノード
            Box::new(selectionnodes::LeafSelectionNode::new(
                key_list.clone(),
                yaml.to_owned(),
            ))
        }
    }
}