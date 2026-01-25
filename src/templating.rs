use chrono::{DateTime, TimeZone, Utc};
use minijinja::{context, Environment};
use semver::{BuildMetadata, Prerelease, Version};
use serde::Serialize;
use std::collections::HashMap;
use std::fmt::Debug;

pub trait TemplateContext: Serialize + Debug {
    fn env(&self) -> &HashMap<String, String>;
    fn date(&self) -> &str;
    fn timestamp(&self) -> &str;
    fn now(&self) -> &str;
}

pub fn render_template<T: TemplateContext>(tmpl: &str, meta: &T) -> String {
    let mut env = Environment::new();
    add_string_filters(&mut env);
    env.add_template("tmpl", tmpl).unwrap();
    let tpl = env.get_template("tmpl").unwrap();
    let ctx = context!(
        meta => meta,
        env => meta.env(),
        date => meta.date(),
        timestamp => meta.timestamp(),
        now => meta.now(),
    );
    tpl.render(ctx).unwrap()
}

pub fn render_envs<T: TemplateContext>(
    envs: &Option<Vec<String>>,
    meta: &T,
) -> Option<Vec<String>> {
    let envs = envs.as_ref()?;
    let rendered: Vec<String> = envs.iter().map(|env| render_template(env, meta)).collect();
    if rendered.is_empty() {
        None
    } else {
        Some(rendered)
    }
}

pub fn add_string_filters(env: &mut Environment) {
    env.add_filter("tolower", tolower_filter);
    env.add_filter("toupper", toupper_filter);
    env.add_filter("replace", replace_filter);
    env.add_filter("trimprefix", trimprefix_filter);
    env.add_filter("trimsuffix", trimsuffix_filter);
    env.add_filter("title", title_filter);
    env.add_filter("split", split_filter);
    env.add_filter("time", time_filter);
    env.add_filter("default", default_filter);
    env.add_filter("incmajor", incmajor_filter);
    env.add_filter("incminor", incminor_filter);
    env.add_filter("incpatch", incpatch_filter);
}

fn tolower_filter(value: String) -> String {
    value.to_lowercase()
}

fn toupper_filter(value: String) -> String {
    value.to_uppercase()
}

fn replace_filter(value: String, old: String, new: String) -> String {
    value.replace(&old, &new)
}

fn trimprefix_filter(value: String, prefix: String) -> String {
    value.strip_prefix(&prefix).unwrap_or(&value).to_string()
}

fn trimsuffix_filter(value: String, suffix: String) -> String {
    value.strip_suffix(&suffix).unwrap_or(&value).to_string()
}

fn title_filter(value: String) -> String {
    let mut out = String::new();
    for (index, word) in value.split_whitespace().enumerate() {
        if index > 0 {
            out.push(' ');
        }
        let mut chars = word.chars();
        if let Some(first) = chars.next() {
            out.extend(first.to_uppercase());
            out.push_str(&chars.as_str().to_lowercase());
        }
    }
    out
}

fn split_filter(value: String, sep: String) -> Vec<String> {
    if sep.is_empty() {
        return vec![value];
    }
    value.split(&sep).map(str::to_string).collect()
}

fn time_filter(value: String, fmt: String) -> String {
    if let Ok(dt) = DateTime::parse_from_rfc3339(&value) {
        return dt.with_timezone(&Utc).format(&fmt).to_string();
    }

    if let Ok(ts) = value.parse::<i64>() {
        if let chrono::LocalResult::Single(dt) = Utc.timestamp_opt(ts, 0) {
            return dt.format(&fmt).to_string();
        }
    }

    value
}

fn default_filter(value: String, fallback: String) -> String {
    if value.is_empty() {
        fallback
    } else {
        value
    }
}

fn incmajor_filter(value: String) -> String {
    inc_version(value, VersionBump::Major)
}

fn incminor_filter(value: String) -> String {
    inc_version(value, VersionBump::Minor)
}

fn incpatch_filter(value: String) -> String {
    inc_version(value, VersionBump::Patch)
}

enum VersionBump {
    Major,
    Minor,
    Patch,
}

fn inc_version(value: String, bump: VersionBump) -> String {
    let (prefix, raw) = strip_version_prefix(&value);
    let parsed = Version::parse(raw);
    let mut version = match parsed {
        Ok(version) => version,
        Err(_) => return value,
    };

    match bump {
        VersionBump::Major => {
            version.major = version.major.saturating_add(1);
            version.minor = 0;
            version.patch = 0;
        }
        VersionBump::Minor => {
            version.minor = version.minor.saturating_add(1);
            version.patch = 0;
        }
        VersionBump::Patch => {
            version.patch = version.patch.saturating_add(1);
        }
    }

    version.pre = Prerelease::EMPTY;
    version.build = BuildMetadata::EMPTY;

    format!("{prefix}{}", version)
}

fn strip_version_prefix(value: &str) -> (String, &str) {
    if let Some(rest) = value.strip_prefix('v') {
        return ("v".to_string(), rest);
    }
    if let Some(rest) = value.strip_prefix('V') {
        return ("V".to_string(), rest);
    }
    ("".to_string(), value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_string_filters() {
        assert_eq!(tolower_filter("AbC".to_string()), "abc");
        assert_eq!(toupper_filter("AbC".to_string()), "ABC");
        assert_eq!(
            replace_filter("a-b".to_string(), "-".to_string(), "_".to_string()),
            "a_b"
        );
        assert_eq!(
            trimprefix_filter("v1.2.3".to_string(), "v".to_string()),
            "1.2.3"
        );
        assert_eq!(
            trimsuffix_filter("app.exe".to_string(), ".exe".to_string()),
            "app"
        );
        assert_eq!(title_filter("hello WORLD".to_string()), "Hello World");
        assert_eq!(
            split_filter("a,b,c".to_string(), ",".to_string()),
            vec!["a", "b", "c"]
        );
        assert_eq!(
            split_filter("keep".to_string(), "".to_string()),
            vec!["keep"]
        );
        assert_eq!(
            default_filter("".to_string(), "stable".to_string()),
            "stable"
        );
        assert_eq!(
            default_filter("rc1".to_string(), "stable".to_string()),
            "rc1"
        );
    }

    #[test]
    fn test_time_filter_formats_rfc3339_and_timestamp() {
        assert_eq!(
            time_filter("2025-01-25T10:30:00Z".to_string(), "%Y-%m-%d".to_string()),
            "2025-01-25"
        );
        assert_eq!(
            time_filter("0".to_string(), "%Y-%m-%d".to_string()),
            "1970-01-01"
        );
        assert_eq!(
            time_filter("not-a-time".to_string(), "%Y".to_string()),
            "not-a-time"
        );
    }

    #[test]
    fn test_version_bump_filters() {
        assert_eq!(incmajor_filter("1.2.3".to_string()), "2.0.0");
        assert_eq!(incminor_filter("1.2.3".to_string()), "1.3.0");
        assert_eq!(incpatch_filter("1.2.3".to_string()), "1.2.4");
        assert_eq!(incminor_filter("v1.2.3".to_string()), "v1.3.0");
        assert_eq!(incpatch_filter("1.2.3-beta.1".to_string()), "1.2.4");
        assert_eq!(
            incmajor_filter("not-a-version".to_string()),
            "not-a-version"
        );
    }

    #[derive(Debug, serde::Serialize)]
    struct TestTemplateContext {
        tag: String,
        env: HashMap<String, String>,
        date: String,
        timestamp: String,
        now: String,
    }

    impl TemplateContext for TestTemplateContext {
        fn env(&self) -> &HashMap<String, String> {
            &self.env
        }

        fn date(&self) -> &str {
            &self.date
        }

        fn timestamp(&self) -> &str {
            &self.timestamp
        }

        fn now(&self) -> &str {
            &self.now
        }
    }

    #[test]
    fn test_render_template_exposes_meta_env_and_time() {
        let mut env = HashMap::new();
        env.insert("RLSR_TEST".to_string(), "ok".to_string());
        let ctx = TestTemplateContext {
            tag: "v1.2.3".to_string(),
            env,
            date: "2025-01-25".to_string(),
            timestamp: "0".to_string(),
            now: "2025-01-25T10:30:00Z".to_string(),
        };

        let rendered = render_template(
            "{{ env.RLSR_TEST }} {{ date }} {{ now|time(\"%Y-%m-%d\") }} {{ meta.tag }}",
            &ctx,
        );

        assert_eq!(rendered, "ok 2025-01-25 2025-01-25 v1.2.3");
    }
}
