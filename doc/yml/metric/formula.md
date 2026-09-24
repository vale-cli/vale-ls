```yaml
formula: |
    (0.39 * (words / sentences)) + (11.8 * (syllables / words)) - 15.59
```

A formula of pre-defined variables to be evaluated.

Counts: `words`, `sentences`, `characters`, `syllables`, `complex_words`, `long_words`, `polysyllabic_words`, `paragraphs`, `heading.h1` through `heading.h6`, `list`, `blockquote`, and `pre`.

Readability scores (Vale 3.23.0 or later): `automated_readability`, `coleman_liau`, `dale_chall`, `flesch_kincaid`, `flesch_reading_ease`, `gunning_fog`, `lix`, and `smog`.

The `math` module and `round(x)` or `round(x, places)` are available.
