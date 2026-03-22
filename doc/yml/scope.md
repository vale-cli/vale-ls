```yaml
scope: heading
```

Vale is “syntax aware,” which means that it’s capable of both applying rules to 
and ignoring certain sections of text. This functionality is implemented 
through a *scoping* system. A scope is specified through a *selector* such as `paragraph.rst`, which indicates that the rule applies to all paragraphs in reStructuredText files.

See the [documentation][1] for a full list of accepted values.

## Multi-scope rules

Rules may define multiple scopes by using a YAML array:

```yaml
scope:
    # h1 OR h2
    - heading.h2
    - heading.h3
```

## Negation & multi-part selectors

Any scope prefaced with "~" is negated:

```yaml
scope:
  # all scopes != h2
  - ~heading.h2
```

You can chain multiple scopes together using "&":


```yaml
scope:
  - ~blockquote & ~heading
```

[1]: https://vale.sh/docs/topics/scoping/
