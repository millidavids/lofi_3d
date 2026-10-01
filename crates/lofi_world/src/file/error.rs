#[derive(Debug, thiserror::Error)]
pub enum MapFileError {
    #[error("{0}")]
    Io(#[from] std::io::Error),
    #[error("not a lofi map file")]
    NotAMap,
    #[error("this map needs a newer lofi (format {found}; this build reads up to {supported})")]
    TooNew { found: u32, supported: u32 },
    #[error("this map has \"{0}\" data that needs a newer lofi")]
    UnknownCritical(String),
    #[error("could not decode map: {0}")]
    Decode(#[from] rmp_serde::decode::Error),
    #[error("could not encode map: {0}")]
    Encode(#[from] rmp_serde::encode::Error),
    #[error("map file is damaged: {0}")]
    Corrupt(String),
    #[error("invalid map: {0}")]
    Invalid(String),
}
