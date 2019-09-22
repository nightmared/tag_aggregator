use std::convert::TryFrom;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{RwLock, Arc};
use std::io::Read;

use rouille::{Request, Response};
use serde_derive::{Deserialize, Serialize};

use libtag_aggregator::{server, utils, Tree};
use server::{Connection, ServerTree};


#[derive(Debug, Clone)]
struct ServerState {
	updated: Arc<AtomicBool>,
	data: Arc<RwLock<ServerTree>>
}

impl ServerState {
	fn new(tree: Arc<RwLock<ServerTree>>) -> Self {
		ServerState {
			updated: Arc::new(AtomicBool::new(false)),
			data: tree
		}
	}
}

#[derive(Serialize, Deserialize, Debug)]
struct ServerConfig {
	conn: Connection
}

#[derive(Serialize, Deserialize, Debug)]
struct HttpResponse {
	success: bool
}


fn req_to_json<T: for<'a> serde::Deserialize<'a>>(req: &Request) -> Result<T, libtag_aggregator::Error> {
	match req.data() {
		Some(mut d) => {
			let mut v = Vec::new();
			d.read_to_end(&mut v)?;
			Ok(serde_json::from_slice(&v)?)
		},
		None => Err(std::io::Error::new(std::io::ErrorKind::AlreadyExists, "The request was already polled").into())
	}

}

fn insert_entry(state: &ServerState, tree: Tree) {
	{
		let mut serv_data = state.data.write().expect("RwLock starvation !");
		let last_version = if serv_data.len() > 0 { serv_data[serv_data.len()-1].0 } else { 1 };
		serv_data.push((last_version+1, tree));
	}
	state.updated.store(true, Ordering::Release);
}


fn main() -> std::io::Result<()> {
	let conf: ServerConfig= utils::load_app_data("server_config.json")?;
    let state = ServerState::new(Arc::new(RwLock::new(utils::load_app_data("server_data.json")?)));

    rouille::start_server(std::net::SocketAddr::try_from(&conf.conn).unwrap(), move |req| {
        match (req.method(), req.raw_url()) {
            ("GET", "/data.json") => {
                Response::json(&*state.data
                    .read().expect("Poisoned RwLock !!!"))
            },
            ("POST", "/submit") => {
				match req_to_json(&req) {
					Ok(tree) => {
						insert_entry(&state, tree);
						Response::json(&HttpResponse { success: true })
					},
					Err(_) => Response::json(&HttpResponse { success: false })
				}
            },
			_ => Response::empty_404()
        }
    })
}
