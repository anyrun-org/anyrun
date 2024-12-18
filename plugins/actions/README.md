# Actions

A plugin for running configurable commands with optional confirmation. Includes built-in power management actions (lock, log out, power off, reboot, suspend, hibernate) and supports custom user-defined actions.

## Usage

Search for any of the configured actions by title or description.
Select the action.
If prompted, confirm it.

Power actions (lock, log out, power off, reboot, suspend, hibernate) are included by default.

## Configuration

```ron
// <Anyrun config dir>/actions.ron
Config(
  // Set to false to disable built-in power actions (default: true).
  enable_power_actions: true,
  // Add your own custom actions here.
  custom_actions: [
    (
      // Required.
      title: "Open Browser",
      // Required. The command is passed through shell.
      command: "xdg-open https://example.com",
      // Optional.
      description: "Launch the default web browser",
      // Optional.
      icon: "web-browser",
      // Optional (default: false).
      confirm: false,
    ),
  ],
)
```

The default power actions are equivalent to the following custom actions configuration:

```ron
Config(
  enable_power_actions: false,
  custom_actions: [
    (
      title: "Lock",
      command: "loginctl lock-session",
      description: "Lock the session screen",
      icon: "system-lock-screen",
      confirm: false,
    ),
    (
      title: "Log out",
      command: "loginctl terminate-session $XDG_SESSION_ID",
      description: "Terminate the session",
      icon: "system-log-out",
      confirm: true,
    ),
    (
      title: "Power off",
      command: "systemctl poweroff || poweroff",
      description: "Shut down the system",
      icon: "system-shutdown",
      confirm: true,
    ),
    (
      title: "Reboot",
      command: "systemctl reboot || reboot",
      description: "Restart the system",
      icon: "system-reboot",
      confirm: true,
    ),
    (
      title: "Suspend",
      command: "systemctl suspend || pm-suspend",
      description: "Suspend the system to RAM",
      icon: "system-suspend",
      confirm: false,
    ),
    (
      title: "Hibernate",
      command: "systemctl hibernate || pm-hibernate",
      description: "Suspend the system to disk",
      icon: "system-suspend-hibernate",
      confirm: false,
    ),
  ],
)
```
