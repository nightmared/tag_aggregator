extern crate serde_derive;

use std::collections::HashMap;

use serde_derive::{Serialize, Deserialize};

#[derive(Serialize, Deserialize, Debug)]
pub(crate) struct InternalData {
    #[serde(flatten)]
    pub tree: HashMap<String, Vec<Entry>>,
    #[serde(skip)]
    pub sender: Option<glib::Sender<()>>
}

#[derive(Serialize, Deserialize, Debug)]
pub(crate) struct Entry {
    pub name: String,
    pub url: String
}
