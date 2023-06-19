use downcast_rs::Downcast;
use nested::Nested;
use std::{sync::Arc, vec};
use yaml_rust::Yaml;

// Ruleファイルの detection- selection配下のノードはこのtraitを実装する。
pub trait SelectionNode: Downcast {
    // 引数で指定されるイベントログのレコードが、条件に一致するかどうかを判定する
    // このトレイトを実装する構造体毎に適切な判定処理を書く必要がある。
    fn select(&self, event_record: &str) -> bool;

    // 初期化処理を行う
    // 戻り値としてエラーを返却できるようになっているので、Ruleファイルが間違っていて、SelectionNodeを構成出来ない時はここでエラーを出す
    // AndSelectionNode等ではinit()関数とは別にnew()関数を実装しているが、new()関数はただインスタンスを作るだけにして、あまり長い処理を書かないようにしている。
    // これはRuleファイルのパースのエラー処理をinit()関数にまとめるためにこうしている。
    fn init(&mut self) -> Result<(), Vec<String>>;

    // 子ノードを取得する(グラフ理論のchildと同じ意味)
    fn get_childs(&self) -> Vec<&dyn SelectionNode>;

    // 子孫ノードを取得する(グラフ理論のdescendantと同じ意味)
    fn get_descendants(&self) -> Vec<&dyn SelectionNode>;
}
downcast_rs::impl_downcast!(SelectionNode);

/// detection - selection配下でAND条件を表すノード
pub struct AndSelectionNode {
    pub child_nodes: Vec<Box<dyn SelectionNode>>,
}

impl AndSelectionNode {
    pub fn new() -> AndSelectionNode {
        AndSelectionNode {
            child_nodes: vec![],
        }
    }
}

impl SelectionNode for AndSelectionNode {
    fn select(&self, event_record: &str) -> bool {
        self.child_nodes
            .iter()
            .all(|child_node| child_node.select(event_record))
    }

    fn init(&mut self) -> Result<(), Vec<String>> {
        let err_msgs = self
            .child_nodes
            .iter_mut()
            .map(|child_node| {
                let res = child_node.init();
                if let Err(err) = res {
                    err
                } else {
                    vec![]
                }
            })
            .fold(
                vec![],
                |mut acc: Vec<String>, cur: Vec<String>| -> Vec<String> {
                    acc.extend(cur.into_iter());
                    acc
                },
            );

        if err_msgs.is_empty() {
            Ok(())
        } else {
            Err(err_msgs)
        }
    }

    fn get_childs(&self) -> Vec<&dyn SelectionNode> {
        let mut ret = vec![];
        self.child_nodes.iter().for_each(|child_node| {
            ret.push(child_node.as_ref());
        });

        ret
    }

    fn get_descendants(&self) -> Vec<&dyn SelectionNode> {
        let mut ret = self.get_childs();

        self.child_nodes
            .iter()
            .flat_map(|child_node| child_node.get_descendants())
            .for_each(|descendant_node| {
                ret.push(descendant_node);
            });

        ret
    }
}

/// detection - selection配下でAll条件を表すノード
pub struct AllSelectionNode {
    pub child_nodes: Vec<Box<dyn SelectionNode>>,
}

impl AllSelectionNode {
    pub fn new() -> AllSelectionNode {
        AllSelectionNode {
            child_nodes: vec![],
        }
    }
}

impl SelectionNode for AllSelectionNode {
    fn select(&self, event_record: &str) -> bool {
        self.child_nodes
            .iter()
            .all(|child_node| child_node.select(event_record))
    }

    fn init(&mut self) -> Result<(), Vec<String>> {
        let err_msgs = self
            .child_nodes
            .iter_mut()
            .map(|child_node| {
                let res = child_node.init();
                if let Err(err) = res {
                    err
                } else {
                    vec![]
                }
            })
            .fold(
                vec![],
                |mut acc: Vec<String>, cur: Vec<String>| -> Vec<String> {
                    acc.extend(cur.into_iter());
                    acc
                },
            );

        if err_msgs.is_empty() {
            Ok(())
        } else {
            Err(err_msgs)
        }
    }

    fn get_childs(&self) -> Vec<&dyn SelectionNode> {
        let mut ret = vec![];
        self.child_nodes.iter().for_each(|child_node| {
            ret.push(child_node.as_ref());
        });

        ret
    }

    fn get_descendants(&self) -> Vec<&dyn SelectionNode> {
        let mut ret = self.get_childs();

        self.child_nodes
            .iter()
            .flat_map(|child_node| child_node.get_descendants())
            .for_each(|descendant_node| {
                ret.push(descendant_node);
            });

        ret
    }
}

/// detection - selection配下でOr条件を表すノード
pub struct OrSelectionNode {
    pub child_nodes: Vec<Box<dyn SelectionNode>>,
}

impl OrSelectionNode {
    pub fn new() -> OrSelectionNode {
        OrSelectionNode {
            child_nodes: vec![],
        }
    }
}

impl SelectionNode for OrSelectionNode {
    fn select(&self, event_record: &str) -> bool {
        self.child_nodes
            .iter()
            .any(|child_node| child_node.select(event_record))
    }

    fn init(&mut self) -> Result<(), Vec<String>> {
        let err_msgs = self
            .child_nodes
            .iter_mut()
            .map(|child_node| {
                let res = child_node.init();
                if let Err(err) = res {
                    err
                } else {
                    vec![]
                }
            })
            .fold(
                vec![],
                |mut acc: Vec<String>, cur: Vec<String>| -> Vec<String> {
                    acc.extend(cur.into_iter());
                    acc
                },
            );

        if err_msgs.is_empty() {
            Ok(())
        } else {
            Err(err_msgs)
        }
    }

    fn get_childs(&self) -> Vec<&dyn SelectionNode> {
        let mut ret = vec![];
        self.child_nodes.iter().for_each(|child_node| {
            ret.push(child_node.as_ref());
        });

        ret
    }

    fn get_descendants(&self) -> Vec<&dyn SelectionNode> {
        let mut ret = self.get_childs();

        self.child_nodes
            .iter()
            .flat_map(|child_node| child_node.get_descendants())
            .for_each(|descendant_node| {
                ret.push(descendant_node);
            });

        ret
    }
}

/// conditionでNotを表すノード
pub struct NotSelectionNode {
    node: Box<dyn SelectionNode>,
}

impl NotSelectionNode {
    pub fn new(select_node: Box<dyn SelectionNode>) -> NotSelectionNode {
        NotSelectionNode { node: select_node }
    }
}

impl SelectionNode for NotSelectionNode {
    fn select(&self, event_record: &str) -> bool {
        !self.node.select(event_record)
    }

    fn init(&mut self) -> Result<(), Vec<String>> {
        Ok(())
    }

    fn get_childs(&self) -> Vec<&dyn SelectionNode> {
        vec![]
    }

    fn get_descendants(&self) -> Vec<&dyn SelectionNode> {
        self.get_childs()
    }
}

/// detectionで定義した条件をconditionで参照するためのもの
pub struct RefSelectionNode {
    // selection_nodeはDetectionNodeのname_2_nodeが所有権を持っていて、RefSelectionNodeのselection_nodeに所有権を持たせることができない。
    // そこでArcを使って、DetectionNodeのname_2_nodeとRefSelectionNodeのselection_nodeで所有権を共有する。
    // RcじゃなくてArcなのはマルチスレッド対応のため
    selection_node: Arc<Box<dyn SelectionNode>>,
}

impl RefSelectionNode {
    pub fn new(select_node: Arc<Box<dyn SelectionNode>>) -> RefSelectionNode {
        RefSelectionNode {
            selection_node: select_node,
        }
    }
}

impl SelectionNode for RefSelectionNode {
    fn select(&self, event_record: &str) -> bool {
        self.selection_node.select(event_record)
    }

    fn init(&mut self) -> Result<(), Vec<String>> {
        Ok(())
    }

    fn get_childs(&self) -> Vec<&dyn SelectionNode> {
        vec![self.selection_node.as_ref().as_ref()]
    }

    fn get_descendants(&self) -> Vec<&dyn SelectionNode> {
        self.get_childs()
    }
}

pub struct LeafSelectionNode {
    key: String,
    key_list: Nested<String>,
    select_value: Yaml,
}

impl LeafSelectionNode {
    pub fn new(keys: Nested<String>, value_yaml: Yaml) -> LeafSelectionNode {
        LeafSelectionNode {
            key: String::default(),
            key_list: keys,
            select_value: value_yaml,
        }
    }
}

impl SelectionNode for LeafSelectionNode {
    fn select(&self, event_record: &str) -> bool {
        true
    }

    fn init(&mut self) -> Result<(), Vec<String>> {
        Ok(())
    }

    fn get_childs(&self) -> Vec<&dyn SelectionNode> {
        vec![]
    }

    fn get_descendants(&self) -> Vec<&dyn SelectionNode> {
        vec![]
    }
}
