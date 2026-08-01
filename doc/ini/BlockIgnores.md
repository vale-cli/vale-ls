```bash
[*.md]
BasedOnStyles = MyStyle
# Default: None
BlockIgnores = (?s) *({< file [^>]* >}.*?{</ ?file >})
```

`BlockIgnores` allow you to exclude block-level sections of text that don't 
have an associated HTML tag that could be used with `SkippedScopes`.

Prefer this key over `IgnorePatterns` in new configurations.

`BlockIgnores` also has more predictable behavior when multiple values are
defined.

`BlockIgnores` are only supported in Markdown, reStructuredText, AsciiDoc,
and Org Mode.
