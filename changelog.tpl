## Features:
{% for commit in commits if commit.subject|starts_with("feat:") or commit.subject|starts_with("refactor:") %}
{{ commit.hash }}: {{ commit.subject|trim("feat: ")|trim("refactor: ") }}
{%- endfor %}

## Fixes:
{% for commit in commits if commit.subject|starts_with("fix:") %}
{{ commit.hash }}: {{ commit.subject|trim("fix: ") }}
{%- endfor %}