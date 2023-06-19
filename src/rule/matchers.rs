use base64::{engine::general_purpose, Engine as _};
use cidr_utils::cidr::{IpCidr, IpCidrError};
use nested::Nested;
use regex::Regex;
use std::net::IpAddr;
use std::str::FromStr;
use std::{cmp::Ordering, collections::HashMap};
use yaml_rust::Yaml;

use downcast_rs::Downcast;
use macos_unifiedlogs::unified_log::LogData;
use memchr::memmem;

// 末端ノードがEventLogの値を比較するロジックを表す。
// 正規条件のマッチや文字数制限など、比較ロジック毎にこのtraitを実装したクラスが存在する。
//
// 新規にLeafMatcherを実装するクラスを作成した場合、
// LeafSelectionNodeのget_matchersクラスの戻り値の配列に新規作成したクラスのインスタンスを追加する。
pub trait LeafMatcher: Downcast {
    /// 指定されたkey_listにマッチするLeafMatcherであるかどうか判定する。
    fn is_target_key(&self, key_list: &Nested<String>) -> bool;

    /// 引数に指定されたJSON形式のデータがマッチするかどうか判定する。
    /// main.rsでWindows Event LogをJSON形式に変換していて、そのJSON形式のWindowsのイベントログデータがここには来る
    /// 例えば正規表現でマッチするロジックなら、ここに正規表現でマッチさせる処理を書く。
    fn is_match(&self, event_value: Option<&String>, recinfo: &LogData) -> bool;

    /// 初期化ロジックをここに記載します。
    /// ルールファイルの書き方が間違っている等の原因により、正しくルールファイルからパースできない場合、戻り値のResult型でエラーを返してください。
    fn init(&mut self, key_list: &Nested<String>, select_value: &Yaml) -> Result<(), Vec<String>>;
}
downcast_rs::impl_downcast!(LeafMatcher);

// 正規表現マッチは遅いため、できるだけ高速なstd::stringのlen/starts_with/ends_with/containsでマッチ判定するためのenum
#[derive(PartialEq, Debug)]
enum FastMatch {
    Exact(String),
    StartsWith(String),
    EndsWith(String),
    Contains(String),
    AllOnly(String),
}

/// デフォルトのマッチクラス
/// ワイルドカードの処理やパイプ
pub struct DefaultMatcher {
    re: Option<Regex>,
    fast_match: Option<Vec<FastMatch>>,
    pipes: Vec<PipeElement>,
    key_list: Nested<String>,
}

impl DefaultMatcher {
    pub fn new() -> DefaultMatcher {
        DefaultMatcher {
            re: None,
            fast_match: None,
            pipes: Vec::new(),
            key_list: Nested::<String>::new(),
        }
    }

    /// このmatcherの正規表現とマッチするかどうか判定します。
    /// 判定対象の文字列とこのmatcherが保持する正規表現が完全にマッチした場合のTRUEを返します。
    /// 例えば、判定対象文字列が"abc"で、正規表現が"ab"の場合、正規表現は判定対象文字列の一部分にしか一致していないので、この関数はfalseを返します。
    fn is_regex_fullmatch(&self, value: &str) -> bool {
        return self.re.as_ref().unwrap().find_iter(value).any(|match_obj| {
            return match_obj.as_str() == value;
        });
    }

    /// Hayabusaのルールファイルのフィールド名とそれに続いて指定されるパイプを、正規表現形式の文字列に変換します。
    /// ワイルドカードの文字列を正規表現にする処理もこのメソッドに実装されています。patternにワイルドカードの文字列を指定して、pipesにPipeElement::Wildcardを指定すればOK!!
    fn from_pattern_to_regex_str(pattern: String, pipes: &[PipeElement]) -> String {
        // パターンをPipeで処理する。
        pipes
            .iter()
            .fold(pattern, |acc, pipe| pipe.pipe_pattern(acc))
    }

    fn eq_ignore_case(event_value_str: &str, match_str: &str) -> bool {
        if match_str.len() == event_value_str.len() {
            return match_str.eq_ignore_ascii_case(event_value_str);
        }
        false
    }

    fn starts_with_ignore_case(event_value_str: &str, match_str: &str) -> Option<bool> {
        let len = match_str.len();
        if len > event_value_str.len() {
            return Some(false);
        }
        // マルチバイト文字を含む場合は、index out of boundsになるため、asciiのみ
        if event_value_str.is_ascii() {
            let match_result = match_str.eq_ignore_ascii_case(&event_value_str[0..len]);
            return Some(match_result);
        }
        None
    }

    fn ends_with_ignore_case(event_value_str: &str, match_str: &str) -> Option<bool> {
        let len1 = match_str.len();
        let len2 = event_value_str.len();
        if len1 > len2 {
            return Some(false);
        }
        // マルチバイト文字を含む場合は、index out of boundsになるため、asciiのみ
        if event_value_str.is_ascii() {
            let match_result = match_str.eq_ignore_ascii_case(&event_value_str[len2 - len1..]);
            return Some(match_result);
        }
        None
    }

    // ワイルドカードマッチを高速なstd::stringのlen/starts_with/ends_withに変換するための関数
    fn convert_to_fast_match(s: &str, ignore_case: bool) -> Option<Vec<FastMatch>> {
        let wildcard_count = s.chars().filter(|c| *c == '*').count();
        let is_literal_asterisk = |s: &str| s.ends_with(r"\*") && !s.ends_with(r"\\*");
        if contains_str(s, "?") || s.ends_with(r"\\\*") || (!s.is_ascii() && contains_str(s, "*")) {
            // 高速なマッチに変換できないパターンは、正規表現マッチのみ
            return None;
        } else if s.starts_with("allOnly*") && s.ends_with('*') && wildcard_count == 2 {
            let removed_asterisk = s[8..(s.len() - 1)].replace(r"\\", r"\");
            if ignore_case {
                return Some(vec![FastMatch::AllOnly(removed_asterisk.to_lowercase())]);
            }
            return Some(vec![FastMatch::AllOnly(removed_asterisk)]);
        } else if s.starts_with('*')
            && s.ends_with('*')
            && wildcard_count == 2
            && !is_literal_asterisk(s)
        {
            let removed_asterisk = s[1..(s.len() - 1)].replace(r"\\", r"\");
            // *が先頭と末尾だけは、containsに変換
            if ignore_case {
                return Some(vec![FastMatch::Contains(removed_asterisk.to_lowercase())]);
            }
            return Some(vec![FastMatch::Contains(removed_asterisk)]);
        } else if s.starts_with('*') && wildcard_count == 1 && !is_literal_asterisk(s) {
            // *が先頭は、ends_withに変換
            return Some(vec![FastMatch::EndsWith(s[1..].replace(r"\\", r"\"))]);
        } else if s.ends_with('*') && wildcard_count == 1 && !is_literal_asterisk(s) {
            // *が末尾は、starts_withに変換
            return Some(vec![FastMatch::StartsWith(
                s[..(s.len() - 1)].replace(r"\\", r"\"),
            )]);
        } else if contains_str(s, "*") {
            // *が先頭・末尾以外にあるパターンは、starts_with/ends_withに変換できないため、正規表現マッチのみ
            return None;
        }
        // *を含まない場合は、文字列長マッチに変換
        Some(vec![FastMatch::Exact(s.replace(r"\\", r"\"))])
    }
}

impl LeafMatcher for DefaultMatcher {
    fn is_target_key(&self, key_list: &Nested<String>) -> bool {
        if key_list.len() <= 1 {
            return true;
        }

        return key_list.get(1).unwrap() == "value";
    }

    fn init(&mut self, key_list: &Nested<String>, select_value: &Yaml) -> Result<(), Vec<String>> {
        let mut tmp_key_list = Nested::<String>::new();
        tmp_key_list.extend(key_list.iter());
        self.key_list = tmp_key_list;
        if select_value.is_null() {
            return Ok(());
        }

        // patternをパースする
        let yaml_value = match select_value {
            Yaml::Boolean(b) => Some(b.to_string()),
            Yaml::Integer(i) => Some(i.to_string()),
            Yaml::Real(r) => Some(r.to_string()),
            Yaml::String(s) => Some(s.to_owned()),
            _ => None,
        };
        if yaml_value.is_none() {
            let errmsg = format!("An unknown error occured. [key:{}]", "");
            return Err(vec![errmsg]);
        }
        let pattern = yaml_value.unwrap();
        // Pipeが指定されていればパースする
        let emp = String::default();
        // 一つ目はただのキーで、2つめ以jj降がpipe

        let mut keys_all: Vec<&str> = key_list.get(0).unwrap_or(&emp).split('|').collect(); // key_listが空はあり得ない

        //all -> allOnlyの対応関係
        let mut change_map: HashMap<&str, &str> = HashMap::new();
        change_map.insert("all", "allOnly");

        //先頭が｜の場合を検知して、all -> allOnlyに変更
        if keys_all[0].is_empty() && keys_all.len() == 2 && keys_all[1] == "all" {
            keys_all[1] = change_map["all"];
        }

        let keys_without_head = &keys_all[1..];

        let mut err_msges = vec![];
        for key in keys_without_head.iter() {
            let pipe_element = PipeElement::new(key, &pattern, key_list);
            match pipe_element {
                Ok(element) => {
                    self.pipes.push(element);
                }
                Err(e) => {
                    err_msges.push(e);
                }
            }
        }
        if !err_msges.is_empty() {
            return Err(err_msges);
        }
        let n = self.pipes.len();
        if n == 0 {
            // パイプがないケース
            self.fast_match = Self::convert_to_fast_match(&pattern, true);
        } else if n == 1 {
            // パイプがあるケース
            self.fast_match = match &self.pipes[0] {
                PipeElement::Startswith => {
                    Self::convert_to_fast_match(format!("{pattern}*").as_str(), true)
                }
                PipeElement::Endswith => {
                    Self::convert_to_fast_match(format!("*{pattern}").as_str(), true)
                }
                PipeElement::Contains => {
                    Self::convert_to_fast_match(format!("*{pattern}*").as_str(), true)
                }
                PipeElement::AllOnly => {
                    Self::convert_to_fast_match(format!("allOnly*{pattern}*").as_str(), true)
                }
                _ => None,
            };
        } else if n == 2 {
            if self.pipes[0] == PipeElement::Base64offset && self.pipes[1] == PipeElement::Contains
            {
                // |base64offset|containsの場合
                let val = pattern.as_str();
                let val_byte = val.as_bytes();
                let mut fastmatches = vec![];
                for i in 0..3 {
                    let mut b64_result = vec![];
                    let mut target_byte = vec![];
                    target_byte.resize_with(i, || 0b0);
                    target_byte.extend_from_slice(val_byte);
                    b64_result.resize_with(target_byte.len() * 4 / 3 + 4, || 0b0);
                    general_purpose::STANDARD
                        .encode_slice(target_byte, &mut b64_result)
                        .ok();
                    let convstr_b64 = String::from_utf8(b64_result);
                    if let Ok(b64_str) = convstr_b64 {
                        // ここでContainsのfastmatch対応を行う
                        let filtered_null_chr = b64_str.replace('\0', "");
                        let b64_offset_contents = match b64_str.find('=').unwrap_or_default() % 4 {
                            2 => {
                                if i == 0 {
                                    filtered_null_chr[..filtered_null_chr.len() - 3].to_string()
                                } else {
                                    filtered_null_chr[(i + 1)..filtered_null_chr.len() - 3]
                                        .to_string()
                                }
                            }
                            3 => {
                                if i == 0 {
                                    filtered_null_chr[..filtered_null_chr.len() - 2].to_string()
                                } else {
                                    filtered_null_chr.replace('\0', "")
                                        [(i + 1)..filtered_null_chr.len() - 2]
                                        .to_string()
                                }
                            }
                            _ => {
                                if i == 0 {
                                    filtered_null_chr
                                } else {
                                    filtered_null_chr[(i + 1)..].to_string()
                                }
                            }
                        };
                        if let Some(fm) =
                            Self::convert_to_fast_match(&format!("*{b64_offset_contents}*"), false)
                        {
                            fastmatches.extend(fm);
                        }
                    } else {
                        err_msges.push(format!(
                            "Failed base64 encoding: {}",
                            convstr_b64.unwrap_err()
                        ));
                    }
                }
                if !fastmatches.is_empty() {
                    self.fast_match = Some(fastmatches);
                }
            } else if self.pipes[0] == PipeElement::Contains && self.pipes[1] == PipeElement::All
            // |contains|allの場合、事前の分岐でAndNodeとしているのでここではcontainsのみとして取り扱う
            {
                self.fast_match =
                    Self::convert_to_fast_match(format!("*{pattern}*").as_str(), true);
            }
        } else {
            let errmsg = format!("Multiple pipe elements cannot be used. key:{}", "");
            return Err(vec![errmsg]);
        }
        if self.fast_match.is_some()
            && matches!(
                &self.fast_match.as_ref().unwrap()[0],
                FastMatch::Exact(_) | FastMatch::Contains(_)
            )
            && !self.key_list.is_empty()
        {
            // FastMatch::Exact/Contains検索に置き換えられたときは正規表現は不要
            return Ok(());
        }
        // 正規表現ではない場合、ワイルドカードであることを表す。
        // ワイルドカードは正規表現でマッチングするので、ワイルドカードを正規表現に変換するPipeを内部的に追加することにする。
        let is_re = self
            .pipes
            .iter()
            .any(|pipe_element| matches!(pipe_element, PipeElement::Re));
        if !is_re {
            self.pipes.push(PipeElement::Wildcard);
        }

        let pattern = DefaultMatcher::from_pattern_to_regex_str(pattern, &self.pipes);
        // Pipeで処理されたパターンを正規表現に変換
        let re_result = Regex::new(&pattern);
        if re_result.is_err() {
            let errmsg = format!("Cannot parse regex. [regex:{}, key:{}]", pattern, "");
            return Err(vec![errmsg]);
        }
        self.re = re_result.ok();

        Ok(())
    }

    fn is_match(&self, event_value: Option<&String>, recinfo: &LogData) -> bool {
        let pipe: &PipeElement = self.pipes.first().unwrap_or(&PipeElement::Wildcard);
        let match_result = match pipe {
            PipeElement::Cidr(ip_result) => match ip_result {
                Ok(mut matcher_ip) => {
                    let val = String::default();
                    let event_value_str = event_value.unwrap_or(&val);
                    let event_ip = IpAddr::from_str(event_value_str);
                    match event_ip {
                        Ok(target_ip) => Some(matcher_ip.contains(target_ip)),
                        Err(_) => Some(false), //IPアドレス以外の形式のとき
                    }
                }
                Err(_) => Some(false), //IPアドレス以外の形式のとき
            },
            _ => None,
        };
        if let Some(result) = match_result {
            return result;
        }

        // yamlにnullが設定されていた場合
        // keylistが空(==JSONのgrep検索)の場合、無視する。
        if self.key_list.is_empty() && self.re.is_none() && self.fast_match.is_none() {
            return false;
        }

        // yamlにnullが設定されていた場合
        if self.re.is_none() && self.fast_match.is_none() {
            // レコード内に対象のフィールドが存在しなければ検知したものとして扱う
            // for v in self.key_list.iter() {
            //     if recinfo.get_value(v).is_none() {
            //         return true;
            //     }
            // }
            return false;
        }

        if event_value.is_none() {
            return false;
        }

        let event_value_str = event_value.unwrap();
        if self.key_list.is_empty() {
            // この場合ただのgrep検索なので、ただ正規表現に一致するかどうか調べればよいだけ
            return self.re.as_ref().unwrap().is_match(event_value_str);
        } else if let Some(fast_matcher) = &self.fast_match {
            let fast_match_result = if fast_matcher.len() == 1 {
                match &fast_matcher[0] {
                    FastMatch::Exact(s) => Some(Self::eq_ignore_case(event_value_str, s)),
                    FastMatch::StartsWith(s) => Self::starts_with_ignore_case(event_value_str, s),
                    FastMatch::EndsWith(s) => Self::ends_with_ignore_case(event_value_str, s),
                    FastMatch::Contains(s) | FastMatch::AllOnly(s) => {
                        Some(contains_str(&event_value_str.to_lowercase(), s))
                    }
                }
            } else {
                Some(fast_matcher.iter().any(|fm| match fm {
                    FastMatch::Contains(s) => contains_str(event_value_str, s),
                    _ => false,
                }))
            };
            if let Some(is_match) = fast_match_result {
                return is_match;
            }
        }
        // 文字数/starts_with/ends_with検索に変換できなかった場合は、正規表現マッチで比較
        self.is_regex_fullmatch(event_value_str)
    }
}

/// パイプ(|)で指定される要素を表すクラス。
/// 要リファクタリング
#[derive(PartialEq)]
enum PipeElement {
    Startswith,
    Endswith,
    Contains,
    Re,
    Wildcard,
    Base64offset,
    Cidr(Result<IpCidr, IpCidrError>),
    All,
    AllOnly,
}

impl PipeElement {
    fn new(key: &str, pattern: &str, key_list: &Nested<String>) -> Result<PipeElement, String> {
        let pipe_element = match key {
            "startswith" => Some(PipeElement::Startswith),
            "endswith" => Some(PipeElement::Endswith),
            "contains" => Some(PipeElement::Contains),
            "re" => Some(PipeElement::Re),
            "base64offset" => Some(PipeElement::Base64offset),
            "cidr" => Some(PipeElement::Cidr(IpCidr::from_str(pattern))),
            "all" => Some(PipeElement::All),
            "allOnly" => Some(PipeElement::AllOnly),
            _ => None,
        };

        if let Some(elment) = pipe_element {
            Ok(elment)
        } else {
            Err(format!("An unknown pipe element was specified. key:{}", ""))
        }
    }

    /// patternをパイプ処理します
    fn pipe_pattern(&self, pattern: String) -> String {
        // enumでポリモーフィズムを実装すると、一つのメソッドに全部の型の実装をする感じになる。Java使い的にはキモイ感じがする。
        let fn_add_asterisk_end = |patt: String| {
            if patt.ends_with("//*") {
                patt
            } else if patt.ends_with("/*") {
                patt + "*"
            } else if patt.ends_with('*') {
                patt
            } else if patt.ends_with('\\') {
                // 末尾が\(バックスラッシュ1つ)の場合は、末尾を\\* (バックスラッシュ2つとアスタリスク)に変換する
                // 末尾が\\*は、バックスラッシュ1文字とそれに続けてワイルドカードパターンであることを表す
                patt + "\\*"
            } else {
                patt + "*"
            }
        };
        let fn_add_asterisk_begin = |patt: String| {
            if patt.starts_with("//*") {
                patt
            } else if patt.starts_with("/*") {
                "*".to_string() + &patt
            } else if patt.starts_with('*') {
                patt
            } else {
                "*".to_string() + &patt
            }
        };

        match self {
            // startswithの場合はpatternの最後にwildcardを足すことで対応する
            PipeElement::Startswith => fn_add_asterisk_end(pattern),
            // endswithの場合はpatternの最初にwildcardを足すことで対応する
            PipeElement::Endswith => fn_add_asterisk_begin(pattern),
            // containsの場合はpatternの前後にwildcardを足すことで対応する
            PipeElement::Contains => fn_add_asterisk_end(fn_add_asterisk_begin(pattern)),
            // WildCardは正規表現に変換する。
            PipeElement::Wildcard => PipeElement::pipe_pattern_wildcard(pattern),
            _ => pattern,
        }
    }

    /// PipeElement::Wildcardのパイプ処理です。
    /// pipe_pattern()に含めて良い処理ですが、複雑な処理になってしまったので別関数にしました。
    fn pipe_pattern_wildcard(pattern: String) -> String {
        let wildcards = vec!["*", "?"];

        // patternをwildcardでsplitした結果をpattern_splitsに入れる
        // 以下のアルゴリズムの場合、pattern_splitsの偶数indexの要素はwildcardじゃない文字列となり、奇数indexの要素はwildcardが入る。
        let mut idx = 0;
        let mut pattern_splits = vec![];
        let mut cur_str = String::default();
        while idx < pattern.len() {
            let prev_idx = idx;
            for wildcard in &wildcards {
                let cur_pattern: String = pattern.chars().skip(idx).collect::<String>();
                if cur_pattern.starts_with(&format!(r"\\{wildcard}")) {
                    // wildcardの前にエスケープ文字が2つある場合
                    cur_str = format!("{}{}", cur_str, r"\");
                    pattern_splits.push(cur_str);
                    pattern_splits.push(wildcard.to_string());

                    cur_str = String::default();
                    idx += 3;
                    break;
                } else if cur_pattern.starts_with(&format!(r"\{wildcard}")) {
                    // wildcardの前にエスケープ文字が1つある場合
                    cur_str = format!("{cur_str}{wildcard}");
                    idx += 2;
                    break;
                } else if cur_pattern.starts_with(wildcard) {
                    // wildcardの場合
                    pattern_splits.push(cur_str);
                    pattern_splits.push(wildcard.to_string());

                    cur_str = String::default();
                    idx += 1;
                    break;
                }
            }
            // 上記のFor文でHitした場合はcontinue
            if prev_idx != idx {
                continue;
            }

            cur_str = format!(
                "{}{}",
                cur_str,
                pattern.chars().skip(idx).take(1).collect::<String>()
            );
            idx += 1;
        }
        // 最後の文字がwildcardじゃない場合は、cur_strに文字が入っているので、それをpattern_splitsに入れておく
        if !cur_str.is_empty() {
            pattern_splits.push(cur_str);
        }

        // SIGMAルールのwildcard表記から正規表現の表記に変換します。
        let ret = pattern_splits.iter().enumerate().fold(
            String::default(),
            |acc: String, (idx, pattern)| {
                let regex_value = if idx % 2 == 0 {
                    // wildcardじゃない場合はescapeした文字列を返す
                    regex::escape(pattern)
                } else {
                    // wildcardの場合、"*"は".*"という正規表現に変換し、"?"は"."に変換する。
                    let wildcard_regex_value = if *pattern == "*" {
                        "(.|\\a|\\f|\\t|\\n|\\r|\\v)*"
                    } else {
                        "."
                    };
                    wildcard_regex_value.to_string()
                };

                format!("{acc}{regex_value}")
            },
        );

        // sigmaのwildcardはcase insensitive
        // なので、正規表現の先頭にcase insensitiveであることを表す記号を付与
        "(?i)".to_string() + &ret
    }
}

fn contains_str(input: &str, check: &str) -> bool {
    memmem::find(input.as_bytes(), check.as_bytes()).is_some()
}
