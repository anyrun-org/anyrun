use serde::Deserialize;

use crate::actions::PowerAction;

#[derive(Deserialize, Default)]
pub struct PowerActionConfig {
    pub command: String,
    pub confirm: bool,
}

#[derive(Deserialize)]
pub struct Config {
    #[serde(default = "Config::default_lock_config")]
    lock: PowerActionConfig,
    #[serde(default = "Config::default_logout_config")]
    logout: PowerActionConfig,
    #[serde(default = "Config::default_poweroff_config")]
    poweroff: PowerActionConfig,
    #[serde(default = "Config::default_reboot_config")]
    reboot: PowerActionConfig,
    #[serde(default = "Config::default_suspend_config")]
    suspend: PowerActionConfig,
    #[serde(default = "Config::default_hibernate_config")]
    hibernate: PowerActionConfig,
}

impl Config {
    fn default_lock_config() -> PowerActionConfig {
        PowerActionConfig {
            command: String::from("loginctl lock-session"),
            confirm: false,
        }
    }

    fn default_logout_config() -> PowerActionConfig {
        PowerActionConfig {
            command: String::from("loginctl terminate-user $USER"),
            confirm: true,
        }
    }

    fn default_poweroff_config() -> PowerActionConfig {
        PowerActionConfig {
            command: String::from("systemctl -i poweroff"),
            confirm: true,
        }
    }

    fn default_reboot_config() -> PowerActionConfig {
        PowerActionConfig {
            command: String::from("systemctl -i reboot"),
            confirm: true,
        }
    }

    fn default_suspend_config() -> PowerActionConfig {
        PowerActionConfig {
            command: String::from("systemctl -i suspend"),
            confirm: false,
        }
    }

    fn default_hibernate_config() -> PowerActionConfig {
        PowerActionConfig {
            command: String::from("systemctl -i hibernate"),
            confirm: false,
        }
    }

    pub const fn get_action_config(&self, action: PowerAction) -> &PowerActionConfig {
        match action {
            PowerAction::Lock => &self.lock,
            PowerAction::Logout => &self.logout,
            PowerAction::Poweroff => &self.poweroff,
            PowerAction::Reboot => &self.reboot,
            PowerAction::Suspend => &self.suspend,
            PowerAction::Hibernate => &self.hibernate,
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            lock: Self::default_lock_config(),
            logout: Self::default_logout_config(),
            poweroff: Self::default_poweroff_config(),
            reboot: Self::default_reboot_config(),
            suspend: Self::default_suspend_config(),
            hibernate: Self::default_hibernate_config(),
        }
    }
}
