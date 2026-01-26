---
title: Templating
description: Template fields, variables, and filters in Rlsr.
---

Rlsr uses Minijinja (Jinja2-compatible) for templating. Use `{{ ... }}` for values and `{% ... %}` for control flow.

## Where Templating Works

Templating is supported in:
- Release hooks: `hooks.before`, `hooks.after`
- Release env values: `env`
- Build fields: `command`, `bin_name`, `artifact`, `archive_name`, `prehook`, `posthook`
- Build env values: `env`
- Additional files: release and build `additional_files`
- Docker target image: `targets.docker.image`
- Changelog templates: `changelog.template`

## Build Templates (BuildMeta)

Build templates use `meta.*` for build metadata. The top-level `env`, `date`, `timestamp`, and `now` are also available.

Available fields:
- `meta.build_name`
- `meta.tag`, `meta.version`, `meta.major`, `meta.minor`, `meta.patch`, `meta.prerelease`
- `meta.short_commit`
- `meta.is_snapshot`, `meta.is_prerelease`, `meta.is_dirty`
- `meta.os`, `meta.arch`, `meta.arm`, `meta.target`
- `meta.matrix` (map of matrix values, e.g. `meta.matrix.os`)
- `env.VAR_NAME`
- `date` (YYYY-MM-DD), `timestamp` (unix seconds), `now` (RFC 3339)

Example:
```yaml
builds:
  - name: "Linux x86_64"
    os: linux
    arch: amd64
    target: x86_64-unknown-linux-musl
    command: "cargo build --release --target {{ meta.target }}"
    artifact: "target/{{ meta.target }}/release/myapp"
    archive_name: "myapp-{{ meta.version }}-{{ meta.os }}-{{ meta.arch }}"
    prehook: "echo 'Building {{ meta.build_name }} for {{ meta.os }}/{{ meta.arch }}'"
    env:
      - "TARGET={{ meta.target }}"
      - "BUILD_DATE={{ date }}"
```

## Release Templates (TemplateMeta)

Release templates use `meta.*` for repository metadata. The top-level `env`, `date`, `timestamp`, and `now` are also available.

Available fields:
- `meta.tag`, `meta.version`, `meta.major`, `meta.minor`, `meta.patch`, `meta.prerelease`
- `meta.commit`, `meta.short_commit`
- `meta.branch`, `meta.previous_tag`
- `meta.project_name`, `meta.release_url`
- `meta.is_snapshot`, `meta.is_prerelease`, `meta.is_dirty`
- `env.VAR_NAME`
- `date` (YYYY-MM-DD), `timestamp` (unix seconds), `now` (RFC 3339)

Notes:
- `meta.release_url` is computed from the git `remote.origin.url` when it points at GitHub; otherwise it is empty.
- `meta.is_snapshot` is true when the current commit is not at the latest tag.
- `meta.is_dirty` is true when there are uncommitted changes.

Example:
```yaml
hooks:
  before:
    - "echo 'Starting {{ meta.tag }} (dirty={{ meta.is_dirty }})'"
  after:
    - "echo 'Release URL: {{ meta.release_url|default(\"n/a\") }}'"
env:
  - "VERSION={{ meta.version }}"
  - "COMMIT={{ meta.short_commit }}"
```

## Changelog Templates

Changelog templates have access to:
- `meta.*` from TemplateMeta (same as release templates)
- `commits`: array of commit objects

Commit fields:
- `commit.hash`, `commit.subject`, `commit.email`
- `commit.handle` (GitHub formatter only)
- `commit.type`, `commit.scope`, `commit.breaking` (conventional commits)

Example:
```jinja
# {{ meta.tag }}

{% for commit in commits if commit.type == "feat" %}
- {{ commit.subject }} {% if commit.scope %}({{ commit.scope }}){% endif %}
{% endfor %}

{% for commit in commits if commit.breaking %}
**Breaking**: {{ commit.subject }}
{% endfor %}
```

## Filters

Available in all templates:
- `tolower`, `toupper`, `title`
- `replace(old, new)`
- `trimprefix(prefix)`, `trimsuffix(suffix)`
- `split(sep)`
- `default(value)` (fallback when empty)
- `time(format)` (format RFC 3339 or unix timestamps)
- `incmajor`, `incminor`, `incpatch`

Changelog-only filters:
- `starts_with(prefix)`, `ends_with(suffix)`
- `trim(chars)`
- `contains(substr)`
- `match(regex)`

Example:
```jinja
{{ meta.tag|trimprefix("v") }}
{{ meta.version|incminor }}
{{ now|time("%Y%m%d") }}
```
