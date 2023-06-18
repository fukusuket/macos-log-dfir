use std::fs;
use std::path::Path;
use yaml_rust::{Yaml, YamlLoader};

pub fn read_yaml_files(dir: &Path) -> Result<Vec<(String, Yaml)>, Box<dyn std::error::Error>> {
    let mut yaml_files = vec![];
    // 指定されたディレクトリ以下のファイルを再帰的に探索
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        // ファイルの拡張子がyamlもしくはymlの場合にのみ処理を行う
        if let Some(extension) = path.extension() {
            if extension == "yaml" || extension == "yml" {
                // YAMLファイルを読み込み、デシリアライズ
                let file_content = fs::read_to_string(&path)?;
                let yaml_contents = YamlLoader::load_from_str(&file_content);
                yaml_files.extend(yaml_contents.unwrap().into_iter().map(|yaml_content| {
                    let filepath = format!("{}", path.display());
                    (filepath, yaml_content)
                }));
            }
        }
    }

    Ok(yaml_files)
}

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}