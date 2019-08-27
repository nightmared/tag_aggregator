use libtag_aggregator::{server, utils};

use std::sync::{RwLock, Arc};

fn main() -> std::io::Result<()> {
    let conf = utils::load_app_data("server_config.json")?;
    let data = Arc::new(RwLock::new(utils::load_app_data("server_data.json")?));

    server::run_web_server(conf, data);
    Ok(())
}
