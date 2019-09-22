use std::sync::{Mutex, Arc};
use std::sync::mpsc;
use std::thread;

use serde_derive::{Serialize, Deserialize};

use libtag_aggregator::{dbus_client, utils, server};
mod ui;

#[derive(Serialize, Deserialize, Debug)]
pub struct ClientConfig {
	pub conn: server::Connection
}

fn run_web_client(conf: ClientConfig)  -> Result<(), libtag_aggregator::Error> {
    let conn = conf.conn;
 	let server_url = format!("http{}://{}:{}/data.json", if conn.use_tls { "s" } else { "" }, conn.server_name, conn.port);



	 // synchronize every 5 minutes
	 loop {
		if let Ok(mut req) = reqwest::get(&server_url) {
			if let Ok(raw) = req.text() {
				let res = serde_json::from_str(&raw)?;
				send_to_dbus_server(res)?;
			}
		}
		thread::sleep(std::time::Duration::from_secs(5*60));
	 }

}

fn main() -> std::io::Result<()> {
	let conf = utils::load_app_data("client_config.json")?;
	let data = Arc::new(Mutex::new(utils::load_app_data("client_data.json")?));

	let (tx, rx) = mpsc::channel();
	dbus_client::run_dbus(&data, rx);
	thread::spawn(move || run_web_client(conf) );
	ui::run_ui(&data, tx);
	Ok(())
}

fn send_to_dbus_server(serv_data: server::ServerTree) -> Result<(), libtag_aggregator::Error> {
	let conn = dbus::Connection::get_private(dbus::BusType::Session)?;
    let path = conn.with_path("fr.nightmared.tag_aggregator", "/fr/nightmared/tag_aggregator", 500);

	let interface = "fr.nightmared.tag_aggregator".into();
	let cur_version = path.method_call_with_args(&interface, &"get_version".into(), |_|{})?.read1()?;

	for (version, tree) in serv_data {
		if version <= cur_version {
			continue;
		}
		for category in tree.keys() {
			for entry in tree.get(category).unwrap() {
				path.method_call_with_args(&interface, &"add_with_category".into(), |msg| {
					let mut args = dbus::arg::IterAppend::new(msg);
					args.append(category);
					args.append(&entry.name);
					args.append(&entry.url);
				})?;
			}
		}
		path.method_call_with_args(&interface, &"set_version".into(), |msg| {
			let mut args = dbus::arg::IterAppend::new(msg);
			args.append(version);
		})?;
	}
	Ok(())
}