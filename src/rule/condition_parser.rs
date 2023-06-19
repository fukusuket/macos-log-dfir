use lazy_static::lazy_static;
use regex::Regex;

use self::selectionnodes::{
    AndSelectionNode, NotSelectionNode, OrSelectionNode, RefSelectionNode, SelectionNode,
};
use super::selectionnodes;
use hashbrown::HashMap;
use itertools::Itertools;
use std::{sync::Arc, vec::IntoIter};

lazy_static! {
    pub static ref CONDITION_REGEXMAP: Vec<Regex> = vec![
        Regex::new(r"^\(").unwrap(),
        Regex::new(r"^\)").unwrap(),
        Regex::new(r"^ ").unwrap(),
        Regex::new(r"^\w+").unwrap(),
    ];
    pub static ref RE_PIPE: Regex = Regex::new(r"\|.*").unwrap();
    // all of selection* と 1 of selection* にマッチする正規表現
    pub static ref OF_SELECTION: Regex = Regex::new(r"(all|1) of ([^*]+)\*").unwrap();
}

#[derive(Debug, Clone)]
/// 字句解析で出てくるトークン
pub enum ConditionToken {
    LeftParenthesis,
    RightParenthesis,
    Space,
    Not,
    And,
    Or,
    SelectionReference(String),

    // パースの時に上手く処理するために作った疑似的なトークン
    ParenthesisContainer(IntoIter<ConditionToken>), // 括弧を表すトークン
    AndContainer(IntoIter<ConditionToken>),         // ANDでつながった条件をまとめるためのトークン
    OrContainer(IntoIter<ConditionToken>),          // ORでつながった条件をまとめるためのトークン
    NotContainer(IntoIter<ConditionToken>), // 「NOT」と「NOTで否定される式」をまとめるためのトークン この配列には要素が一つしか入らないが、他のContainerと同じように扱えるようにするためにVecにしている。あんまり良くない。
    OperandContainer(IntoIter<ConditionToken>), // ANDやORやNOT等の演算子に対して、非演算子を表す
}

// ここを参考にしました。https://qiita.com/yasuo-ozu/items/7ce2f8ff846ba00dd244
impl IntoIterator for ConditionToken {
    type Item = ConditionToken;
    type IntoIter = IntoIter<ConditionToken>;

    fn into_iter(self) -> Self::IntoIter {
        match self {
            ConditionToken::ParenthesisContainer(sub_tokens) => sub_tokens,
            ConditionToken::AndContainer(sub_tokens) => sub_tokens,
            ConditionToken::OrContainer(sub_tokens) => sub_tokens,
            ConditionToken::NotContainer(sub_tokens) => sub_tokens,
            ConditionToken::OperandContainer(sub_tokens) => sub_tokens,
            _ => vec![].into_iter(),
        }
    }
}

impl ConditionToken {
    fn replace_subtoken(&self, sub_tokens: Vec<ConditionToken>) -> ConditionToken {
        match self {
            ConditionToken::ParenthesisContainer(_) => {
                ConditionToken::ParenthesisContainer(sub_tokens.into_iter())
            }
            ConditionToken::AndContainer(_) => ConditionToken::AndContainer(sub_tokens.into_iter()),
            ConditionToken::OrContainer(_) => ConditionToken::OrContainer(sub_tokens.into_iter()),
            ConditionToken::NotContainer(_) => ConditionToken::NotContainer(sub_tokens.into_iter()),
            ConditionToken::OperandContainer(_) => {
                ConditionToken::OperandContainer(sub_tokens.into_iter())
            }
            ConditionToken::LeftParenthesis => ConditionToken::LeftParenthesis,
            ConditionToken::RightParenthesis => ConditionToken::RightParenthesis,
            ConditionToken::Space => ConditionToken::Space,
            ConditionToken::Not => ConditionToken::Not,
            ConditionToken::And => ConditionToken::And,
            ConditionToken::Or => ConditionToken::Or,
            ConditionToken::SelectionReference(name) => {
                ConditionToken::SelectionReference(name.clone())
            }
        }
    }

    pub fn sub_tokens(&self) -> Vec<ConditionToken> {
        // TODO ここでcloneを使わずに実装できるようにしたい。
        match self {
            ConditionToken::ParenthesisContainer(sub_tokens) => sub_tokens.as_slice().to_vec(),
            ConditionToken::AndContainer(sub_tokens) => sub_tokens.as_slice().to_vec(),
            ConditionToken::OrContainer(sub_tokens) => sub_tokens.as_slice().to_vec(),
            ConditionToken::NotContainer(sub_tokens) => sub_tokens.as_slice().to_vec(),
            ConditionToken::OperandContainer(sub_tokens) => sub_tokens.as_slice().to_vec(),
            ConditionToken::LeftParenthesis => vec![],
            ConditionToken::RightParenthesis => vec![],
            ConditionToken::Space => vec![],
            ConditionToken::Not => vec![],
            ConditionToken::And => vec![],
            ConditionToken::Or => vec![],
            ConditionToken::SelectionReference(_) => vec![],
        }
    }

    pub fn sub_tokens_without_parenthesis(&self) -> Vec<ConditionToken> {
        match self {
            ConditionToken::ParenthesisContainer(_) => vec![],
            _ => self.sub_tokens(),
        }
    }
}

#[derive(Debug)]
pub struct ConditionCompiler {}

// conditionの式を読み取るクラス。
impl ConditionCompiler {
    pub fn new() -> Self {
        ConditionCompiler {}
    }

    pub fn compile_condition(
        &self,
        condition_str: &str,
        name_2_node: &HashMap<String, Arc<Box<dyn SelectionNode>>>,
    ) -> Result<Box<dyn SelectionNode>, String> {
        let node_keys: Vec<String> = name_2_node.keys().cloned().collect();
        let condition_str = Self::convert_condition(condition_str, &node_keys);
        // パイプはここでは処理しない
        let captured = self::RE_PIPE.captures(condition_str.as_str());
        let replaced_condition = if let Some(cap) = captured {
            let captured = cap.get(0).unwrap().as_str();
            condition_str.replacen(captured, "", 1)
        } else {
            condition_str.to_string()
        };

        let result = self.compile_condition_body(&replaced_condition, name_2_node);
        if let Err(msg) = result {
            Err(format!("A condition parse error has occurred. {msg}"))
        } else {
            result
        }
    }

    // all of selection* と 1 of selection* を通常のand/orに変換する
    pub fn convert_condition(condition_str: &str, node_keys: &[String]) -> String {
        let mut converted_str = condition_str.to_string();
        for matched in OF_SELECTION.find_iter(condition_str) {
            let match_str: &str = matched.as_str();
            let sep = if match_str.starts_with("all") {
                " and "
            } else {
                " or "
            };
            let target_node_key_prefix = match_str
                .replace('*', "")
                .replace("all of ", "")
                .replace("1 of ", "");
            let replaced_condition = node_keys
                .iter()
                .filter(|x| x.starts_with(target_node_key_prefix.as_str()))
                .join(sep);
            converted_str =
                converted_str.replace(match_str, format!("({})", replaced_condition).as_str())
        }
        converted_str
    }

    /// 与えたConditionからSelectionNodeを作る
    fn compile_condition_body(
        &self,
        condition_str: &str,
        name_2_node: &HashMap<String, Arc<Box<dyn SelectionNode>>>,
    ) -> Result<Box<dyn SelectionNode>, String> {
        let tokens = self.tokenize(condition_str)?;

        let parsed = self.parse(tokens.into_iter())?;

        Self::to_selectnode(parsed, name_2_node)
    }

    /// 構文解析を実行する。
    fn parse(&self, tokens: IntoIter<ConditionToken>) -> Result<ConditionToken, String> {
        // 括弧で囲まれた部分を解析します。
        // (括弧で囲まれた部分は後で解析するため、ここでは一時的にConditionToken::ParenthesisContainerに変換しておく)
        // 括弧の中身を解析するのはparse_rest_parenthesis()で行う。
        let tokens = self.parse_parenthesis(tokens)?;

        // AndとOrをパースする。
        let tokens = self.parse_and_or_operator(tokens)?;

        // OperandContainerトークンの中身をパースする。(現状、Notを解析するためだけにある。将来的に修飾するキーワードが増えたらここを変える。)
        let token = Self::parse_operand_container(tokens)?;

        // 括弧で囲まれている部分を探して、もしあればその部分を再帰的に構文解析します。
        self.parse_rest_parenthesis(token)
    }

    /// 括弧で囲まれている部分を探して、もしあればその部分を再帰的に構文解析します。
    fn parse_rest_parenthesis(&self, token: ConditionToken) -> Result<ConditionToken, String> {
        if let ConditionToken::ParenthesisContainer(sub_token) = token {
            let new_token = self.parse(sub_token)?;
            return Ok(new_token);
        }

        let sub_tokens = token.sub_tokens();
        if sub_tokens.is_empty() {
            return Ok(token);
        }

        let mut new_sub_tokens = vec![];
        for sub_token in sub_tokens {
            let new_token = self.parse_rest_parenthesis(sub_token)?;
            new_sub_tokens.push(new_token);
        }
        Ok(token.replace_subtoken(new_sub_tokens))
    }

    /// 字句解析を行う
    fn tokenize(&self, condition_str: &str) -> Result<Vec<ConditionToken>, String> {
        let mut cur_condition_str = condition_str.to_string();

        let mut tokens = Vec::new();
        while !cur_condition_str.is_empty() {
            let captured = self::CONDITION_REGEXMAP.iter().find_map(|regex| {
                return regex.captures(cur_condition_str.as_str());
            });
            if captured.is_none() {
                // トークンにマッチしないのはありえないという方針でパースしています。
                return Err("An unusable character was found.".to_string());
            }

            let mached_str = captured.unwrap().get(0).unwrap().as_str();
            let token = self.to_enum(mached_str.to_string());
            if let ConditionToken::Space = token {
                // 空白は特に意味ないので、読み飛ばす。
                cur_condition_str = cur_condition_str.replacen(mached_str, "", 1);
                continue;
            }

            tokens.push(token);
            cur_condition_str = cur_condition_str.replacen(mached_str, "", 1);
        }

        Ok(tokens)
    }

    /// 文字列をConditionTokenに変換する。
    fn to_enum(&self, token: String) -> ConditionToken {
        if token == "(" {
            ConditionToken::LeftParenthesis
        } else if token == ")" {
            ConditionToken::RightParenthesis
        } else if token == " " {
            ConditionToken::Space
        } else if token == "not" {
            ConditionToken::Not
        } else if token == "and" {
            ConditionToken::And
        } else if token == "or" {
            ConditionToken::Or
        } else {
            ConditionToken::SelectionReference(token)
        }
    }

    /// 右括弧と左括弧をだけをパースする。戻り値の配列にはLeftParenthesisとRightParenthesisが含まれず、代わりにTokenContainerに変換される。TokenContainerが括弧で囲まれた部分を表現している。
    fn parse_parenthesis(
        &self,
        mut tokens: IntoIter<ConditionToken>,
    ) -> Result<Vec<ConditionToken>, String> {
        let mut ret = vec![];
        while let Some(token) = tokens.next() {
            // まず、左括弧を探す。
            let is_left = matches!(token, ConditionToken::LeftParenthesis);
            if !is_left {
                ret.push(token);
                continue;
            }

            // 左括弧が見つかったら、対応する右括弧を見つける。
            let mut left_cnt = 1;
            let mut right_cnt = 0;
            let mut sub_tokens = vec![];
            for token in tokens.by_ref() {
                if let ConditionToken::LeftParenthesis = token {
                    left_cnt += 1;
                } else if let ConditionToken::RightParenthesis = token {
                    right_cnt += 1;
                }
                if left_cnt == right_cnt {
                    break;
                }
                sub_tokens.push(token);
            }
            // 最後までついても対応する右括弧が見つからないことを表している
            if left_cnt != right_cnt {
                return Err("')' was expected but not found.".to_string());
            }

            // ここで再帰的に呼び出す。
            ret.push(ConditionToken::ParenthesisContainer(sub_tokens.into_iter()));
        }

        // この時点で右括弧が残っている場合は右括弧の数が左括弧よりも多いことを表している。
        let is_right_left = ret
            .iter()
            .any(|token| matches!(token, ConditionToken::RightParenthesis));
        if is_right_left {
            return Err("'(' was expected but not found.".to_string());
        }

        Ok(ret)
    }

    /// AND, ORをパースする。
    fn parse_and_or_operator(&self, tokens: Vec<ConditionToken>) -> Result<ConditionToken, String> {
        if tokens.is_empty() {
            // 長さ0は呼び出してはいけない
            return Err("Unknown error.".to_string());
        }

        // まず、selection1 and not selection2みたいな式のselection1やnot selection2のように、ANDやORでつながるトークンをまとめる。
        let tokens = self.to_operand_container(tokens)?;

        // 先頭又は末尾がAND/ORなのはだめ
        if self.is_logical(&tokens[0]) || self.is_logical(&tokens[tokens.len() - 1]) {
            return Err("An illegal logical operator(and, or) was found.".to_string());
        }

        // OperandContainerとLogicalOperator(AndとOR)が交互に並んでいるので、それぞれリストに投入
        let mut operand_list = vec![];
        let mut operator_list = vec![];
        for (i, token) in tokens.into_iter().enumerate() {
            if (i % 2 == 1) != self.is_logical(&token) {
                // インデックスが奇数の時はLogicalOperatorで、インデックスが偶数のときはOperandContainerになる
                return Err(
                    "The use of a logical operator(and, or) was wrong.".to_string(),
                );
            }

            if i % 2 == 0 {
                // ここで再帰的にAND,ORをパースする関数を呼び出す
                operand_list.push(token);
            } else {
                operator_list.push(token);
            }
        }

        // 先にANDでつながっている部分を全部まとめる
        let mut operant_ite = operand_list.into_iter();
        let mut operands = vec![operant_ite.next().unwrap()];
        for token in operator_list.iter() {
            if let ConditionToken::Or = token {
                // Orの場合はそのままリストに追加
                operands.push(operant_ite.next().unwrap());
            } else {
                // Andの場合はANDでつなげる
                let and_operands = vec![operands.pop().unwrap(), operant_ite.next().unwrap()];
                let and_container = ConditionToken::AndContainer(and_operands.into_iter());
                operands.push(and_container);
            }
        }

        // 次にOrでつながっている部分をまとめる
        let or_contaienr = ConditionToken::OrContainer(operands.into_iter());
        Ok(or_contaienr)
    }

    /// OperandContainerの中身をパースする。現状はNotをパースするためだけに存在している。
    fn parse_operand_container(parent_token: ConditionToken) -> Result<ConditionToken, String> {
        if let ConditionToken::OperandContainer(sub_tokens) = parent_token {
            // 現状ではNOTの場合は、「not」と「notで修飾されるselectionノードの名前」の2つ入っているはず
            // NOTが無い場合、「selectionノードの名前」の一つしか入っていないはず。

            // 上記の通り、3つ以上入っていることはないはず。
            if sub_tokens.len() >= 3 {
                return Err(
                    "Unknown error. Maybe it is because there are multiple names of selection nodes."
                        .to_string(),
                );
            }

            // 0はありえないはず
            if sub_tokens.len() == 0 {
                return Err("Unknown error.".to_string());
            }

            // 1つだけ入っている場合、NOTはありえない。
            if sub_tokens.len() == 1 {
                let operand_subtoken = sub_tokens.into_iter().next().unwrap();
                if let ConditionToken::Not = operand_subtoken {
                    return Err("An illegal not was found.".to_string());
                }

                return Ok(operand_subtoken);
            }

            // ２つ入っている場合、先頭がNotで次はNotじゃない何かのはず
            let mut sub_tokens_ite = sub_tokens;
            let first_token = sub_tokens_ite.next().unwrap();
            let second_token = sub_tokens_ite.next().unwrap();
            if let ConditionToken::Not = first_token {
                if let ConditionToken::Not = second_token {
                    Err("Not is continuous.".to_string())
                } else {
                    let not_container =
                        ConditionToken::NotContainer(vec![second_token].into_iter());
                    Ok(not_container)
                }
            } else {
                Err(
                    "Unknown error. Maybe it is because there are multiple names of selection nodes."
                        .to_string(),
                )
            }
        } else {
            let sub_tokens = parent_token.sub_tokens_without_parenthesis();
            if sub_tokens.is_empty() {
                return Ok(parent_token);
            }

            let mut new_sub_tokens = vec![];
            for sub_token in sub_tokens {
                let new_sub_token = Self::parse_operand_container(sub_token)?;
                new_sub_tokens.push(new_sub_token);
            }

            Ok(parent_token.replace_subtoken(new_sub_tokens))
        }
    }

    /// ConditionTokenからSelectionNodeトレイトを実装した構造体に変換します。
    fn to_selectnode(
        token: ConditionToken,
        name_2_node: &HashMap<String, Arc<Box<dyn SelectionNode>>>,
    ) -> Result<Box<dyn SelectionNode>, String> {
        // RefSelectionNodeに変換
        if let ConditionToken::SelectionReference(selection_name) = token {
            let selection_node = name_2_node.get(&selection_name);
            if let Some(select_node) = selection_node {
                let selection_node = select_node;
                let selection_node = Arc::clone(selection_node);
                let ref_node = RefSelectionNode::new(selection_node);
                return Ok(Box::new(ref_node));
            } else {
                let err_msg = format!("{selection_name} is not defined.");
                return Err(err_msg);
            }
        }

        // AndSelectionNodeに変換
        if let ConditionToken::AndContainer(sub_tokens) = token {
            let mut select_and_node = AndSelectionNode::new();
            for sub_token in sub_tokens {
                let sub_node = Self::to_selectnode(sub_token, name_2_node)?;
                select_and_node.child_nodes.push(sub_node);
            }
            return Ok(Box::new(select_and_node));
        }

        // OrSelectionNodeに変換
        if let ConditionToken::OrContainer(sub_tokens) = token {
            let mut select_or_node = OrSelectionNode::new();
            for sub_token in sub_tokens {
                let sub_node = Self::to_selectnode(sub_token, name_2_node)?;
                select_or_node.child_nodes.push(sub_node);
            }
            return Ok(Box::new(select_or_node));
        }

        // NotSelectionNodeに変換
        if let ConditionToken::NotContainer(sub_tokens) = token {
            if sub_tokens.len() > 1 {
                return Err("Unknown error".to_string());
            }

            let select_sub_node =
                Self::to_selectnode(sub_tokens.into_iter().next().unwrap(), name_2_node)?;
            let select_not_node = NotSelectionNode::new(select_sub_node);
            return Ok(Box::new(select_not_node));
        }

        Err("Unknown error".to_string())
    }

    /// ConditionTokenがAndまたはOrTokenならばTrue
    fn is_logical(&self, token: &ConditionToken) -> bool {
        matches!(token, ConditionToken::And | ConditionToken::Or)
    }

    /// ConditionToken::OperandContainerに変換できる部分があれば変換する。
    fn to_operand_container(
        &self,
        tokens: Vec<ConditionToken>,
    ) -> Result<Vec<ConditionToken>, String> {
        let mut ret = vec![];
        let mut grouped_operands = vec![]; // ANDとORの間にあるトークンを表す。ANDとORをOperatorとしたときのOperand
        for token in tokens.into_iter() {
            if self.is_logical(&token) {
                // ここに来るのはエラーのはずだが、後でエラー出力するので、ここではエラー出さない。
                if grouped_operands.is_empty() {
                    ret.push(token);
                    continue;
                }
                ret.push(ConditionToken::OperandContainer(
                    grouped_operands.into_iter(),
                ));
                ret.push(token);
                grouped_operands = vec![];
                continue;
            }

            grouped_operands.push(token);
        }
        if !grouped_operands.is_empty() {
            ret.push(ConditionToken::OperandContainer(
                grouped_operands.into_iter(),
            ));
        }

        Ok(ret)
    }
}