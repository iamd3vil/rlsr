---
title: Templating
description: Template fields, variables, and filters in Rlsr.
---

Rlsr uses Minijinja (Jinja2-compatible) for templating. Use `{{ ... }}` for values and `{% ... %}` for control flow.

## Where Templating Works

Templating is supported in:
- Release hooks: `hooks.before`, `hooks.after`
- Release env values: `env`
- Build fields: `name`, `command`, `bin_name`, `artifact`, `archive_name`, `prehook`, `posthook`
- Build env values: `env`
- Rust and Go build fields: `rust.tool`, `rust.flags`, `rust.binary`, `go.main`, `go.flags`, `go.ldflags`, `go.binary`
- Additional files: release and build `additional_files`
- GitHub release title: `targets.github.name`
- Docker target images: `targets.docker.image`, `targets.docker.images`
- Changelog templates: `changelog.template`

Buildx tags (`builds[].buildx.tags`) are rendered with build metadata and reused for Docker publishing when no explicit Docker images are configured. The `targets.docker.push` flag is a boolean toggle and does not support templating.

## Build Templates (BuildMeta)

Build templates use `meta.*` for build metadata. The top-level `env`, `date`, `timestamp`, and `now` are also available.

Available fields:
- `meta.build_name`
- `meta.tag`, `meta.version`, `meta.major`, `meta.minor`, `meta.patch`, `meta.prerelease`
- `meta.short_commit`
- `meta.is_snapshot`, `meta.is_nightly`, `meta.is_prerelease`, `meta.is_dirty`
- `meta.os`, `meta.arch`, `meta.arm`, `meta.target` (filled in per target for `rust` and `go` builds)
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
- `meta.is_snapshot`, `meta.is_nightly`, `meta.is_prerelease`, `meta.is_dirty`
- `env.VAR_NAME`
- `date` (YYYY-MM-DD), `timestamp` (unix seconds), `now` (RFC 3339)

Notes:
- `meta.release_url` is computed from the git `remote.origin.url` when it points at GitHub; otherwise it is empty.
- `meta.is_snapshot` is true for `--snapshot` builds. In a snapshot, `meta.tag` and `meta.version` hold the rendered `snapshot.version_template`, and `meta.previous_tag` is the latest tag.
- `meta.is_nightly` is true for `--nightly` builds. In a nightly, `meta.tag` is the rolling tag (`nightly` by default), `meta.version` holds the rendered `nightly.version_template`, `meta.previous_tag` is the latest tag, and `meta.is_prerelease` is always true.
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
- `github_notes`: GitHub's auto-generated release notes (GitHub formatter only, see below)

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

### Per-release notes

Changelog templates can `{% include %}` files, with paths relative to the
directory rlsr runs from. Use this to put hand-written notes (highlights,
upgrade steps) above the generated changes:

```jinja
{% include "release-notes/" ~ meta.tag ~ ".md" ignore missing %}

## What's Changed
{% for commit in commits %}
* {{ commit.subject }}{% if commit.handle %} by @{{ commit.handle }}{% endif %}

{% endfor %}

**Full Changelog**: https://github.com/OWNER/REPO/compare/{{ meta.previous_tag }}...{{ meta.tag }}
```

Write `release-notes/v1.2.0.md` before tagging `v1.2.0`. With `ignore missing`,
releases without a notes file get only the generated part. Included files are
rendered as templates too, so `{{ meta.version }}` works inside them; wrap any
literal `{{` in `{% raw %}...{% endraw %}`.

### GitHub-generated notes

With `format: "github"`, a template that uses `github_notes` gets the body of
GitHub's [generated release notes](https://docs.github.com/en/repositories/releasing-projects-on-github/automatically-generated-release-notes)
for `meta.previous_tag...meta.tag`. This is the same output as the "Generate
release notes" button on GitHub: merged PRs with authors and links, a
"New Contributors" section and a "Full Changelog" compare link.

```jinja
{% include "release-notes/" ~ meta.tag ~ ".md" ignore missing %}

{{ github_notes }}
```

Notes:
- The notes are built from merged pull requests, not from `commits`, so
  `changelog.exclude` doesn't apply. Use a
  [`.github/release.yml`](https://docs.github.com/en/repositories/releasing-projects-on-github/automatically-generated-release-notes#configuring-automatically-generated-release-notes)
  to group or exclude PRs by label or author.
- The repository comes from `remote.origin.url` and the call uses
  `GITHUB_TOKEN`.
- The API is only called when `github_notes` appears in the main template file,
  not in included files.

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
