pub(crate) mod lib;
mod ui;
mod dbus;

use lib::InternalData;

use std::sync::{Mutex, Arc};
use std::thread;
use std::env;
use std::fs::File;
use std::io::prelude::*;
use std::path::Path;
use std::sync::mpsc;

fn main() -> std::io::Result<()> {
    let config_base_dir = env::var_os("XDG_CONFIG_HOME")
        .unwrap_or(env::var_os("HOME")
            .unwrap_or(env::current_dir()
                .expect("Couldn't get the current directory")
                    .into_os_string()));
    let config_file_path = Path::new(&config_base_dir).join("tag-aggregator/config.json");
    let mut fs = File::open(config_file_path)?;
    let mut conf_buf = Vec::new();
    fs.read_to_end(&mut conf_buf)?;
    let conf = InternalData::from(match serde_json::from_slice::<InternalData>(&conf_buf) {
        Ok(x) => x,
        Err(e) => {
            eprintln!("Unable to parse the config.json file: {:?}", e);
            return Err(std::io::Error::from(std::io::ErrorKind::InvalidData));
        }
    });


    let data = Arc::new(Mutex::new(conf));
    let (tx, rx) = mpsc::channel();

    let dbus_client_data = data.clone();
    thread::spawn(move || dbus::dbus_client(dbus_client_data, rx));

    ui::run_ui(data, tx);
    Ok(())
}
