use std::fs::File;
use std::io::{BufReader, BufWriter, Seek, SeekFrom, Write};

use indexmap::IndexSet;
use serde::{Deserialize, Serialize};

use crate::HistoryConfig;

#[derive(Serialize, Deserialize, Default)]
struct PersistedHistory<T> {
    elements: T,
}
type PersistedHistoryOwned = PersistedHistory<IndexSet<HistoryItem>>;
type PersistedHistoryBorrowed<'a> = PersistedHistory<&'a IndexSet<HistoryItem>>;

#[derive(Serialize, Deserialize, PartialEq, Eq, Hash, Debug)]
pub struct HistoryItem {
    pub command: String,
}

impl HistoryItem {
    fn new(command: String) -> Self {
        Self { command }
    }
}

#[derive(Debug)]
pub struct History {
    store: File,
    elements: IndexSet<HistoryItem>,
    pub cap: usize,
}

impl History {
    pub fn new(history_config: &HistoryConfig) -> Result<History, std::io::Error> {
        let maybe_history_path =
            dirs::state_dir().map(|s| s.join("anyrun").join("shell").join("history.json"));

        if let Some(history_path) = maybe_history_path {
            if let Some(dir) = history_path.parent() {
                std::fs::create_dir_all(dir)?;
            }

            let file = File::options()
                .read(true)
                .write(true)
                .create(true)
                .truncate(false)
                .open(&history_path)?;

            let reader = BufReader::new(&file);
            let persisted_history: PersistedHistoryOwned = match serde_json::from_reader(reader) {
                Ok(val) => val,
                Err(e) if e.is_eof() => PersistedHistory::default(),
                Err(e) => return Err(e.into()),
            };

            Ok(History {
                store: file,
                elements: persisted_history.elements,
                cap: history_config.capacity,
            })
        } else {
            Err(std::io::Error::new(
                std::io::ErrorKind::Unsupported,
                "failed to get the user state directory",
            ))
        }
    }

    pub fn push(&mut self, value: String) -> Result<(), std::io::Error> {
        // insert_before ensures new usages of existing commands bubble up to the top of the history, simple `insert` does not
        self.elements
            .insert_before(self.elements.len(), HistoryItem::new(value));

        if self.elements.len() > self.cap {
            let remove_count = self.elements.len().saturating_sub(self.cap);
            self.elements.drain(0..remove_count);
        }

        self.store.set_len(0)?;
        self.store.seek(SeekFrom::Start(0))?;

        let mut writer = BufWriter::new(&self.store);
        serde_json::to_writer(
            &mut writer,
            &PersistedHistoryBorrowed {
                elements: &self.elements,
            },
        )?;
        writer.flush()?;

        Ok(())
    }

    pub fn elements(&self) -> impl Iterator<Item = &HistoryItem> {
        self.elements.iter()
    }
}
