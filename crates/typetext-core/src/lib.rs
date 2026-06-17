use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SnippetFile {
    pub version: u32,
    pub groups: Vec<SnippetGroup>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SnippetGroup {
    pub name: String,
    pub snippets: Vec<Snippet>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Snippet {
    pub title: String,
    pub body: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    #[serde(default = "default_hotkey")]
    pub hotkey: String,
    #[serde(default = "default_typing_delay_ms")]
    pub typing_delay_ms: u64,
    #[serde(default = "default_close_after_insert")]
    pub close_after_insert: bool,
    #[serde(default, alias = "startWithWindows")]
    pub open_on_startup: bool,
    #[serde(default = "default_theme")]
    pub theme: String,
    #[serde(default = "default_queued_snippet_click_action")]
    pub queued_snippet_click_action: QueuedSnippetClickAction,
    #[serde(default = "default_check_for_updates")]
    pub check_for_updates: bool,
    #[serde(default)]
    pub last_update_check_unix: Option<u64>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum QueuedSnippetClickAction {
    AddAgain,
    Remove,
}

#[derive(Debug, Clone)]
pub struct PortablePaths {
    pub data_dir: PathBuf,
    pub snippets_path: PathBuf,
    pub settings_path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchResult {
    pub group_index: usize,
    pub snippet_index: usize,
    pub group_name: String,
    pub title: String,
    pub body: String,
}

impl Default for SnippetFile {
    fn default() -> Self {
        Self {
            version: 1,
            groups: vec![SnippetGroup {
                name: "Common Replies".to_string(),
                snippets: vec![
                    Snippet {
                        title: "Follow up".to_string(),
                        body: "Hi, just following up on this. Please let me know if you need anything else.".to_string(),
                    },
                    Snippet {
                        title: "Thanks".to_string(),
                        body: "Thanks for your help. I appreciate it.".to_string(),
                    },
                ],
            }],
        }
    }
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            hotkey: default_hotkey(),
            typing_delay_ms: default_typing_delay_ms(),
            close_after_insert: default_close_after_insert(),
            open_on_startup: false,
            theme: default_theme(),
            queued_snippet_click_action: default_queued_snippet_click_action(),
            check_for_updates: default_check_for_updates(),
            last_update_check_unix: None,
        }
    }
}

impl PortablePaths {
    pub fn beside_executable() -> Result<Self> {
        let exe = std::env::current_exe().context("Could not determine current executable path")?;
        let app_dir = exe
            .parent()
            .ok_or_else(|| anyhow!("Could not determine executable directory"))?;
        Ok(Self::from_app_dir(app_dir))
    }

    pub fn from_app_dir(app_dir: impl AsRef<Path>) -> Self {
        let data_dir = app_dir.as_ref().join("data");
        Self {
            snippets_path: data_dir.join("snippets.json"),
            settings_path: data_dir.join("settings.json"),
            data_dir,
        }
    }
}

pub fn load_or_create_snippets(paths: &PortablePaths) -> Result<SnippetFile> {
    load_or_create_json(&paths.snippets_path, &SnippetFile::default()).and_then(|file| {
        validate_snippets(&file)?;
        Ok(file)
    })
}

pub fn save_snippets(paths: &PortablePaths, snippets: &SnippetFile) -> Result<()> {
    validate_snippets(snippets)?;
    save_json(&paths.snippets_path, snippets)
}

pub fn export_snippets(path: impl AsRef<Path>, snippets: &SnippetFile) -> Result<()> {
    validate_snippets(snippets)?;
    save_json(path.as_ref(), snippets)
}

pub fn load_or_create_settings(paths: &PortablePaths) -> Result<AppSettings> {
    load_or_create_json(&paths.settings_path, &AppSettings::default())
}

pub fn save_settings(paths: &PortablePaths, settings: &AppSettings) -> Result<()> {
    save_json(paths.settings_path.as_path(), settings)
}

pub fn import_droptext_ini(path: impl AsRef<Path>) -> Result<SnippetFile> {
    let path = path.as_ref();
    let raw =
        fs::read_to_string(path).with_context(|| format!("Could not read {}", path.display()))?;
    parse_droptext_ini(&raw).with_context(|| format!("Could not parse {}", path.display()))
}

pub fn parse_droptext_ini(raw: &str) -> Result<SnippetFile> {
    let mut groups: Vec<SnippetGroup> = Vec::new();
    let mut current_group: Option<usize> = None;

    for (line_index, raw_line) in raw.trim_start_matches('\u{feff}').lines().enumerate() {
        let line_number = line_index + 1;
        let line = raw_line.trim();

        if line.is_empty() || line.starts_with(';') || line.starts_with('#') {
            continue;
        }

        if line.starts_with('[') {
            let Some(section_end) = line.find(']') else {
                return Err(anyhow!("Line {line_number}: section is missing closing ]"));
            };
            if !line[section_end + 1..].trim().is_empty() {
                return Err(anyhow!(
                    "Line {line_number}: unexpected text after section name"
                ));
            }

            let name = line[1..section_end].trim();
            if name.is_empty() {
                return Err(anyhow!("Line {line_number}: section name is empty"));
            }

            current_group = groups.iter().position(|group| group.name == name);
            if current_group.is_none() {
                groups.push(SnippetGroup {
                    name: name.to_string(),
                    snippets: Vec::new(),
                });
                current_group = Some(groups.len() - 1);
            }
            continue;
        }

        let Some(separator) = line.find('=') else {
            return Err(anyhow!("Line {line_number}: expected key=value entry"));
        };
        let Some(group_index) = current_group else {
            return Err(anyhow!(
                "Line {line_number}: entry appears before any section"
            ));
        };

        let title = line[..separator].trim();
        if title.is_empty() {
            return Err(anyhow!("Line {line_number}: snippet title is empty"));
        }

        let body = parse_droptext_value(line[separator + 1..].trim(), line_number)?;
        groups[group_index].snippets.push(Snippet {
            title: title.to_string(),
            body,
        });
    }

    groups.retain(|group| !group.snippets.is_empty());
    if groups.is_empty() {
        return Err(anyhow!("No DropText snippets were found."));
    }

    let snippets = SnippetFile { version: 1, groups };
    validate_snippets(&snippets)?;
    Ok(snippets)
}

pub fn search_snippets(snippets: &SnippetFile, query: &str) -> Vec<SearchResult> {
    let query = query.trim().to_lowercase();
    snippets
        .groups
        .iter()
        .enumerate()
        .flat_map(|(group_index, group)| {
            let query = query.clone();
            group
                .snippets
                .iter()
                .enumerate()
                .filter_map(move |(snippet_index, snippet)| {
                    let haystack =
                        format!("{} {} {}", group.name, snippet.title, snippet.body).to_lowercase();
                    if query.is_empty() || haystack.contains(&query) {
                        Some(SearchResult {
                            group_index,
                            snippet_index,
                            group_name: group.name.clone(),
                            title: snippet.title.clone(),
                            body: snippet.body.clone(),
                        })
                    } else {
                        None
                    }
                })
        })
        .collect()
}

pub fn validate_snippets(snippets: &SnippetFile) -> Result<()> {
    for group in &snippets.groups {
        if group.name.trim().is_empty() {
            return Err(anyhow!("Every group needs a name."));
        }

        for snippet in &group.snippets {
            if snippet.title.trim().is_empty() {
                return Err(anyhow!("Every snippet needs a title."));
            }
        }
    }

    Ok(())
}

fn load_or_create_json<T>(path: &Path, default_value: &T) -> Result<T>
where
    T: Serialize + for<'de> Deserialize<'de> + Clone,
{
    if !path.exists() {
        save_json(path, default_value)?;
        return Ok(default_value.clone());
    }

    let raw =
        fs::read_to_string(path).with_context(|| format!("Could not read {}", path.display()))?;
    serde_json::from_str(&raw).with_context(|| format!("Could not parse {}", path.display()))
}

fn save_json<T>(path: &Path, value: &T) -> Result<()>
where
    T: Serialize,
{
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("Could not create {}", parent.display()))?;
    }

    let raw = serde_json::to_string_pretty(value)?;
    let temp_path = path.with_extension("json.tmp");
    fs::write(&temp_path, format!("{raw}\n"))
        .with_context(|| format!("Could not write {}", temp_path.display()))?;
    fs::rename(&temp_path, path).with_context(|| {
        format!(
            "Could not move {} to {}",
            temp_path.display(),
            path.display()
        )
    })?;
    Ok(())
}

fn parse_droptext_value(raw: &str, line_number: usize) -> Result<String> {
    let Some(unquoted) = raw.strip_prefix('"') else {
        return Ok(raw.trim().to_string());
    };

    let mut value = String::new();
    let mut chars = unquoted.chars();
    let mut closed = false;

    while let Some(ch) = chars.next() {
        match ch {
            '"' => {
                closed = true;
                break;
            }
            '\\' => {
                let Some(escaped) = chars.next() else {
                    value.push('\\');
                    break;
                };
                match escaped {
                    'r' => value.push('\r'),
                    'n' => value.push('\n'),
                    't' => value.push('\t'),
                    '"' => value.push('"'),
                    '\\' => value.push('\\'),
                    other => {
                        value.push('\\');
                        value.push(other);
                    }
                }
            }
            other => value.push(other),
        }
    }

    if !closed {
        return Err(anyhow!(
            "Line {line_number}: quoted value is missing closing \""
        ));
    }

    if !chars.as_str().trim().is_empty() {
        return Err(anyhow!(
            "Line {line_number}: unexpected text after quoted value"
        ));
    }

    Ok(value.replace("\r\n", "\n").replace('\r', "\n"))
}

fn default_hotkey() -> String {
    "Ctrl+Alt+Space".to_string()
}

fn default_typing_delay_ms() -> u64 {
    80
}

fn default_close_after_insert() -> bool {
    true
}

fn default_theme() -> String {
    "system".to_string()
}

fn default_queued_snippet_click_action() -> QueuedSnippetClickAction {
    QueuedSnippetClickAction::AddAgain
}

fn default_check_for_updates() -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_matches_title_body_and_group() {
        let snippets = SnippetFile::default();

        assert_eq!(search_snippets(&snippets, "follow").len(), 1);
        assert_eq!(search_snippets(&snippets, "help").len(), 1);
        assert_eq!(search_snippets(&snippets, "common").len(), 2);
    }

    #[test]
    fn parse_droptext_ini_converts_sections_to_groups() {
        let raw = r#"
[Initial Progranm Exam Entry]
Program Exam Summary="Requested to conduct an examination \r\nConducted Exam \r\nCompleted Exam Stuff. \r\nDone the details"

[Initial Do Exam Entry]
Program Exam Done="Done the exam \r\nFor details please conduct further \r\nI am not even sure. \r\nDone the details"
"#;

        let snippets = parse_droptext_ini(raw).unwrap();

        assert_eq!(snippets.version, 1);
        assert_eq!(snippets.groups.len(), 2);
        assert_eq!(snippets.groups[0].name, "Initial Progranm Exam Entry");
        assert_eq!(snippets.groups[0].snippets[0].title, "Program Exam Summary");
        assert_eq!(
            snippets.groups[0].snippets[0].body,
            "Requested to conduct an examination \nConducted Exam \nCompleted Exam Stuff. \nDone the details"
        );
    }

    #[test]
    fn parse_droptext_ini_merges_duplicate_sections() {
        let raw = r#"
[Common]
One="First"
[Common]
Two="Second"
"#;

        let snippets = parse_droptext_ini(raw).unwrap();

        assert_eq!(snippets.groups.len(), 1);
        assert_eq!(snippets.groups[0].snippets.len(), 2);
        assert_eq!(snippets.groups[0].snippets[1].title, "Two");
    }

    #[test]
    fn parse_droptext_ini_reports_entries_before_sections() {
        let error = parse_droptext_ini(r#"Title="Body""#).unwrap_err();

        assert!(error.to_string().contains("before any section"));
    }
}
