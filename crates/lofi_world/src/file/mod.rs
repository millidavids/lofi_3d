pub(crate) mod codec;
mod error;
mod format;
mod records;
mod save;
mod sections;
mod v1;
mod v2;
mod v3;

pub use error::MapFileError;
pub use format::{
    FileStamp, LoadedMap, MAP_FILE_EXTENSION, MAP_FORMAT_VERSION, decode_map, encode_map,
};
pub(crate) use save::write_file;
pub use save::{SaveReport, backup_path, load_map, save_autosave, save_map};
pub use sections::UnknownSection;
