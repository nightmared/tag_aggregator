use std::{fmt, thread};

use curl::easy::Easy;

use serde_derive::{Serialize, Deserialize};

use crate::lib;

#[derive(Debug)]
pub(crate) enum Connection {
    HTTP(String),
    Unix(String)
}

impl<'de> serde::Deserialize<'de> for Connection {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct ConnectionVisitor;
        impl<'de> serde::de::Visitor<'de> for ConnectionVisitor {
            type Value = Connection;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("'http[s]://server_name[:port]/path' or 'file://path'")
            }

            fn visit_str<E: serde::de::Error>(self, value: &str) -> Result<Connection, E> {
                if value.starts_with("file://") && value.len() > 7 {
                    Ok(Connection::Unix(value[7..].into()))
                } else {
                    Ok(Connection::HTTP(value.into()))
                }
            }
        }
        deserializer.deserialize_identifier(ConnectionVisitor)
    }
}

impl serde::Serialize for Connection {
    fn serialize<S: serde::Serializer>(self: &Self, serializer: S) -> Result<S::Ok, S::Error> {
        let tmp = match self {
            Connection::HTTP(s) => s.clone(),
            Connection::Unix(s) => format!("file://{}", s)
        };
        serializer.serialize_str(&tmp)
    }
}

#[derive(Serialize, Deserialize, Debug)]
struct ServerConfig {
    conn: Connection,
    data_storage: String
}

#[derive(Serialize, Deserialize, Debug)]
struct ServerData {
    pos: Option<u64>,
    tree: lib::Tree
}

fn retrieve<T: for <'a> serde::Deserialize<'a>>(conn: Connection) -> Result<T, lib::Error> {
    let mut buf = Vec::new();
    let mut easy = Easy::new();
    match conn {
        Connection::HTTP(name) => easy.url(&name)?,
        Connection::Unix(file_name) => easy.unix_socket(&file_name)?
    }
    {
    let mut transfer = easy.transfer();
    transfer.write_function(|data| {
        buf.extend_from_slice(data);
        Ok(data.len())
    })?;
    transfer.perform()?;
    }
    Ok(serde_json::from_slice::<T>(&buf)?)
}

pub(crate) fn run_server_client(conn: Connection) {
    thread::spawn(move || {
        // connect to the dbus server

    });
}
