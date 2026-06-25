use anyhow::{Context, Result, anyhow};
use chrono::{DateTime, FixedOffset, Local};
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::{Read, Take, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

pub const DEFAULT_TYPING_DELAY_MS: u64 = 80;
pub const DEFAULT_WINDOWS_CHARACTER_DELAY_MS: u64 = 22;
pub const DEFAULT_WINDOWS_SEPARATOR_DELAY_MS: u64 = 35;
pub const DEFAULT_EMPTY_LINES_BETWEEN_SNIPPETS: u32 = 0;
pub const MAX_TYPING_DELAY_MS: u64 = 2_000;
pub const MAX_WINDOWS_INPUT_DELAY_MS: u64 = 250;
pub const MAX_EMPTY_LINES_BETWEEN_SNIPPETS: u32 = 12;
pub const MAX_SNIPPET_FILE_BYTES: u64 = 10 * 1024 * 1024;
pub const MAX_SETTINGS_FILE_BYTES: u64 = 64 * 1024;
pub const MAX_GROUPS: usize = 1_000;
pub const MAX_SNIPPETS: usize = 10_000;
pub const MAX_GROUP_NAME_CHARS: usize = 256;
pub const MAX_SNIPPET_TITLE_CHARS: usize = 512;
pub const MAX_SNIPPET_BODY_CHARS: usize = 1_000_000;
pub const SUPPORTED_SNIPPET_TOKENS: &[(&str, &str)] = &[
    ("time.today", "Current time (legacy DropText alias)"),
    ("time.now", "Current time"),
    ("date.today", "Today's date"),
    ("date.tomorrow", "Tomorrow's date"),
    ("date.yesterday", "Yesterday's date"),
    ("datetime.now", "Current date and time"),
    ("weekday.today", "Current weekday"),
];

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SnippetFile {
    pub version: u32,
    pub groups: Vec<SnippetGroup>,
}

#[derive(Debug, Clone)]
pub struct DropTextImport {
    pub snippets: SnippetFile,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SnippetGroup {
    pub name: String,
    pub snippets: Vec<Snippet>,
    #[serde(default)]
    pub sort_order: SnippetSortOrder,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum SnippetSortOrder {
    #[default]
    Custom,
    AlphabeticalAscending,
    AlphabeticalDescending,
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
    #[serde(default = "default_windows_character_delay_ms")]
    pub windows_character_delay_ms: u64,
    #[serde(default = "default_windows_separator_delay_ms")]
    pub windows_separator_delay_ms: u64,
    #[serde(default = "default_close_after_insert")]
    pub close_after_insert: bool,
    #[serde(default = "default_start_snippets_on_new_line")]
    pub start_snippets_on_new_line: bool,
    #[serde(default = "default_empty_lines_between_snippets")]
    pub empty_lines_between_snippets: u32,
    #[serde(default, alias = "startWithWindows")]
    pub open_on_startup: bool,
    #[serde(default = "default_theme")]
    pub theme: String,
    #[serde(default = "default_accent_color")]
    pub accent_color: String,
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
                sort_order: SnippetSortOrder::Custom,
            }],
        }
    }
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            hotkey: default_hotkey(),
            typing_delay_ms: default_typing_delay_ms(),
            windows_character_delay_ms: default_windows_character_delay_ms(),
            windows_separator_delay_ms: default_windows_separator_delay_ms(),
            close_after_insert: default_close_after_insert(),
            start_snippets_on_new_line: default_start_snippets_on_new_line(),
            empty_lines_between_snippets: default_empty_lines_between_snippets(),
            open_on_startup: false,
            theme: default_theme(),
            accent_color: default_accent_color(),
            queued_snippet_click_action: default_queued_snippet_click_action(),
            check_for_updates: default_check_for_updates(),
            last_update_check_unix: None,
        }
    }
}

impl PortablePaths {
    pub fn strictly_beside_executable() -> Result<Self> {
        let exe = std::env::current_exe().context("Could not determine current executable path")?;
        let app_dir = exe
            .parent()
            .ok_or_else(|| anyhow!("Could not determine executable directory"))?;
        Self::strictly_from_app_dir(app_dir)
    }

    fn strictly_from_app_dir(app_dir: &Path) -> Result<Self> {
        let paths = Self::from_app_dir(app_dir);
        if !writable_data_dir(&paths.data_dir) {
            return Err(anyhow!(
                "The portable data folder is not writable: {}",
                paths.data_dir.display()
            ));
        }
        Ok(paths)
    }

    pub fn beside_executable() -> Result<Self> {
        let exe = std::env::current_exe().context("Could not determine current executable path")?;
        let app_dir = exe
            .parent()
            .ok_or_else(|| anyhow!("Could not determine executable directory"))?;
        if cfg!(windows) && is_windows_installed_app_dir(app_dir) {
            let data_dir =
                platform_data_dir().ok_or_else(|| anyhow!("Could not locate user data folder"))?;
            let user_paths = Self::from_data_dir(data_dir);
            copy_seed_data_if_missing(&Self::from_app_dir(app_dir), &user_paths);
            return Ok(user_paths);
        }

        let portable_paths = Self::from_app_dir(app_dir);
        if !is_macos_app_bundle_executable_dir(app_dir)
            && writable_data_dir(&portable_paths.data_dir)
        {
            return Ok(portable_paths);
        }

        let data_dir = platform_data_dir().ok_or_else(|| {
            anyhow!("Could not create portable data folder or locate user data folder")
        })?;
        Ok(Self::from_data_dir(data_dir))
    }

    pub fn from_app_dir(app_dir: impl AsRef<Path>) -> Self {
        let data_dir = app_dir.as_ref().join("data");
        Self::from_data_dir(data_dir)
    }

    pub fn from_data_dir(data_dir: impl Into<PathBuf>) -> Self {
        let data_dir = data_dir.into();
        Self {
            snippets_path: data_dir.join("snippets.json"),
            settings_path: data_dir.join("settings.json"),
            data_dir,
        }
    }
}

fn writable_data_dir(data_dir: &Path) -> bool {
    if fs::create_dir_all(data_dir).is_err() {
        return false;
    }

    let probe_path = data_dir.join(".typetext-write-test");
    match fs::write(&probe_path, b"") {
        Ok(()) => {
            let _ = fs::remove_file(probe_path);
            true
        }
        Err(_) => false,
    }
}

fn is_windows_installed_app_dir(app_dir: &Path) -> bool {
    if !cfg!(windows) {
        return false;
    }

    ["ProgramFiles", "ProgramFiles(x86)"]
        .iter()
        .filter_map(std::env::var_os)
        .map(PathBuf::from)
        .any(|program_files| app_dir.starts_with(program_files))
}

fn copy_seed_data_if_missing(source: &PortablePaths, target: &PortablePaths) {
    let _ = fs::create_dir_all(&target.data_dir);
    for (source_path, target_path) in [
        (&source.snippets_path, &target.snippets_path),
        (&source.settings_path, &target.settings_path),
    ] {
        if source_path.exists() && !target_path.exists() {
            let _ = fs::copy(source_path, target_path);
        }
    }
}

fn is_macos_app_bundle_executable_dir(app_dir: &Path) -> bool {
    if !cfg!(target_os = "macos") {
        return false;
    }

    let contents_dir = match app_dir.parent() {
        Some(path) if app_dir.file_name().is_some_and(|name| name == "MacOS") => path,
        _ => return false,
    };
    let bundle_dir = match contents_dir.parent() {
        Some(path)
            if contents_dir
                .file_name()
                .is_some_and(|name| name == "Contents") =>
        {
            path
        }
        _ => return false,
    };

    bundle_dir
        .extension()
        .is_some_and(|extension| extension == "app")
}

pub fn load_or_create_snippets(paths: &PortablePaths) -> Result<SnippetFile> {
    load_or_create_json(
        &paths.snippets_path,
        &SnippetFile::default(),
        MAX_SNIPPET_FILE_BYTES,
    )
    .and_then(|file| {
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
    let settings = load_or_create_json(
        &paths.settings_path,
        &AppSettings::default(),
        MAX_SETTINGS_FILE_BYTES,
    )?;
    validate_settings(&settings)?;
    Ok(settings)
}

pub fn save_settings(paths: &PortablePaths, settings: &AppSettings) -> Result<()> {
    validate_settings(settings)?;
    save_json(paths.settings_path.as_path(), settings)
}

pub fn import_droptext(path: impl AsRef<Path>) -> Result<SnippetFile> {
    Ok(import_droptext_with_warnings(path)?.snippets)
}

pub fn import_droptext_with_warnings(path: impl AsRef<Path>) -> Result<DropTextImport> {
    let path = path.as_ref();
    let bytes = read_limited(path, MAX_SNIPPET_FILE_BYTES)?;
    let raw = decode_droptext_text(&bytes);
    let is_csv = path
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("csv"));

    if is_csv {
        parse_droptext_csv_with_warnings(&raw)
    } else {
        parse_droptext_ini(&raw).map(|snippets| DropTextImport {
            snippets,
            warnings: Vec::new(),
        })
    }
    .with_context(|| format!("Could not parse {}", path.display()))
}

pub fn parse_droptext_ini(raw: &str) -> Result<SnippetFile> {
    parse_droptext_data(raw, false, &mut Vec::new())
}

pub fn parse_droptext_csv(raw: &str) -> Result<SnippetFile> {
    Ok(parse_droptext_csv_with_warnings(raw)?.snippets)
}

fn parse_droptext_csv_with_warnings(raw: &str) -> Result<DropTextImport> {
    let records = parse_csv_records(raw.trim_start_matches('\u{feff}'))?;
    if records.is_empty() {
        return Err(anyhow!("The DropText CSV file is empty."));
    }

    let mut droptext_data = String::new();
    for (record_index, record) in records.into_iter().enumerate() {
        if record.len() != 7 {
            return Err(anyhow!(
                "CSV record {} has {} fields; expected 7",
                record_index + 1,
                record.len()
            ));
        }
        if !droptext_data.is_empty() && !droptext_data.ends_with(['\r', '\n']) {
            droptext_data.push('\n');
        }
        droptext_data.push_str(&record[2]);
    }

    let mut warnings = Vec::new();
    let snippets = parse_droptext_data(&droptext_data, true, &mut warnings)?;
    Ok(DropTextImport { snippets, warnings })
}

pub fn expand_snippet_tokens(body: &str) -> String {
    expand_snippet_tokens_at(body, Local::now().fixed_offset())
}

fn expand_snippet_tokens_at(body: &str, now: DateTime<FixedOffset>) -> String {
    let today = now.date_naive();
    let tomorrow = today.succ_opt().unwrap_or(today);
    let yesterday = today.pred_opt().unwrap_or(today);
    let replacements = [
        ("time.today", now.format("%H:%M").to_string()),
        ("time.now", now.format("%H:%M").to_string()),
        ("date.today", today.format("%d/%m/%Y").to_string()),
        ("date.tomorrow", tomorrow.format("%d/%m/%Y").to_string()),
        ("date.yesterday", yesterday.format("%d/%m/%Y").to_string()),
        ("datetime.now", now.format("%d/%m/%Y %H:%M").to_string()),
        ("weekday.today", today.format("%A").to_string()),
    ];

    let mut expanded = String::with_capacity(body.len());
    let mut offset = 0;
    while offset < body.len() {
        let remaining = &body[offset..];

        if let Some(after_opening) = remaining.strip_prefix("{{")
            && let Some(end) = after_opening.find("}}")
        {
            expanded.push('{');
            expanded.push_str(&after_opening[..end]);
            expanded.push('}');
            offset += 2 + end + 2;
            continue;
        }

        if let Some(after_opening) = remaining.strip_prefix('{')
            && let Some(end) = after_opening.find('}')
        {
            let token = &after_opening[..end];
            if let Some((_, value)) = replacements.iter().find(|(name, _)| *name == token) {
                expanded.push_str(value);
                offset += 1 + end + 1;
                continue;
            }
        }

        let character = remaining
            .chars()
            .next()
            .expect("remaining text is not empty");
        expanded.push(character);
        offset += character.len_utf8();
    }

    expanded
}

fn parse_droptext_data(
    raw: &str,
    decode_unquoted_escapes: bool,
    warnings: &mut Vec<String>,
) -> Result<SnippetFile> {
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
                    sort_order: SnippetSortOrder::Custom,
                });
                current_group = Some(groups.len() - 1);
            }
            continue;
        }

        let (separator, value_start, repaired_missing_separator) =
            if let Some(separator) = line.find('=') {
                (separator, separator + 1, false)
            } else if decode_unquoted_escapes {
                let opening_quote = line
                    .find('"')
                    .ok_or_else(|| anyhow!("Line {line_number}: expected key=value entry"))?;
                (opening_quote, opening_quote, true)
            } else {
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

        let body = parse_droptext_value(
            line[value_start..].trim(),
            line_number,
            decode_unquoted_escapes,
        )?;
        if repaired_missing_separator {
            warnings.push(format!(
                "Line {line_number}: inserted missing = before quoted value for \"{title}\"."
            ));
        }
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
            let mut matches: Vec<_> = group
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
                .collect();
            match group.sort_order {
                SnippetSortOrder::Custom => {}
                SnippetSortOrder::AlphabeticalAscending => {
                    matches.sort_by_key(|result| result.title.to_lowercase())
                }
                SnippetSortOrder::AlphabeticalDescending => {
                    matches.sort_by_key(|result| std::cmp::Reverse(result.title.to_lowercase()))
                }
            }
            matches
        })
        .collect()
}

pub fn validate_snippets(snippets: &SnippetFile) -> Result<()> {
    if snippets.groups.len() > MAX_GROUPS {
        return Err(anyhow!(
            "Snippet data contains more than {MAX_GROUPS} groups."
        ));
    }

    let mut snippet_count = 0usize;
    for group in &snippets.groups {
        if group.name.trim().is_empty() {
            return Err(anyhow!("Every group needs a name."));
        }
        if group.name.chars().count() > MAX_GROUP_NAME_CHARS {
            return Err(anyhow!(
                "Group names cannot exceed {MAX_GROUP_NAME_CHARS} characters."
            ));
        }

        for snippet in &group.snippets {
            snippet_count = snippet_count
                .checked_add(1)
                .ok_or_else(|| anyhow!("Snippet count overflowed."))?;
            if snippet_count > MAX_SNIPPETS {
                return Err(anyhow!(
                    "Snippet data contains more than {MAX_SNIPPETS} snippets."
                ));
            }
            if snippet.title.trim().is_empty() {
                return Err(anyhow!("Every snippet needs a title."));
            }
            if snippet.title.chars().count() > MAX_SNIPPET_TITLE_CHARS {
                return Err(anyhow!(
                    "Snippet titles cannot exceed {MAX_SNIPPET_TITLE_CHARS} characters."
                ));
            }
            if snippet.body.chars().count() > MAX_SNIPPET_BODY_CHARS {
                return Err(anyhow!(
                    "Snippet bodies cannot exceed {MAX_SNIPPET_BODY_CHARS} characters."
                ));
            }
        }
    }

    Ok(())
}

pub fn validate_settings(settings: &AppSettings) -> Result<()> {
    if settings.typing_delay_ms > MAX_TYPING_DELAY_MS {
        return Err(anyhow!(
            "Typing delay cannot exceed {MAX_TYPING_DELAY_MS} milliseconds."
        ));
    }
    if settings.windows_character_delay_ms > MAX_WINDOWS_INPUT_DELAY_MS
        || settings.windows_separator_delay_ms > MAX_WINDOWS_INPUT_DELAY_MS
    {
        return Err(anyhow!(
            "Windows input delays cannot exceed {MAX_WINDOWS_INPUT_DELAY_MS} milliseconds."
        ));
    }
    if settings.empty_lines_between_snippets > MAX_EMPTY_LINES_BETWEEN_SNIPPETS {
        return Err(anyhow!(
            "Empty lines between snippets cannot exceed {MAX_EMPTY_LINES_BETWEEN_SNIPPETS}."
        ));
    }
    if settings.hotkey.len() > 128 {
        return Err(anyhow!("The hotkey value is too long."));
    }
    if settings.theme.len() > 32 || settings.accent_color.len() > 32 {
        return Err(anyhow!("A display setting is too long."));
    }
    Ok(())
}

fn load_or_create_json<T>(path: &Path, default_value: &T, max_bytes: u64) -> Result<T>
where
    T: Serialize + for<'de> Deserialize<'de> + Clone,
{
    if !path.exists() {
        save_json(path, default_value)?;
        return Ok(default_value.clone());
    }

    let bytes = read_limited(path, max_bytes)?;
    let raw = String::from_utf8(bytes)
        .with_context(|| format!("{} is not valid UTF-8", path.display()))?;
    serde_json::from_str(&raw).with_context(|| format!("Could not parse {}", path.display()))
}

fn read_limited(path: &Path, max_bytes: u64) -> Result<Vec<u8>> {
    let file =
        fs::File::open(path).with_context(|| format!("Could not read {}", path.display()))?;
    let mut reader: Take<fs::File> = file.take(max_bytes.saturating_add(1));
    let mut bytes = Vec::new();
    reader
        .read_to_end(&mut bytes)
        .with_context(|| format!("Could not read {}", path.display()))?;
    if bytes.len() as u64 > max_bytes {
        return Err(anyhow!(
            "{} exceeds the {} byte safety limit.",
            path.display(),
            max_bytes
        ));
    }
    Ok(bytes)
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
    static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("typetext.json");
    let mut temp_file = None;
    for _ in 0..16 {
        let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let candidate = parent.join(format!(
            ".{file_name}.{}.{}.tmp",
            std::process::id(),
            sequence
        ));
        match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&candidate)
        {
            Ok(file) => {
                temp_file = Some((candidate, file));
                break;
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => {
                return Err(error).with_context(|| {
                    format!("Could not create a temporary file in {}", parent.display())
                });
            }
        }
    }
    let (temp_path, mut file) = temp_file.ok_or_else(|| {
        anyhow!(
            "Could not allocate a unique temporary file in {}",
            parent.display()
        )
    })?;
    if let Err(error) = file
        .write_all(format!("{raw}\n").as_bytes())
        .and_then(|_| file.sync_all())
    {
        let _ = fs::remove_file(&temp_path);
        return Err(error).with_context(|| format!("Could not write {}", temp_path.display()));
    }
    drop(file);
    if let Err(error) = fs::rename(&temp_path, path) {
        let _ = fs::remove_file(&temp_path);
        return Err(error).with_context(|| {
            format!(
                "Could not move {} to {}",
                temp_path.display(),
                path.display()
            )
        });
    }
    Ok(())
}

fn platform_data_dir() -> Option<PathBuf> {
    if cfg!(windows) {
        std::env::var_os("LOCALAPPDATA")
            .or_else(|| std::env::var_os("APPDATA"))
            .map(PathBuf::from)
            .map(|path| path.join("TypeText").join("data"))
    } else if cfg!(target_os = "macos") {
        std::env::var_os("HOME").map(|home| {
            PathBuf::from(home)
                .join("Library")
                .join("Application Support")
                .join("TypeText")
                .join("data")
        })
    } else {
        std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .or_else(|| {
                std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/share"))
            })
            .map(|path| path.join("typetext").join("data"))
    }
}

fn parse_droptext_value(
    raw: &str,
    line_number: usize,
    decode_unquoted_escapes: bool,
) -> Result<String> {
    let Some(unquoted) = raw.strip_prefix('"') else {
        let value = raw.trim();
        return if decode_unquoted_escapes {
            Ok(decode_droptext_escapes(value))
        } else {
            Ok(value.to_string())
        };
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

    Ok(normalize_newlines(value))
}

fn decode_droptext_escapes(raw: &str) -> String {
    let mut value = String::new();
    let mut chars = raw.chars();
    while let Some(ch) = chars.next() {
        if ch != '\\' {
            value.push(ch);
            continue;
        }

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
    normalize_newlines(value)
}

fn normalize_newlines(value: String) -> String {
    value.replace("\r\n", "\n").replace('\r', "\n")
}

fn decode_droptext_text(bytes: &[u8]) -> String {
    if let Ok(utf8) = std::str::from_utf8(bytes) {
        return utf8.to_string();
    }

    bytes
        .iter()
        .map(|&byte| match byte {
            0x80 => '\u{20ac}',
            0x82 => '\u{201a}',
            0x83 => '\u{0192}',
            0x84 => '\u{201e}',
            0x85 => '\u{2026}',
            0x86 => '\u{2020}',
            0x87 => '\u{2021}',
            0x88 => '\u{02c6}',
            0x89 => '\u{2030}',
            0x8a => '\u{0160}',
            0x8b => '\u{2039}',
            0x8c => '\u{0152}',
            0x8e => '\u{017d}',
            0x91 => '\u{2018}',
            0x92 => '\u{2019}',
            0x93 => '\u{201c}',
            0x94 => '\u{201d}',
            0x95 => '\u{2022}',
            0x96 => '\u{2013}',
            0x97 => '\u{2014}',
            0x98 => '\u{02dc}',
            0x99 => '\u{2122}',
            0x9a => '\u{0161}',
            0x9b => '\u{203a}',
            0x9c => '\u{0153}',
            0x9e => '\u{017e}',
            0x9f => '\u{0178}',
            _ => char::from(byte),
        })
        .collect()
}

fn parse_csv_records(raw: &str) -> Result<Vec<Vec<String>>> {
    let mut records = Vec::new();
    let mut record = Vec::new();
    let mut field = String::new();
    let mut chars = raw.chars().peekable();
    let mut in_quotes = false;
    let mut field_started = false;
    let mut quote_closed = false;

    while let Some(ch) = chars.next() {
        if in_quotes {
            if ch == '"' {
                if chars.peek() == Some(&'"') {
                    chars.next();
                    field.push('"');
                } else {
                    in_quotes = false;
                    quote_closed = true;
                }
            } else {
                field.push(ch);
            }
            continue;
        }

        if quote_closed && !matches!(ch, ',' | '\r' | '\n') {
            return Err(anyhow!("Unexpected text after a closing CSV quote"));
        }

        match ch {
            '"' if !field_started => {
                in_quotes = true;
                field_started = true;
            }
            ',' => {
                record.push(std::mem::take(&mut field));
                field_started = false;
                quote_closed = false;
            }
            '\r' | '\n' => {
                if ch == '\r' && chars.peek() == Some(&'\n') {
                    chars.next();
                }
                record.push(std::mem::take(&mut field));
                if record.iter().any(|value| !value.is_empty()) {
                    records.push(std::mem::take(&mut record));
                } else {
                    record.clear();
                }
                field_started = false;
                quote_closed = false;
            }
            other => {
                field.push(other);
                field_started = true;
            }
        }
    }

    if in_quotes {
        return Err(anyhow!("CSV field is missing its closing quote"));
    }
    if field_started || quote_closed || !record.is_empty() {
        record.push(field);
        if record.iter().any(|value| !value.is_empty()) {
            records.push(record);
        }
    }

    Ok(records)
}

fn default_hotkey() -> String {
    "Ctrl+Alt+Space".to_string()
}

fn default_typing_delay_ms() -> u64 {
    DEFAULT_TYPING_DELAY_MS
}

fn default_windows_character_delay_ms() -> u64 {
    DEFAULT_WINDOWS_CHARACTER_DELAY_MS
}

fn default_windows_separator_delay_ms() -> u64 {
    DEFAULT_WINDOWS_SEPARATOR_DELAY_MS
}

fn default_close_after_insert() -> bool {
    true
}

fn default_start_snippets_on_new_line() -> bool {
    false
}

fn default_empty_lines_between_snippets() -> u32 {
    DEFAULT_EMPTY_LINES_BETWEEN_SNIPPETS
}

fn default_theme() -> String {
    "system".to_string()
}

fn default_accent_color() -> String {
    "#0A7E76".to_string()
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
    fn rejects_settings_outside_runtime_safety_limits() {
        let settings = AppSettings {
            typing_delay_ms: MAX_TYPING_DELAY_MS + 1,
            ..AppSettings::default()
        };
        assert!(validate_settings(&settings).is_err());

        let settings = AppSettings {
            windows_character_delay_ms: MAX_WINDOWS_INPUT_DELAY_MS + 1,
            ..AppSettings::default()
        };
        assert!(validate_settings(&settings).is_err());

        let settings = AppSettings {
            empty_lines_between_snippets: MAX_EMPTY_LINES_BETWEEN_SNIPPETS + 1,
            ..AppSettings::default()
        };
        assert!(validate_settings(&settings).is_err());
    }

    #[test]
    fn rejects_oversized_snippet_content() {
        let mut snippets = SnippetFile::default();
        snippets.groups[0].snippets[0].body = "x".repeat(MAX_SNIPPET_BODY_CHARS + 1);

        assert!(validate_snippets(&snippets).is_err());
    }

    #[test]
    fn search_respects_each_groups_alphabetical_sort_order() {
        let mut snippets = SnippetFile::default();
        snippets.groups[0].sort_order = SnippetSortOrder::AlphabeticalAscending;

        let ascending: Vec<_> = search_snippets(&snippets, "")
            .into_iter()
            .map(|result| result.title)
            .collect();
        assert_eq!(ascending, ["Follow up", "Thanks"]);

        snippets.groups[0].sort_order = SnippetSortOrder::AlphabeticalDescending;
        let descending: Vec<_> = search_snippets(&snippets, "")
            .into_iter()
            .map(|result| result.title)
            .collect();
        assert_eq!(descending, ["Thanks", "Follow up"]);
    }

    #[test]
    fn snippet_files_without_sort_order_default_to_custom() {
        let snippets: SnippetFile =
            serde_json::from_str(r#"{"version":1,"groups":[{"name":"Existing","snippets":[]}]}"#)
                .unwrap();

        assert_eq!(snippets.groups[0].sort_order, SnippetSortOrder::Custom);
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

    #[test]
    fn parse_droptext_csv_extracts_groups_and_decodes_body_content() {
        let raw = concat!(
            "<new>,852,\"[Common]\n",
            "Greeting=Hello \\r\\n\\r\\nWorld\n",
            "Quote=Use \"\"Show Markup\"\" here\n\n",
            "[Other]\n",
            "Path=Keep C:\\unknown intact\",680,997,1438,468\r\n",
        );

        let snippets = parse_droptext_csv(raw).unwrap();

        assert_eq!(snippets.groups.len(), 2);
        assert_eq!(snippets.groups[0].name, "Common");
        assert_eq!(snippets.groups[0].snippets.len(), 2);
        assert_eq!(snippets.groups[0].snippets[0].title, "Greeting");
        assert_eq!(snippets.groups[0].snippets[0].body, "Hello \n\nWorld");
        assert_eq!(
            snippets.groups[0].snippets[1].body,
            "Use \"Show Markup\" here"
        );
        assert_eq!(
            snippets.groups[1].snippets[0].body,
            r"Keep C:\unknown intact"
        );
    }

    #[test]
    fn parse_droptext_csv_recovers_missing_equals_before_quoted_value() {
        let raw = concat!(
            "<new>,852,\"[Common]\n",
            "Extension Last Log\"\"Quoted body\"\"\",680,997,1438,468\r\n",
        );

        let imported = parse_droptext_csv_with_warnings(raw).unwrap();
        let snippets = imported.snippets;

        assert_eq!(snippets.groups.len(), 1);
        assert_eq!(snippets.groups[0].snippets.len(), 1);
        assert_eq!(snippets.groups[0].snippets[0].title, "Extension Last Log");
        assert_eq!(snippets.groups[0].snippets[0].body, "Quoted body");
        assert_eq!(
            imported.warnings,
            ["Line 2: inserted missing = before quoted value for \"Extension Last Log\"."]
        );
    }

    #[test]
    fn parse_droptext_csv_reports_wrong_field_count() {
        let error = parse_droptext_csv("<new>,852,\"[Common]\nOne=First\",680\r\n").unwrap_err();

        assert!(error.to_string().contains("expected 7"));
    }

    #[test]
    fn droptext_text_decoder_falls_back_to_windows_1252() {
        assert_eq!(decode_droptext_text(b"It\x92s ready"), "It’s ready");
    }

    #[test]
    fn expands_dynamic_snippet_tokens_using_one_timestamp() {
        let now = DateTime::parse_from_rfc3339("2026-06-20T17:42:31+10:00").unwrap();
        let body = concat!(
            "{time.today}|{time.now}|{date.today}|{date.tomorrow}|",
            "{date.yesterday}|{datetime.now}|{weekday.today}",
        );

        assert_eq!(
            expand_snippet_tokens_at(body, now),
            "17:42|17:42|20/06/2026|21/06/2026|19/06/2026|20/06/2026 17:42|Saturday"
        );
    }

    #[test]
    fn preserves_unknown_tokens_and_unescapes_literal_braces() {
        let now = DateTime::parse_from_rfc3339("2026-06-20T17:42:31+10:00").unwrap();

        assert_eq!(
            expand_snippet_tokens_at(
                "Known: {date.today}; unknown: {person.name}; literal: {{date.today}}; café",
                now,
            ),
            "Known: 20/06/2026; unknown: {person.name}; literal: {date.today}; café"
        );
    }

    #[test]
    fn detects_macos_app_bundle_executable_dir() {
        let app_dir = Path::new("/Applications/TypeText.app/Contents/MacOS");

        assert_eq!(
            is_macos_app_bundle_executable_dir(app_dir),
            cfg!(target_os = "macos")
        );
        assert!(!is_macos_app_bundle_executable_dir(Path::new(
            "/tmp/TypeText/MacOS"
        )));
    }

    #[test]
    fn detects_windows_installed_app_dir_only_on_windows() {
        let app_dir = Path::new(r"C:\Program Files\TypeText");

        assert_eq!(is_windows_installed_app_dir(app_dir), cfg!(windows));
    }

    #[test]
    fn strict_portable_paths_never_fall_back_from_the_app_directory() {
        let base = std::env::temp_dir().join(format!(
            "typetext-strict-path-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&base).unwrap();
        fs::write(base.join("data"), "blocks directory creation").unwrap();

        let error = PortablePaths::strictly_from_app_dir(&base).unwrap_err();

        assert!(error.to_string().contains("portable data folder"));
        let _ = fs::remove_dir_all(base);
    }

    #[test]
    fn copy_seed_data_only_fills_missing_user_files() {
        let base = std::env::temp_dir().join(format!(
            "typetext-seed-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let source = PortablePaths::from_data_dir(base.join("source"));
        let target = PortablePaths::from_data_dir(base.join("target"));
        fs::create_dir_all(&source.data_dir).unwrap();
        fs::create_dir_all(&target.data_dir).unwrap();
        fs::write(&source.snippets_path, "source snippets").unwrap();
        fs::write(&source.settings_path, "source settings").unwrap();
        fs::write(&target.settings_path, "user settings").unwrap();

        copy_seed_data_if_missing(&source, &target);

        assert_eq!(
            fs::read_to_string(&target.snippets_path).unwrap(),
            "source snippets"
        );
        assert_eq!(
            fs::read_to_string(&target.settings_path).unwrap(),
            "user settings"
        );
        let _ = fs::remove_dir_all(base);
    }
}
