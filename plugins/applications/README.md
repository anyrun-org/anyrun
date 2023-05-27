# Applications

Launch applications.

## Usage

Simply search for the application you wish to launch.

*NOTE: The applications plugin does not look for executables in your $PATH, it looks for [desktop entries](https://specifications.freedesktop.org/desktop-entry-spec/desktop-entry-spec-latest.html) in standard locations (`XDG_DATA_DIRS`).*

## Configuration

```ron
// <Anyrun config dir>/applications.ron
Config(
  // Also show the Desktop Actions defined in the desktop files, e.g. "New Window" from LibreWolf
  desktop_actions: true,
  max_entries: 5, 
)
```