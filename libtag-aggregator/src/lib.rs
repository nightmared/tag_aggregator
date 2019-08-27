#![feature(proc_macro_hygiene)]
use std::collections::HashMap;

use serde_derive::{Serialize, Deserialize};

pub mod ui;
pub mod dbus_client;
pub mod server;
pub mod utils;

pub type Category = String;
pub type Tree = HashMap<Category, Vec<Entry>>;


#[derive(Serialize, Deserialize, Debug)]
pub struct InternalData {
    #[serde(flatten)]
    pub tree: Tree,
    #[serde(skip)]
    pub sender: Option<glib::Sender<()>>,
    #[serde(default = None)]
    pub pos: Option<u64>
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Entry {
    pub name: String,
    pub url: String
}

#[derive(Debug)]
pub enum Error {
    DbusError(dbus::Error),
	DbusTypeCastingError(dbus::arg::TypeMismatchError),
    SerialisationError(serde_json::Error),
    IOError(std::io::Error),
    HyperError(hyper::Error)
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

impl From<hyper::Error> for Error {
    fn from(e: hyper::Error) -> Self {
        Error::HyperError(e)
    }
}

impl From<Error> for hyper::Error {
    fn from(e: Error) -> hyper::Error {
        if let Error::HyperError(e) = e {
            e
        } else {
            panic!("Invalid error")
        }
    }
}
