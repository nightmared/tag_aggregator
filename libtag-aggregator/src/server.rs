use std::convert::TryFrom;
use std::net::ToSocketAddrs;
use serde_derive::{Deserialize, Serialize};

pub type ServerVersion = u64;
pub type ServerTree = Vec<(ServerVersion, crate::Tree)>;

#[derive(Deserialize, Serialize, Debug)]
pub struct Message(ServerTree);

#[derive(Deserialize, Serialize, Debug)]
pub struct Connection {
	pub use_tls: bool,
	pub server_name: String,
	pub port: u16
}

impl TryFrom<&Connection> for std::net::SocketAddr {
   type Error = crate::Error;

   fn try_from(conn: &Connection) -> Result<Self, crate::Error> {
       Ok((conn.server_name.as_str(), conn.port)
           .to_socket_addrs()?
           .next().expect("No matching server name found, check your config and you DNS resolution"))
   }
}