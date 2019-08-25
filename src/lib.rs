use std::collections::HashMap;

use serde_derive::{Serialize, Deserialize};

pub(crate) type Category = String;
pub(crate) type Tree = HashMap<Category, Vec<Entry>>;


#[derive(Serialize, Deserialize, Debug)]
pub(crate) struct InternalData {
    #[serde(flatten)]
    pub tree: Tree,
    #[serde(skip)]
    pub sender: Option<glib::Sender<()>>,
    #[serde(default = None)]
    pub pos: Option<u64>
}

#[derive(Serialize, Deserialize, Debug)]
pub(crate) struct Entry {
    pub name: String,
    pub url: String
}

#[derive(Debug)]
pub(crate) enum Error {
    DbusError(dbus::Error),
	DbusTypeCastingError(dbus::arg::TypeMismatchError),
    SerialisationError(serde_json::Error),
    IOError(std::io::Error)
}

impl From<dbus::Error> for Error {
    fn from(e: dbus::Error) -> Self {
        Error::DbusError(e)
    }
}

impl From<dbus::arg::TypeMismatchError> for Error {
    fn from(e: dbus::arg::TypeMismatchError) -> Self {
        Error::DbusTypeCastingError(e)
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

impl From<Error> for std::io::Error {
    fn from(e: Error) -> std::io::Error {
        match e {
            Error::IOError(e) => e,
            _ => std::io::Error::new(std::io::ErrorKind::Other, format!("{:?}", e))
        }
    }
}
