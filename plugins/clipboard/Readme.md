# Clipboard

Manage your clipboard history using [cliphist](https://github.com/sentriz/cliphist).

## Usage

Type in `<prefix><search term>`, where prefix is the configured prefix (default in [Configuration](#Configuration)).

If no prefix is configured, the plugin will show all history entries when Anyrun is launched.

## Configuration

```ron
// <Anyrun config dir>/clipboard.ron
Config(
  prefix: "",
  max_entries: 15,
)
