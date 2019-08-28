use std::thread;
use std::str::FromStr;
use std::convert::TryFrom;
use std::net::ToSocketAddrs;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{RwLock, Arc};
use std::io::Read;

use futures::{future, Async, Future, Stream};
use futures::future::IntoFuture;
use serde_derive::{Serialize, Deserialize};
use hyper::{Client, Body, Request, Response, Server, Method};
use hyper::body::Payload;
use hyper::service::service_fn;
use bytes::{BytesMut, Buf, BufMut, IntoBuf};
use tokio_codec::{Encoder, Decoder};
use tokio_io::{AsyncRead, AsyncWrite};

use macro_hack::try_future;

type ServerVersion = u64;
type ServerTree = Vec<(ServerVersion, crate::Tree)>;

#[derive(Deserialize, Serialize, Debug)]
struct Message(ServerTree);
struct MessageCodec;

impl Decoder for MessageCodec {
	type Item = Message;
	type Error = crate::Error;

	fn decode(&mut self, buf: &mut BytesMut) -> Result<Option<Message>, crate::Error> {
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

	fn encode(&mut self, msg: Message, buf: &mut BytesMut) -> Result<(), crate::Error> {
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
        futures::task::current().notify();
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
				println!("{:?}", data);
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
						{
						let mut serv_data = data.data.write().expect("RwLock starvation !");
						let last_version = if serv_data.len() > 0 { serv_data[serv_data.len()-1].0 } else { 1 };
						serv_data.push((last_version, body));
						}
						println!("{:?}", data);
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
    buf: BytesMut,
    fut: Box<dyn Stream<Item=hyper::Chunk, Error=hyper::Error> + Send>
}

//impl Stream for MessageClient {
//	type Item = Message;
//	type Error = hyper::Error;
//
//	fn poll(&mut self) -> Result<Async<Option<Message>>, hyper::Error> {
//		println!("called !");
//		match self.body.poll_data() {
//			Ok(Async::Ready(Some(d))) => {
//				let mut codec = MessageCodec;
//				let src_bytes = d.into_bytes();
//				self.buf.reserve(src_bytes.len());
//				self.buf.put(src_bytes);
//				match codec.decode(&mut self.buf) {
//					Ok(Some(x)) => Ok(Async::Ready(Some(x))),
//					_ => Ok(Async::NotReady)
//				}
//			},
//			Err(e) => Err(e.into()),
//			_ => Ok(Async::NotReady)
//		}
//	}
//}

//impl std::io::Write for MessageClient {
//	fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
//		self.buf.reserve(buf.len());
//		self.buf.put(buf);
//		Ok(buf.len())
//	}
//
//	fn flush(&mut self) -> std::io::Result<()> {
//		Ok(())
//	}
//}
//
//impl AsyncWrite for MessageClient {
//	fn shutdown(&mut self) -> std::io::Result<Async<()>> {
//		// dropping the body
//		self.body = Body::default();
//		Ok(Async::Ready(()))
//	}
//}

impl std::io::Read for MessageClient {
	fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize>  {
		let offset = self.buf
			.iter()
			.zip(buf.iter_mut())
			.map(|(&x, dest)| *dest = x)
			.count();
		self.buf.advance(offset);
		Ok(offset)
	}
}

impl AsyncRead for MessageClient {
    fn poll_read(&mut self, buf: &mut [u8]) -> futures::Poll<usize, std::io::Error> {
        println!("plled");
        match self.fut.poll() {
            Ok(Async::Ready(Some(chunk))) => {
                let chunk = chunk.into_bytes();
                self.buf.reserve(chunk.len());
                self.buf.put(chunk);
                Ok(Async::Ready(self.read(buf)?))
            },
            _ => Ok(Async::NotReady),
            Err(e) => Err(crate::Error::from(e).into())
        }
    }
}

fn web_client(conn: Connection) -> Result<impl Future<Item=(), Error=()>, crate::Error> {
	let dbus_conn = dbus::Connection::get_private(dbus::BusType::Session)?;
	let dbus_path = dbus_conn.with_path("fr.nightmared.tag_aggregator", "/fr/nightmared/tag_aggregator", 500);

	let client = Client::new();
	let server_url = format!("http{}://{}:{}/data.json", if conn.use_tls { "s" } else { "" }, conn.server_name, conn.port);
	let fut = client
		.get(hyper::Uri::from_str(&server_url).unwrap())
		.and_then(|res| {
            let frames = tokio_codec::FramedRead::new(MessageClient {
				fut: Box::new(res.into_body()),
				buf: BytesMut::new()
			}, MessageCodec);

			Ok(frames.for_each(|msg| {
				println!("{:?}", msg);
                Err(crate::Error::IOError(std::io::Error::from(std::io::ErrorKind::AddrInUse)))
				//Ok(())
			}).into_future()
            )
		})
		.map_err(|e| {eprintln!("{:?}", e);})
        .and_then(|_| Ok(()));
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
