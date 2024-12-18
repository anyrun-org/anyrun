# Powermenu

A plugin to lock, logout, power off your machine, etc.

## Usage

Search for one of the following actions: lock, logout, power off, reboot, suspend, hibernate.
Select the action.
If prompted, confirm it.

## Configuration

```ron
// <Anyrun config dir>/powermenu.ron
Config(
  lock: (
    command: "loginctl lock-session",
    confirm: false,
  ),
  logout: (
    command: "loginctl terminate-user $USER",
    confirm: true,
  ),
  poweroff: (
    command: "systemctl -i poweroff",
    confirm: true,
  ),
  reboot: (
    command: "systemctl -i reboot",
    confirm: true,
  ),
  suspend: (
    command: "systemctl -i suspend",
    confirm: false,
  ),
  hibernate: (
    command: "systemctl -i hibernate",
    confirm: false,
  ),
)
```
