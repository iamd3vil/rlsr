{#- Notes files are rendered as templates: wrap literal {{ or {% in {% raw %}.
    github_notes only lists merged PRs, so commits pushed directly are listed here. -#}
{% if meta.is_nightly %}
Nightly build `{{ meta.version }}` from {{ meta.commit }}. The next nightly replaces it; use the [latest release](https://github.com/iamd3vil/rlsr/releases/latest) for a stable version.
{% endif %}
{% include "release-notes/" ~ meta.tag ~ ".md" ignore missing %}
{%- for commit in commits if not commit.subject | match("\\(#[0-9]+\\)$") %}
{%- if loop.first %}

## Commits
{% endif %}
* {{ commit.subject }}{% if commit.handle %} by @{{ commit.handle }}{% endif %} in {{ commit.hash }}
{%- endfor %}

{{ github_notes }}
