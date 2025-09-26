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

  // A command to preprocess the command from the desktop file. The commands should take arguments in this order:
  // command_name <term|no-term> <command>
  preprocess_exec_script: Some("/home/user/.local/share/anyrun/preprocess_application_command.sh")

  // The terminal used for running terminal based desktop entries, if left as `None` a static list of terminals is used
  // to determine what terminal to use.
  terminal: Some(Terminal(
    // The main terminal command
    command: "alacritty",
    // What arguments should be passed to the terminal process to run the command correctly
    // {} is replaced with the command in the desktop entry
    args: "-e {}",
  )),

  // Whether to prioritize actions or applications
  // Could be ActionsFirst (default), ApplicationsFirst, or NoPriority
  entry_priority: ActionsFirst,
)
```
