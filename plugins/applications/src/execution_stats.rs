use std::collections::HashMap;
use std::fs;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::Path;
use std::sync::{Arc, Mutex};

use crate::Config;
use crate::scrubber::DesktopEntry;

pub(crate) struct ExecutionStats {
    weight_map: Arc<Mutex<HashMap<String, i64>>>,
    max_weight: i64,
    execution_statistics_path: String,
}

impl ExecutionStats {
    pub(crate) fn from_file_or_default(execution_statistics_path: &str, config: &Config) -> Self {
        let execution_statistics: HashMap<String, i64> = fs::read_to_string(execution_statistics_path)
            .map_err(|error| format!("Error parsing applications plugin config: {}", error))
            .and_then(|content: String| ron::from_str(&content)
                .map_err(|error| format!("Error reading applications plugin config: {}", error)))
            .unwrap_or_else(|error_message| {
                format!("{}", error_message);
                HashMap::new()
            });

        ExecutionStats {
            weight_map: Arc::new(Mutex::new(execution_statistics)),
            max_weight: config.max_counted_usages,
            execution_statistics_path: execution_statistics_path.to_owned(),
        }
    }

    pub(crate) fn save(&self) -> Result<(), String> {
        let path = Path::new(&self.execution_statistics_path);
        if let Some(containing_folder) = path.parent() {
            if !containing_folder.exists() {
                fs::create_dir_all(containing_folder)
                    .map_err(|error| format!("Error creating containing folder for usage statistics: {:?}", error))?;
            }
            let mut file = OpenOptions::new().create(true).write(true).truncate(true).open(path)
                .map_err(|error| format!("Error creating data file for usage statistics: {:?}", error))?;
            let weight_map = self.weight_map.lock()
                .map_err(|error| format!("Error locking file for usage statistics: {:?}", error))?;
            let serialized_data = ron::to_string(&*weight_map)
                .map_err(|error| format!("Error serializing usage statistics: {:?}", error))?;
            file.write_all(serialized_data.as_bytes())
                .map_err(|error| format!("Error writing data file for usage statistics: {:?}", error))
        } else {
            Err(format!("Error getting parent folder of: {:?}", path))
        }
    }

    pub(crate) fn register_usage(&self, application: &DesktopEntry) {
        {
            let mut guard = self.weight_map.lock().unwrap();
            if let Some(count) = guard.get_mut(&application.exec) {
                *count += 1;
            } else {
                guard.insert(application.exec.clone(), 1);
            }
        }
        if let Err(error_message) = self.save() {
            eprintln!("{}", error_message);
        }
    }

    pub(crate) fn get_weight(&self, application: &DesktopEntry) -> i64 {
        let weight = *self.weight_map.lock().unwrap().get(&application.exec).unwrap_or(&0);

        if weight < self.max_weight {
            weight
        } else {
            self.max_weight
        }
    }
}
