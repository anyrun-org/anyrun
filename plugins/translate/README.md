# Translate

Quickly translate text using the Google Translate API.

## Usage

Type in `<prefix><target lang> <text to translate>` or `<prefix><src lang><language_delimiter><target lang> <text to translate>`,
where the `prefix` and `language_delimiter` are config options (defaults are in [Configuration](#Configuration)) and the rest are pretty obvious.

## Configuration

```ron
// <Anyrun config dir>/translate.ron
Config(
  prefix: ":",
  language_delimiter: ">",
  max_entries: 3,
)
```