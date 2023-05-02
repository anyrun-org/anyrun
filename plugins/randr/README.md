# Randr

A plugin to quickly change monitor configurations on the fly.

## Usage

Type in the configured prefix (default is in [Configuration](#Configuration)), and select from the options. Fuzzy matching is enabled so it can be
used to narrow down the options.

## Configuration

```ron
//<Anyrun config dir>/randr.ron
Config(
  prefix: ":dp",
  max_entries: 5, 
)
```