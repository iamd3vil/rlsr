{% for commit in commits %}
{% if commit.handle %}
{{ commit.hash }}: {{ commit.subject }} (@{{ commit.handle }})
{% else %}
{{ commit.hash }}: {{ commit.subject }}
{% endif %}
{% endfor %}