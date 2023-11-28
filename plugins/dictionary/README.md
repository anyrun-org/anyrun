# Dictionary

Look up word definitions using the [Free Dictionary API](https://dictionaryapi.dev/).

## Usage

Type in `<prefix><word to define>`, where prefix is the configured prefix (default in [Configuration](#Configuration)).

## Configuration

```ron
// <Anyrun config dir>/dictionary.ron
Config(
  prefix: ":def",
  max_entries: 5,
)
```
