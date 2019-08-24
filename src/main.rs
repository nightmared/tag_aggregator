pub(crate) mod lib;
mod ui;
mod dbus;
mod server;
mod utils;

use std::sync::{Mutex, Arc};
use std::sync::mpsc;


fn main() -> std::io::Result<()> {
    let conf = utils::load_config("data.json")?;
    
    let data = Arc::new(Mutex::new(conf));

    let (tx, rx) = mpsc::channel();
    dbus::run_dbus(&data, rx);
    ui::run_ui(&data, tx);

    Ok(())
}
