{%- for commit in commits %}
{{ commit.hash }}: {{ commit.subject }}
{%- endfor %}