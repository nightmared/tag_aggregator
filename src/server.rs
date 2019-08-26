use std::thread;
use std::str::FromStr;
use std::convert::TryFrom;
use std::net::ToSocketAddrs;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, Arc};

use futures::{future, Async, Future, Stream};

use serde_derive::{Serialize, Deserialize};
use hyper::{Client, Body, Request, Response, Server};
use hyper::service::service_fn;

use crate::{lib, utils};

type ServerVersion = u64;
type MessageSize = u64;
type ServerTree = Vec<(ServerVersion, lib::Tree)>;

#[derive(Deserialize, Serialize, Debug)]
pub(crate) struct Connection {
    use_tls: bool,
    server_name: String,
    port: u16
}

#[derive(Debug, Clone)]
pub(crate) struct ServerState {
    updated: Arc<AtomicBool>,
    data: Arc<Mutex<ServerTree>>
}

impl ServerState {
    fn new(tree: ServerTree) -> Self {
        ServerState {
            updated: Arc::new(AtomicBool::new(false)),
            data: Arc::new(Mutex::new(tree))
        }
    }
}

impl Stream for ServerState {
    type Item = Vec<u8>;
    type Error = std::io::Error;

    fn poll(&mut self) -> Result<Async<Option<Self::Item>>, Self::Error> {
        if self.updated.compare_and_swap(true, false, Ordering::AcqRel) {
            let serv_data = self.data.lock().expect("Mutex starvation !");
            let mut tmp = serde_json::to_vec(&serv_data[serv_data.len()-1])?;
            let size = (tmp.len() as MessageSize).to_be_bytes();
            let mut data: Vec<u8> = Vec::from(&size as &[u8]);
            data.append(&mut tmp);
            return Ok(Async::Ready(Some(data)))
        }
        Ok(Async::NotReady)
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub(crate) struct ServerConfig {
    conn: Connection,
    data_storage: String
}


impl TryFrom<&Connection> for std::net::SocketAddr {
    type Error = lib::Error;

    fn try_from(conn: &Connection) -> Result<Self, lib::Error> {
        Ok((conn.server_name.as_str(), conn.port)
                .to_socket_addrs()?
                .next().expect("No matching server name found, check your config and you DNS resolution"))
    }
}

fn server_main(data: ServerState) -> impl Fn(Request<Body>) -> Box<dyn Future<Item=Response<Body>, Error=hyper::Error> + Send> {
    move |req| {
        Box::new(future::ok(Response::builder()
                .body(Body::wrap_stream(data.clone())).unwrap()))
    }
}

fn tcp_server(conf: ServerConfig, data: ServerState) -> Result<(), lib::Error> {
    let server = Server::bind(&std::net::SocketAddr::try_from(&conf.conn)?)
        .serve(move || service_fn(server_main(data.clone())))
        .map_err(|_|{});
    hyper::rt::run(server);
    Ok(())
}

fn send_to_dbus_server(conn: &dbus::ConnPath<&dbus::Connection>, serv_data: ServerTree) -> Result<(), lib::Error> {
    let interface = "fr.nightmared.tag_aggregator".into();
    let cur_version = conn.method_call_with_args(&interface, &"get_version".into(), |_|{})?.read1()?;

    for (version, tree) in serv_data {
        if version <= cur_version {
            continue;
        }
        for category in tree.keys() {
            for entry in tree.get(category).unwrap() {
                conn.method_call_with_args(&interface, &"add_with_category".into(), |msg| {
                    let mut args = dbus::arg::IterAppend::new(msg);
                    args.append(category);
                    args.append(&entry.name);
                    args.append(&entry.url);
                })?;
            }
        }
        conn.method_call_with_args(&interface, &"set_version".into(), |msg| {
            let mut args = dbus::arg::IterAppend::new(msg);
            args.append(version);
        })?;
    }
    Ok(())
}

pub(crate) fn run_web_server(config: ServerConfig) {
    let data = utils::load_app_data("server_data.json").expect("Could not open the server_data.json file");
    tcp_server(config, ServerState::new(data));
}

#[derive(Serialize, Deserialize, Debug)]
pub(crate) struct ClientConfig {
    conn: Connection
}

fn web_client(conn: Connection) -> Result<(), lib::Error> {
    let dbus_conn = dbus::Connection::get_private(dbus::BusType::Session)?;
    let dbus_path = dbus_conn.with_path("fr.nightmared.tag_aggregator", "/fr/nightmared/tag_aggregator", 500);

    let client = Client::new();
    let server_url = format!("http{}://{}:{}", if conn.use_tls { "s" } else { "" }, conn.server_name, conn.port);
    let fut = client
        .get(hyper::Uri::from_str(&server_url).unwrap())
        .and_then(|res| {
            res.into_body().concat2()
        })
        .and_then(|body| {
            Ok(())
        })
        .map_err(|_| {
        });
    hyper::rt::spawn(fut);
    unimplemented!()
}

pub(crate) fn run_web_client(conf: ClientConfig) {
    thread::spawn(move || {
        // connect to the dbus server
        web_client(conf.conn).unwrap();
    });
}
