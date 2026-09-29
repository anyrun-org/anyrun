# Rink

A simple calculator plugin powered by [Rink](https://github.com/tiffany352/rink-rs).

## Usage

Type in the configured prefix (default is no prefix), then just type in your calculations/unit conversions.

## Configuration

```ron
// <Anyrun config dir>/rink.ron
Config(
  prefix: "",
  // Pull currency conversions from https://rinkcalc.app/data/currency.json
  pull_currencies: true,
)
```
