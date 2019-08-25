use std::thread;
use std::convert::TryFrom;
use std::net::ToSocketAddrs;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, Arc};

use futures::{future, Async, Future, Stream};

use serde_derive::{Serialize, Deserialize};
use hyper::{Body, Request, Response, Server};
use hyper::service::service_fn;

use crate::{lib, utils};

type ServerTree = Vec<(u64, lib::Tree)>;

#[derive(Deserialize, Serialize, Debug)]
pub(crate) struct Connection {
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
        if self.updated.load(Ordering::Acquire) {
            let serv_data = self.data.lock().expect("Mutex starvation !");
            return Ok(Async::Ready(Some(serde_json::to_vec(&serv_data[serv_data.len()-1])?)))
        }
        Ok(Async::NotReady)
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub(crate) struct ServerConfig {
    conn: Connection,
    data_storage: String
}

#[derive(Serialize, Deserialize, Debug)]
pub(crate) struct ClientConfig {
    conn: Connection
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
/*
fn tcp_client(conn: Connection) -> Result<(), lib::Error> {
    let mut stream = get_stream(&conn)?;

    let poll = Poll::new()?;
    let mut events = Events::with_capacity(2);

    poll.register(&stream, Token(0), Ready::readable() | mio::unix::UnixReady::hup(), PollOpt::edge())?;

    let mut data = Vec::new();
    let mut buf = Vec::new();

    let dbus_conn = dbus::Connection::get_private(dbus::BusType::Session)?;
    let dbus_path = dbus_conn.with_path("fr.nightmared.tag_aggregator", "/fr/nightmared/tag_aggregator", 500);

    loop {
        poll.poll(&mut events, None)?;

        for event in &events {
            if event.token() == Token(0) {
                loop{
                    if let Err(e) = stream.read_to_end(&mut buf) {
                        if mio::unix::UnixReady::from(event.readiness()).is_hup()
                            || e.kind() == std::io::ErrorKind::ConnectionAborted
                            || e.kind() == std::io::ErrorKind::ConnectionReset {

                            // reset the connection and start again
                            poll.deregister(&stream)?;
                            stream.shutdown(std::net::Shutdown::Both)?;
                            stream = get_stream(&conn)?;
                            poll.register(&stream, Token(0), Ready::readable() | mio::unix::UnixReady::hup(), PollOpt::edge())?;
                            if let Ok(serv_data) = serde_json::from_slice::<ServerData>(&data) {
                                // send server_data
                                send_to_dbus_server(&dbus_path, serv_data)?;
                                break;
                            }
                            data.clear();
                            break;
                        }
                    } else {
                        data.append(&mut buf);
                    }
                }
            }

        }
    }
}
*/

pub(crate) fn run_web_server(config: ServerConfig) {
    let data = utils::load_app_data("server_data.json").expect("Could not open the server_data.json file");
    tcp_server(config, ServerState::new(data));
}

/*
pub(crate) fn run_web_client(conf: ClientConfig) {
    thread::spawn(move || {
        // connect to the dbus server
        tcp_client(conf.conn).unwrap();
    });
}
*/
