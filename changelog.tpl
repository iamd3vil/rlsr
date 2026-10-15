{#- Notes files are rendered as templates: wrap literal {{ or {% in {% raw %}.
    github_notes only lists merged PRs, so commits pushed directly are listed here. -#}
{% include "release-notes/" ~ meta.tag ~ ".md" ignore missing %}
{% for commit in commits if not commit.subject | match("\\(#[0-9]+\\)$") %}
{% if loop.first %}

## Commits
{% endif %}
* {{ commit.subject }}{% if commit.handle %} by @{{ commit.handle }}{% endif %} in {{ commit.hash }}
{% endfor %}

{{ github_notes }}
