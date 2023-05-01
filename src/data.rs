use std::{
    collections::BTreeMap,
    fs::{self, File},
    io::Write,
    path::Path,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};

use anyhow::Result;
use dashmap::{try_result::TryResult, DashMap};
use once_cell::sync::OnceCell;
use parking_lot::{RwLock, RwLockReadGuard, RwLockWriteGuard};
use serde::{
    de,
    ser::{SerializeMap, SerializeSeq},
    Deserialize, Serialize,
};
use tokio::sync::watch::{self, Receiver};

use crate::{entity::MarkdownConfig, helpers, html};

static GENKIT_DATA: OnceCell<RwLock<GenkitData>> = OnceCell::new();
// Atomic boolean to indicate if the data has been modified.
// Currenly, mainly concerned with the `url_previews` field.
static DIRTY: AtomicBool = AtomicBool::new(false);
static DATA_FILENAME: OnceCell<&str> = OnceCell::new();

pub fn load<P: AsRef<Path>>(path: P) {
    GENKIT_DATA.get_or_init(|| {
        RwLock::new(GenkitData::new(path.as_ref().join(get_data_filename())).unwrap())
    });
}

pub fn read() -> RwLockReadGuard<'static, GenkitData> {
    GENKIT_DATA.get().unwrap().read()
}

pub fn write() -> RwLockWriteGuard<'static, GenkitData> {
    GENKIT_DATA.get().unwrap().write()
}

pub fn set_data_filename(filename: &'static str) {
    DATA_FILENAME.set(filename).unwrap();
}

fn get_data_filename() -> &'static str {
    DATA_FILENAME.get().unwrap_or(&"genkit.json")
}

/// Export all data into the json file.
/// If the data is empty, we never create the json file.
pub fn export<P: AsRef<Path>>(path: P) -> Result<()> {
    // Prevent repeatedly exporting the same data.
    // Otherwise will cause infinity auto reload.
    if DIRTY.load(Ordering::Relaxed) {
        let data = read();
        if !data.url_previews.is_empty() {
            let mut file = File::create(path.as_ref().join(get_data_filename()))?;
            file.write_all(data.export_to_json()?.as_bytes())?;
        }
        DIRTY.store(false, Ordering::Relaxed);
    }
    Ok(())
}

#[derive(Clone, Debug)]
pub struct UrlPreviewInfo {
    pub title: String,
    pub description: String,
    pub image: Option<String>,
}

impl Serialize for UrlPreviewInfo {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut seq = serializer.serialize_seq(Some(3))?;
        seq.serialize_element(&self.title)?;
        seq.serialize_element(&self.description)?;
        if let Some(image) = self.image.as_ref() {
            seq.serialize_element(image)?;
        } else {
            seq.serialize_element("")?;
        }
        seq.end()
    }
}

impl<'de> Deserialize<'de> for UrlPreviewInfo {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_any(UrlPreviewInfoVisitor)
    }
}

struct UrlPreviewInfoVisitor;

impl<'de> de::Visitor<'de> for UrlPreviewInfoVisitor {
    type Value = UrlPreviewInfo;

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        formatter.write_str("2 or 3 elements tuple")
    }

    fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
    where
        A: de::SeqAccess<'de>,
    {
        let (title, description, image) = (
            seq.next_element()?.unwrap_or_default(),
            seq.next_element()?.unwrap_or_default(),
            seq.next_element()?,
        );
        Ok(UrlPreviewInfo {
            title,
            description,
            image,
        })
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GenkitData {
    #[serde(skip)]
    markdown_config: MarkdownConfig,
    // The preview tasks.
    #[serde(skip)]
    preview_tasks: DashMap<String, Receiver<Option<PreviewEvent>>>,
    // All url preview data.
    url_previews: Arc<DashMap<String, UrlPreviewInfo>>,
}

// Implement Serialize manually to keep urlPreviews ordered.
impl Serialize for GenkitData {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut url_previews = BTreeMap::new();
        self.url_previews.iter().for_each(|kv| {
            let (key, value) = kv.pair();
            url_previews.insert(key.to_owned(), value.to_owned());
        });

        let mut map = serializer.serialize_map(Some(1))?;
        map.serialize_entry("urlPreviews", &url_previews)?;
        map.end()
    }
}

#[derive(Debug, Clone)]
pub enum PreviewEvent {
    Finished(UrlPreviewInfo),
    Failed(String),
}

impl GenkitData {
    pub fn new(source: impl AsRef<Path>) -> Result<Self> {
        let path = source.as_ref();
        if path.exists() {
            let json = fs::read_to_string(path)?;
            Ok(serde_json::from_str(&json)?)
        } else {
            Ok(GenkitData {
                markdown_config: MarkdownConfig::default(),
                url_previews: Arc::new(DashMap::default()),
                preview_tasks: DashMap::default(),
            })
        }
    }

    pub fn get_all_previews(&self) -> Arc<DashMap<String, UrlPreviewInfo>> {
        Arc::clone(&self.url_previews)
    }

    pub fn get_preview(&self, url: &str) -> Option<UrlPreviewInfo> {
        match self.url_previews.try_get(url) {
            TryResult::Present(info) => Some(info.to_owned()),
            TryResult::Absent => None,
            TryResult::Locked => {
                panic!("The url preview data is locked, please try again later.")
            }
        }
    }

    /// Preview url asynchronously, return a tuple.
    /// The first bool argument indicating whether is a first time previewing.
    /// The second argument is the receiver to wait preview event finished.
    pub fn preview_url(&self, url: &str) -> (bool, Receiver<Option<PreviewEvent>>) {
        if let Some(rx) = self.preview_tasks.get(url) {
            // In the preview queue.
            (false, rx.clone())
        } else {
            let (tx, rx) = watch::channel::<Option<PreviewEvent>>(None);
            // Not in the preview queue, enqueue the preview task.
            self.preview_tasks.insert(url.to_owned(), rx.clone());

            let url = url.to_owned();
            let list = Arc::clone(&self.url_previews);
            // Spawn a background task to preview the url.
            tokio::spawn(async move {
                match helpers::fetch_url(&url).await {
                    Ok(html) => {
                        let meta = html::parse_html_meta(html);
                        let info = UrlPreviewInfo {
                            title: meta.title.into_owned(),
                            description: meta.description.into_owned(),
                            image: meta.image.as_ref().map(|image| image.to_string()),
                        };

                        list.insert(url, info.clone());
                        DIRTY.store(true, Ordering::Relaxed);
                        tx.send(Some(PreviewEvent::Finished(info)))
                    }
                    Err(err) => tx.send(Some(PreviewEvent::Failed(err.to_string()))),
                }
            });
            (true, rx)
        }
    }

    pub fn set_markdown_config(&mut self, config: MarkdownConfig) -> &mut Self {
        self.markdown_config = config;
        self
    }

    pub fn get_markdown_config(&self) -> &MarkdownConfig {
        &self.markdown_config
    }

    fn export_to_json(&self) -> Result<String> {
        Ok(serde_json::to_string_pretty(self)?)
    }
}
