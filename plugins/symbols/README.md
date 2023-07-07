# Symbols

Look up unicode symbols and custom user defined symbols.

## Usage

Simply search for the symbol's name.

## Configuration

```ron
// <Anyrun config dir>/symbols.ron
Config(
  // The prefix that the search needs to begin with to yield symbol results
  prefix: "",
  // Custom user defined symbols to be included along the unicode symbols
  symbols: {
    // "name": "text to be copied"
    "shrug": "¯\\_(ツ)_/¯",
  },
  max_entries: 3,
)
```
