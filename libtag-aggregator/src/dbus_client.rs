use std::thread;
use std::sync::{Mutex, Arc};
use std::sync::mpsc;

use dbus::{Connection, BusType, NameFlag, tree::Factory};

use crate::Entry;

fn add_entry(msg: &dbus::Message, data: &Mutex<crate::InternalData>, category: String, title: String, url: String) -> dbus::tree::MethodResult {
	match data.lock() {
		Ok(mut id) => {
			match id.tree.get_mut(&category) {
				Some(x) => x.push(Entry { name: title, url }),
				None => { id.tree.insert("default".into(), vec![Entry { name: title, url }]); }
			};
			id.sender.as_ref().unwrap().send(()).expect("Message sending failed");
			Ok(vec!(msg.method_return().append1(true)))
		},
		// A thread panicked while holding a mutex
		Err(_) => Ok(vec!(msg.method_return().append1(false)))
	}
}

fn dbus_client(data: Arc<Mutex<crate::InternalData>>, rx: mpsc::Receiver<glib::Sender<()>>) -> Result<(), dbus::Error> {
	let data2 = data.clone();
	let data3 = data.clone();
	let data4 = data.clone();
	{
		let mut id =  data.lock().expect("Couldn't acquire the mutex. Starvation ?");
		id.sender = Some(rx.recv().expect("Couldn't obtain the IPC channel"));
	}
	let c = Connection::get_private(BusType::Session)?;
	c.register_name("fr.nightmared.tag_aggregator", NameFlag::ReplaceExisting as u32)?;
	let f = Factory::new_sync::<()>();
	let tree = f.tree(()).add(f.object_path("/fr/nightmared/tag_aggregator", ()).introspectable().add(
		f.interface("fr.nightmared.tag_aggregator", ()).add_m(
			f.method_sync("get_version", (), move |m| {
				let id = data3.lock().expect("Couldn't lock a mutex !");
				Ok(vec![m.msg.method_return().append1(id.pos.unwrap_or(0))])
			}).outarg::<u64,_>("version")
		).add_m(
			f.method_sync("set_version", (), move |m| {
				let mut id = data4.lock().expect("Couldn't lock a mutex !");
				let new_version = m.msg.read1()?;
				if new_version > id.pos.unwrap_or(0) {
					id.pos = Some(new_version);
				}
				Ok(vec![m.msg.method_return().append1(true)])
			}).inarg::<u64,_>("version")
			  .outarg::<bool,_>("succes")
		).add_m(
			f.method_sync("add", (), move |m| {
				let (title, url): (String, String) = m.msg.read2()?;
				add_entry(m.msg, &data, "default".into(), title, url)
			}).inarg::<String, _>("title")
			  .inarg::<String, _>("url")
			  .outarg::<bool,_>("success")
		).add_m(
			f.method_sync("add_with_category", (), move |m| {
				let (category, title, url): (String, String, String) = m.msg.read3()?;
				add_entry(m.msg, &data2, category, title, url)
			}).inarg::<String, _>("category")
			  .inarg::<String, _>("title")
			  .inarg::<String, _>("url")
			  .outarg::<bool,_>("success")
		)
	));
	tree.set_registered(&c, true)?;
	c.add_handler(tree);
	loop { c.incoming(1000).next(); }
}

pub fn run_dbus(data: &Arc<Mutex<crate::InternalData>>, rx: mpsc::Receiver<glib::Sender<()>>) {
	let data = data.clone();
	thread::spawn(move || dbus_client(data, rx).unwrap());
}

