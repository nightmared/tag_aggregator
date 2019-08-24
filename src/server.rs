use std::thread;
use std::io::Read;
use std::time::Duration;
use mio::net::TcpStream;
use mio::{Poll, PollOpt, Token, Events, Ready};
use std::net::ToSocketAddrs;

use serde_derive::{Serialize, Deserialize};

use crate::lib;

#[derive(Serialize, Deserialize, Debug)]
struct ServerData {
    trees: Vec<(u64, lib::Tree)>
}

#[derive(Deserialize, Serialize, Debug)]
pub(crate) struct Connection {
    server_name: String,
    port: u16
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

fn get_stream(conn: &Connection) -> Result<TcpStream, lib::Error> {
    let stream = TcpStream::connect(
        &(conn.server_name.as_str(), conn.port)
            .to_socket_addrs()?
            .next().expect("No matching server name found, check your config and you DNS resolution")
        )?;
    stream.set_keepalive(Some(Duration::new(5, 0)))?;
    Ok(stream)
}

fn send_to_dbus_server(conn: &dbus::ConnPath<&dbus::Connection>, serv_data: ServerData) -> Result<(), lib::Error> {
    let interface = "fr.nightmared.tag_aggregator".into();
    let cur_version = conn.method_call_with_args(&interface, &"get_version".into(), |_|{})?.read1()?;

    for (version, tree) in serv_data.trees {
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

fn dbus_web_client(conn: Connection) -> Result<(), lib::Error> {
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
                        if e.kind() == std::io::ErrorKind::ConnectionAborted
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

pub(crate) fn run_web_client(conf: ClientConfig) {
    thread::spawn(move || {
        // connect to the dbus server
        dbus_web_client(conf.conn).unwrap();
    });
}
