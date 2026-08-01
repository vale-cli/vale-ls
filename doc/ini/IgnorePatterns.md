```ini
[*.md]
BasedOnStyles = MyStyle
# Default: None
IgnorePatterns = (?s) *({< file [^>]* >}.*?{</ ?file >})
```

`IgnorePatterns` allow you to exclude block-level sections of text that don't 
have an associated HTML tag that could be used with `SkippedScopes`.

`IgnorePatterns` are only supported in Markdown, reStructuredText, AsciiDoc,
and Org Mode.

`IgnorePatterns` is a legacy compatibility key. Prefer `BlockIgnores` in new
configurations.

For a single pattern, `IgnorePatterns` and `BlockIgnores` typically behave the
same. In advanced cases (such as multiple values or escaped commas), behavior
can differ.
