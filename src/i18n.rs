use anyhow::{Context, Result};
use std::{collections::BTreeMap, path::PathBuf, sync::OnceLock};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Language {
    English,
    Korean,
    Japanese,
}

impl Language {
    pub fn code(self) -> &'static str {
        match self {
            Self::English => "en",
            Self::Korean => "ko",
            Self::Japanese => "ja",
        }
    }

    fn from_code(code: &str) -> Option<Self> {
        match code {
            "en" => Some(Self::English),
            "ko" => Some(Self::Korean),
            "ja" => Some(Self::Japanese),
            _ => None,
        }
    }
}

type Messages = BTreeMap<String, String>;
static ENGLISH: OnceLock<Messages> = OnceLock::new();
static KOREAN: OnceLock<Messages> = OnceLock::new();
static JAPANESE: OnceLock<Messages> = OnceLock::new();

fn messages(language: Language) -> &'static Messages {
    match language {
        Language::English => ENGLISH.get_or_init(|| {
            serde_json::from_str(include_str!("../resources/i18n/en.json"))
                .expect("English translations must be valid JSON")
        }),
        Language::Korean => KOREAN.get_or_init(|| {
            serde_json::from_str(include_str!("../resources/i18n/ko.json"))
                .expect("Korean translations must be valid JSON")
        }),
        Language::Japanese => JAPANESE.get_or_init(|| {
            serde_json::from_str(include_str!("../resources/i18n/ja.json"))
                .expect("Japanese translations must be valid JSON")
        }),
    }
}

pub fn tr(language: Language, key: &'static str) -> &'static str {
    messages(language)
        .get(key)
        .or_else(|| messages(Language::English).get(key))
        .map(String::as_str)
        .unwrap_or(key)
}

pub fn format(language: Language, key: &'static str, values: &[(&str, &str)]) -> String {
    let mut text = tr(language, key).to_owned();
    for (name, value) in values {
        text = text.replace(&format!("{{{name}}}"), value);
    }
    text
}

fn settings_path() -> Result<PathBuf> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .filter(|path| !path.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))
        .context("No user configuration directory is available")?;
    Ok(base.join("aircard/settings.json"))
}

pub fn load_language() -> Language {
    settings_path()
        .and_then(|path| Ok(std::fs::read_to_string(path)?))
        .ok()
        .and_then(|data| parse_saved_language(&data))
        .unwrap_or(Language::English)
}

fn parse_saved_language(data: &str) -> Option<Language> {
    let value: serde_json::Value = serde_json::from_str(data).ok()?;
    value
        .get("language")?
        .as_str()
        .and_then(Language::from_code)
}

pub fn save_language(language: Language) -> Result<()> {
    let path = settings_path()?;
    std::fs::create_dir_all(path.parent().context("Invalid settings path")?)?;
    let data = serde_json::to_vec_pretty(&serde_json::json!({ "language": language.code() }))?;
    std::fs::write(&path, data).with_context(|| format!("Save settings at {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn translation_files_have_matching_keys() {
        let english: Vec<_> = messages(Language::English).keys().collect();
        for language in [Language::Korean, Language::Japanese] {
            assert_eq!(english, messages(language).keys().collect::<Vec<_>>());
        }
    }

    #[test]
    fn language_codes_and_interpolation_work() {
        assert_eq!(Language::from_code("ko"), Some(Language::Korean));
        assert_eq!(Language::from_code("unknown"), None);
        assert_eq!(
            format(Language::English, "activity.hash_found", &[("hash", "abc")]),
            "Card hash found: abc"
        );
        assert_eq!(
            parse_saved_language(r#"{"language":"ja"}"#),
            Some(Language::Japanese)
        );
        assert_eq!(parse_saved_language(r#"{"language":"fr"}"#), None);
    }
}
