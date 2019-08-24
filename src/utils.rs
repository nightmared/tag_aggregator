use std::env;
use std::fs::File;
use std::io::prelude::*;
use std::path::Path;

use serde::Deserialize;

pub(crate) fn load_app_data<T: for <'a> Deserialize<'a>>(file_name: &str) -> std::io::Result<T> {
    let config_base_dir = env::var_os("XDG_CONFIG_HOME")
        .unwrap_or(env::var_os("HOME")
            .unwrap_or(env::current_dir()
                .expect("Couldn't get the current directory")
                    .into_os_string()));
    let mut config_file_path = Path::new(&config_base_dir).join("tag-aggregator/");
    config_file_path.push(file_name);
    // if you filename is not a valid utf-8 name, it's YOUR problem (like using a weird path and/or a
    // weird OS)
    load_json(&config_file_path.to_string_lossy())
}

pub(crate) fn load_json<T: for <'a> Deserialize<'a>>(file_name: &str) -> std::io::Result<T> {
    let mut fs = File::open(file_name).map_err(|e| {
        eprintln!("Unable to open the file '{}'.", file_name);
        e
    })?;
    let mut conf_buf = Vec::new();
    fs.read_to_end(&mut conf_buf)?;
    serde_json::from_slice::<T>(&conf_buf).map_err(|e| {
        eprintln!("Unable to parse the file '{}': {:?}", file_name, e);
        std::io::Error::from(std::io::ErrorKind::InvalidData)
    })
}
