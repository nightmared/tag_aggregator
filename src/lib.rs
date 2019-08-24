use std::collections::HashMap;

use serde_derive::{Serialize, Deserialize};

pub(crate) type Tree = HashMap<String, Vec<Entry>>;

#[derive(Serialize, Deserialize, Debug)]
pub(crate) struct InternalData {
    #[serde(flatten)]
    pub tree: Tree,
    #[serde(skip)]
    pub sender: Option<glib::Sender<()>>
}

#[derive(Serialize, Deserialize, Debug)]
pub(crate) struct Entry {
    pub name: String,
    pub url: String
}

#[derive(Debug)]
pub(crate) enum Error {
    CurlError(curl::Error),
    SerialisationError(serde_json::Error),
    IOError(std::io::Error)
}

impl From<curl::Error> for Error {
    fn from(e: curl::Error) -> Self {
        Error::CurlError(e)
    }
}

impl From<serde_json::Error> for Error {
    fn from(e: serde_json::Error) -> Self {
        Error::SerialisationError(e)
    }
}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::IOError(e)
    }
}
