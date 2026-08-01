```ini
[formats]
mdx = md

[*.mdx]
CommentDelimiters = {/*, */}
```

`CommentDelimiters` overrides the default HTML comment delimiters used by Vale.

This is useful for formats such as MDX that use non-HTML comment syntax.
