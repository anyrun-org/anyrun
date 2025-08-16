# Nix-run

`nix run` graphical apps from nixpkgs straight from Anyrun

## Usage

Simply search for programs name/executable name

## Configuration

```ron
// <Anyrun config dir>/nix-run.ron
Config(
  // Whether or not to allow unfree packages
  allow_unfree: false,
  // Nixpkgs channel to get the package list from
  channel: "nixpkgs-unstable",
  max_entries: 3,
)
```
