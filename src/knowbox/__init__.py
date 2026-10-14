"""knowbox — personal/agent knowledge-base CLI.

Markdown-as-truth notes with YAML frontmatter, index-first protocol, hybrid
BM25 + gated semantic search, git auto-commit/push. Stdlib only; runs under
uv-managed Python 3.12+. The CLI lives here; the knowledge *data* lives in a
separate repo/dir (default ``~/knowbox``, override with ``KNOWBOX_ROOT``).
"""

__version__ = "2.1.0"
