```yaml
# becomes (?m)\b(?:appears to be|arguably)\b after compilation
tokens:
  - appears to be
  - arguably
```

A list of strings or regular expressions to be transformed into a word-bounded, non-capturing group.

When `ignorecase: true`, compilation also adds the `(?i)` flag.
