use std::fs;
use std::path::Path;
use yaml_rust::{Yaml, YamlLoader};

pub fn read_yaml_files(dir: &Path) -> Result<Vec<(String, Yaml)>, Box<dyn std::error::Error>> {
    let mut yaml_files = vec![];
    visit_dirs(dir, &mut yaml_files)?;
    Ok(yaml_files)
}

fn visit_dirs(
    dir: &Path,
    yaml_files: &mut Vec<(String, Yaml)>,
) -> Result<(), Box<dyn std::error::Error>> {
    if dir.is_dir() {
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();

            if path.is_dir() {
                visit_dirs(&path, yaml_files)?;
            } else if let Some(extension) = path.extension() {
                if extension == "yaml" || extension == "yml" {
                    let file_content = fs::read_to_string(&path)?;
                    let yaml_contents = YamlLoader::load_from_str(&file_content);
                    yaml_files.extend(yaml_contents.unwrap().into_iter().map(|yaml_content| {
                        let filepath = format!("{}", path.display());
                        (filepath, yaml_content)
                    }));
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::yml::read_yaml_files;
    use std::path::{Path, PathBuf};

    #[test]
    fn test_read_yaml_files() {
        let r = read_yaml_files(&Path::new("./rules")).unwrap();
        assert_eq!(r.len(), 66);
    }
}
