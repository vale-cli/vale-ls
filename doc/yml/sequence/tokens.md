```yaml
tokens:
  - tag: MD
  - pattern: be
  - tag: JJ
  # The `|` notation means that we'll accept `VB` or `VBN` in position 4.
  - tag: VB|VBN
```

A list of tokens with associated NLP metadata.

Supported token fields are:

- `pattern`: a regex pattern matched against token text.
- `tag`: a POS-tag pattern (for example, `NN|NNS`).
- `skip`: number of optional positions before this token.
- `negate`: invert `pattern`/`tag` matching.
