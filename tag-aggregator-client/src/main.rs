use libtag_aggregator::{ui, dbus_client, server, utils};

use std::sync::{Mutex, Arc};
use std::sync::mpsc;


fn main() -> std::io::Result<()> {
    let conf = utils::load_app_data("client_config.json")?;
    let data = Arc::new(Mutex::new(utils::load_app_data("client_data.json")?));

    let (tx, rx) = mpsc::channel();
    dbus_client::run_dbus(&data, rx);
    server::run_web_client(conf);
    ui::run_ui(&data, tx);
    Ok(())
}
