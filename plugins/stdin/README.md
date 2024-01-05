# Stdin

Reads lines from the standard input and fuzzy matches on those. The selected one is printed to stdout.
Allows for easy integration into scripts that have been made with something like dmenu in mind.

## Usage

This plugin should generally be used alone, if a dmenu replacement is needed. This can be done with `anyrun --plugins libstdin.so`.
The content to fuzzy match on needs to be piped into Anyrun.
