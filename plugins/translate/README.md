# Translate

Quickly translate text using the Google Translate API.

## Usage

Type in `<prefix><target lang> <text to translate>`, where prefix is the configured prefix (default is in [Configuration](#Configuration)) and the rest are pretty obvious.

## Configuration

```ron
// <Anyrun config dir>/translate.ron
Config(
  prefix: ":",
  max_entries: 3,
)
```