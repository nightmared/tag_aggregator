use std::thread;
use std::str::FromStr;
use std::convert::TryFrom;
use std::net::ToSocketAddrs;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{RwLock, Arc};

use futures::{future, Async, Future, Stream};
use serde_derive::{Serialize, Deserialize};
use hyper::{Client, Body, Request, Response, Server, Method};
use hyper::body::Payload;
use hyper::service::service_fn;
use bytes::{BytesMut, Buf, BufMut, IntoBuf};
use tokio_codec::{Encoder, Decoder};

use macro_hack::try_future;

type ServerVersion = u64;
type ServerTree = Vec<(ServerVersion, crate::Tree)>;

#[derive(Deserialize, Serialize, Debug)]
struct Message(ServerTree);
struct MessageCodec;

impl Decoder for MessageCodec {
	type Item = Message;
	type Error = crate::Error;

	fn decode(self: &mut Self, buf: &mut BytesMut) -> Result<Option<Message>, crate::Error> {
		if buf.len() > 8 {
			let buf = &mut buf.clone().into_buf();
			let size = buf.get_u64_be() as usize;
			if buf.remaining() < size {
				return Ok(None);
			}
			let mut obj = Vec::with_capacity(size);
			buf.copy_to_slice(&mut obj);
			Ok(Some(serde_json::from_slice(&obj)?))
		} else {
			Ok(None)
		}
	}
}

impl Encoder for MessageCodec {
	type Item = Message;
	type Error = crate::Error;

	fn encode(self: &mut Self, msg: Message, buf: &mut BytesMut) -> Result<(), crate::Error> {
		let data = serde_json::to_vec(&msg)?;
		let size = data.len();
		buf.reserve(size+8);
		buf.put_u64_be(size as u64);
		buf.put_slice(&data);
		Ok(())
	}
}

#[derive(Deserialize, Serialize, Debug)]
pub struct Connection {
	use_tls: bool,
	server_name: String,
	port: u16
}

#[derive(Debug, Clone)]
pub struct ServerState {
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

impl Stream for ServerState {
	type Item = bytes::Bytes;
	type Error = std::io::Error;

	fn poll(&mut self) -> Result<Async<Option<Self::Item>>, Self::Error> {
		if self.updated.compare_and_swap(true, false, Ordering::AcqRel) {
			let serv_data = self.data.read().expect("RwLock starvation !");
			let mut buf = bytes::BytesMut::new();
			let mut codec = MessageCodec;
			codec.encode(Message(serv_data.to_vec()), &mut buf)?;
			return Ok(Async::Ready(Some(buf.into())))
		}
		Ok(Async::NotReady)
	}
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ServerConfig {
	conn: Connection
}


impl TryFrom<&Connection> for std::net::SocketAddr {
	type Error = crate::Error;

	fn try_from(conn: &Connection) -> Result<Self, crate::Error> {
		Ok((conn.server_name.as_str(), conn.port)
				.to_socket_addrs()?
				.next().expect("No matching server name found, check your config and you DNS resolution"))
	}
}


fn server_main(data: ServerState) -> impl Fn(Request<Body>) -> Box<dyn Future<Item=Response<Body>, Error=hyper::Error> + Send> {
	move |req| {
		match (req.method(), req.uri().path()) {
			(&Method::GET, "/data.json") => {
				Box::new(future::ok(Response::builder()
					.body(Body::wrap_stream(data.clone())).unwrap()))
			},
			(&Method::POST, "/add_entry") => {
				let data = data.clone();
				Box::new(req.into_body()
					.concat2()
					.map_err(|e| crate::Error::from(e))
					.and_then(move |str| {
						let body = try_future!(serde_json::from_slice(&str));
						let serv_data = data.data.write().expect("RwLock starvation !");
						data.updated.store(true, Ordering::Release);
						Box::new(future::ok(Response::builder()
							.status(200)
							.body(Body::from("{\"status\": \"success\"}"))
							.unwrap()))
					})
					// TODO: fix this to prevent panics
					.map_err(|e| hyper::Error::from(e)))
			},
			_ => {
				Box::new(future::ok(Response::builder()
					.status(404)
					.body(Body::from("Wrong server, buddy !"))
					.unwrap()))

			}
		}
	}
}

pub fn tcp_server(conf: ServerConfig, data: ServerState) -> Result<(), crate::Error> {
	let server = Server::bind(&std::net::SocketAddr::try_from(&conf.conn)?)
		.serve(move || service_fn(server_main(data.clone())))
		.map_err(|_|{});
	hyper::rt::run(server);
	Ok(())
}

fn send_to_dbus_server(conn: &dbus::ConnPath<&dbus::Connection>, serv_data: ServerTree) -> Result<(), crate::Error> {
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

pub fn run_web_server(config: ServerConfig, data: Arc<RwLock<ServerTree>>) {
	tcp_server(config, ServerState::new(data));
}

struct MessageClient {
	body: Body,
	buf: BytesMut
}

impl Stream for MessageClient {
	type Item = Message;
	type Error = hyper::Error;

	fn poll(self: &mut Self) -> Result<Async<Option<Message>>, hyper::Error> {
		match self.body.poll_data() {
			Ok(Async::Ready(Some(d))) => {
				let mut codec = MessageCodec;
				let src_bytes = d.into_bytes();
				self.buf.reserve(src_bytes.len());
				self.buf.put(src_bytes);
				match codec.decode(&mut self.buf) {
					Ok(Some(x)) => Ok(Async::Ready(Some(x))),
					_ => Ok(Async::NotReady)
				}
			},
			Err(e) => Err(e.into()),
			_ => Ok(Async::NotReady)
		}
	}
}

fn web_client(conn: Connection) -> Result<impl Future<Item=(), Error=()>, crate::Error> {
	let dbus_conn = dbus::Connection::get_private(dbus::BusType::Session)?;
	let dbus_path = dbus_conn.with_path("fr.nightmared.tag_aggregator", "/fr/nightmared/tag_aggregator", 500);

	let client = Client::new();
	let server_url = format!("http{}://{}:{}", if conn.use_tls { "s" } else { "" }, conn.server_name, conn.port);
	let fut = client
		.get(hyper::Uri::from_str(&server_url).unwrap())
		.and_then(|res| {
			MessageClient {
				body: res.into_body(),
				buf: BytesMut::new()
			}
			.for_each(|msg| {
				println!("{:?}", msg);
				Ok(())
			})
		})
		.map_err(|_| {});
	Ok(fut)
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ClientConfig {
	conn: Connection
}


pub fn run_web_client(conf: ClientConfig) {
	thread::spawn(move || {
		// connect to the dbus server
		hyper::rt::run(web_client(conf.conn).unwrap());
		println!("The web client exited, maybe the web server is down ?");
	});
}
