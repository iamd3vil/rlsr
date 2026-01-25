use async_trait::async_trait;
use color_eyre::eyre::Result;
use color_eyre::eyre::WrapErr;
use minijinja::Environment;
use serde::Serialize;

use crate::templating;
use crate::utils;
use crate::TemplateMeta;

mod default_formatter;
mod github;

#[derive(Debug, Serialize, Clone)]
pub struct Commit {
    pub hash: String,
    pub subject: String,
    pub email: String,
    pub handle: Option<String>, // Will be used in GH formatter.
    #[serde(rename = "type")]
    pub commit_type: Option<String>,
    pub scope: Option<String>,
    pub breaking: bool,
}

impl Commit {
    pub fn from_parts(hash: String, subject: String, email: String) -> Self {
        let (commit_type, scope, breaking) = parse_conventional_subject(&subject);
        Self {
            hash,
            subject,
            email,
            handle: None,
            commit_type,
            scope,
            breaking,
        }
    }
}

#[async_trait]
#[allow(clippy::needless_arbitrary_self_type)]
pub trait Formatter: Send {
    async fn format(&self, commits: &[Commit], meta: &TemplateMeta) -> Result<String>;
}

pub async fn get_new_formatter(
    format: &str,
    tmpl: Option<String>,
) -> Result<Box<dyn Formatter + Send>> {
    match format {
        "github" => get_github_formatter(tmpl).await,
        _ => get_default_formatter(tmpl).await,
    }
}

/// Creates a new instance of the default formatter
pub async fn get_default_formatter(tmpl: Option<String>) -> Result<Box<dyn Formatter + Send>> {
    Ok(Box::new(
        default_formatter::DefaultFormatter::new(tmpl).await?,
    ))
}

pub async fn get_github_formatter(tmpl: Option<String>) -> Result<Box<dyn Formatter + Send>> {
    let token = utils::get_github_token();
    Ok(Box::new(github::GithubFormatter::new(token, tmpl).await?))
}

pub fn get_minijinja_env(content: String) -> Result<Environment<'static>> {
    let mut env = Environment::new();
    env.add_template_owned("tmpl", content)
        .wrap_err("error adding template")?;
    templating::add_string_filters(&mut env);
    env.add_filter("starts_with", starts_with_filter);
    env.add_filter("ends_with", ends_with_filter);
    env.add_filter("trim", trim_filter);
    env.add_filter("contains", contains_filter);
    env.set_lstrip_blocks(true);
    env.set_trim_blocks(true);
    Ok(env)
}

fn starts_with_filter(value: String, prefix: String) -> bool {
    value.starts_with(&prefix)
}

fn ends_with_filter(value: String, suffix: String) -> bool {
    value.ends_with(&suffix)
}

fn trim_filter(value: String, chars: String) -> String {
    if chars.is_empty() {
        return value.trim().to_string();
    }
    value.trim_matches(|c| chars.contains(c)).to_string()
}

fn contains_filter(value: String, needle: String) -> bool {
    value.contains(&needle)
}

fn parse_conventional_subject(subject: &str) -> (Option<String>, Option<String>, bool) {
    let trimmed = subject.trim();
    let (head, _rest) = match trimmed.split_once(':') {
        Some(parts) => parts,
        None => return (None, None, false),
    };
    let head = head.trim();
    if head.is_empty() {
        return (None, None, false);
    }

    let (breaking, head) = match head.strip_suffix('!') {
        Some(stripped) => (true, stripped),
        None => (false, head),
    };

    if let Some(open_idx) = head.find('(') {
        let close_idx = match head[open_idx + 1..].find(')') {
            Some(idx) => open_idx + 1 + idx,
            None => return (None, None, breaking),
        };
        if close_idx != head.len() - 1 {
            return (None, None, breaking);
        }
        let commit_type = head[..open_idx].trim();
        let scope = head[open_idx + 1..close_idx].trim();
        if commit_type.is_empty() || scope.is_empty() {
            return (None, None, breaking);
        }
        return (
            Some(commit_type.to_string()),
            Some(scope.to_string()),
            breaking,
        );
    }

    let commit_type = head.trim();
    if commit_type.is_empty() {
        return (None, None, breaking);
    }
    (Some(commit_type.to_string()), None, breaking)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_conventional_subject_with_scope() {
        let (commit_type, scope, breaking) =
            parse_conventional_subject("feat(parser): add support");
        assert_eq!(commit_type.as_deref(), Some("feat"));
        assert_eq!(scope.as_deref(), Some("parser"));
        assert!(!breaking);
    }

    #[test]
    fn test_parse_conventional_subject_breaking_variants() {
        let (commit_type, scope, breaking) = parse_conventional_subject("fix!: change api");
        assert_eq!(commit_type.as_deref(), Some("fix"));
        assert!(scope.is_none());
        assert!(breaking);

        let (commit_type, scope, breaking) = parse_conventional_subject("docs(readme)!: update");
        assert_eq!(commit_type.as_deref(), Some("docs"));
        assert_eq!(scope.as_deref(), Some("readme"));
        assert!(breaking);
    }

    #[test]
    fn test_parse_conventional_subject_non_conforming() {
        let (commit_type, scope, breaking) = parse_conventional_subject("Merge branch main");
        assert!(commit_type.is_none());
        assert!(scope.is_none());
        assert!(!breaking);
    }

    #[test]
    fn test_trim_and_contains_filters() {
        assert_eq!(
            trim_filter("**hello**".to_string(), "*".to_string()),
            "hello"
        );
        assert_eq!(
            trim_filter("  hello  ".to_string(), "".to_string()),
            "hello"
        );
        assert!(contains_filter("hello".to_string(), "ell".to_string()));
        assert!(!contains_filter("hello".to_string(), "nope".to_string()));
    }
}
