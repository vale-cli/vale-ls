```yaml
model: my-tagger
```

The tagger to read this rule's tags with, named without its extension.

Rules ported from another checker are written against that checker's idea of a
noun, and read differently under Vale's own tagger; naming its model here has
them read as intended.

A model is a `.dict` asset under `config/dictionaries`, so it ships and syncs
like any other asset. Leaving this unset uses Vale's own tagger.
