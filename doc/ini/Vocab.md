```bash
Vocab = Vocab1, Vocab2
```

[Vocabularies][1] allow you to maintain custom lists of terminology independent 
of your styles.

```bash
StylesPath = "..."

# Here's where we define the exceptions to use in *all*
# `BasedOnStyles`.
Vocab = Some-Name

[*]
# 'Vale' and 'MyStyle' automatically respects all
# custom exceptions.
#
# The built-in 'Vale' style is required for using
# `Vale.Terms`, `Vale.Avoid`, or `Vale.Spelling`.
BasedOnStyles = Vale, MyStyle
```

[1]: https://vale.sh/docs/topics/vocab/
