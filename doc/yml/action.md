```yaml
action:
  name: replace
```

`action` defines the quick fix Vale should suggest for a matched token.

The available `name` options are:

- `suggest`: an array of dynamically-computed suggestions.
- `replace`: an array of static suggestions (used by default in `substitution` and `capitalization` rules).
- `remove`: remove the matched text.
- `convert`: convert the matched text (for example, to simple case).
- `edit`: perform in-place edits of the matched text.

See the fixer documentation for details on each action type [1].

[1]: https://docs.vale.sh/topics/actions
