extern crate serde_derive;
extern crate gtk;
extern crate gio;

use std::sync::{Mutex, Arc};
use std::collections::HashMap;
use std::thread;
use std::env;
use std::fs::File;
use std::io::prelude::*;
use std::path::Path;
use std::sync::mpsc::{Sender, Receiver, channel};

use gtk::prelude::*;
use gio::prelude::*;
use gtk::{Application, ApplicationWindow, TreeView, TreeViewColumnBuilder, TreeStore};
use gtk::Type;

use serde_derive::{Serialize, Deserialize};

use dbus::{Connection, BusType, NameFlag, tree::Factory};

#[derive(Serialize, Deserialize, Debug)]
struct Entry {
    name: String,
    url: String
}

#[derive(Serialize, Deserialize, Debug)]
struct InternalData {
    #[serde(flatten)]
    tree: HashMap<String, Vec<Entry>>
}

fn add_entry(msg: &dbus::Message, data: &Mutex<InternalData>, category: String, title: String, url: String, tx: &Mutex<Sender<()>>) -> dbus::tree::MethodResult {
    match data.lock() {
        Ok(mut id) => {
            match id.tree.get_mut(&category) {
                Some(x) => x.push(Entry { name: title, url }),
                None => { id.tree.insert("default".into(), vec![Entry { name: title, url }]); }
            };
            match tx.lock() {
                Ok(x) => x.send(()).expect("Message sending failed"),
                Err(_) => return Ok(vec!(msg.method_return().append1(false)))
            };
            Ok(vec!(msg.method_return().append1(true)))
        },
        // A thread panicked while holding a mutex
        Err(_) => Ok(vec!(msg.method_return().append1(false)))
    }
}

fn on_update(data: Arc<Mutex<InternalData>>, store: TreeStore, view: TreeView) {
    let data_locked = data.lock().expect("Mutex starvation !");
    for key in data_locked.tree.keys() {
        let default = store.get_iter_first().unwrap();
        for v in data_locked.tree.get(key).unwrap() {
            let row  = store.append(Some(&default));
            store.set(&row, &[0, 1], &[&v.name, &v.url]);
        }
    }
    view.show_all();
}

fn gtk_handler(data: Arc<Mutex<InternalData>>, rx: Receiver<()>) {
    let app = Application::new(Some("fr.nightmared.tag_aggregator.gtk"), Default::default()).expect("starting the gtk application failed");
    let (itx, irx) = channel();
    app.connect_activate(move |app| {
        let win = ApplicationWindow::new(app);
        win.set_title("Tab Aggregator");

        let tree = TreeStore::new(&[Type::String, Type::String]);
        let _default = tree.append(None);

        let (gtx, grx) = glib::MainContext::channel(glib::PRIORITY_DEFAULT);
        let itx = itx.clone();
        thread::spawn(move || {
            itx.send(gtx).unwrap();
        });

        let treeview = TreeView::new_with_model(&tree);
        treeview.append_column(&TreeViewColumnBuilder::new().title("Name").build());
        treeview.append_column(&TreeViewColumnBuilder::new().title("URL").build());

        win.add(&treeview);

        let data = data.clone();
        grx.attach(None, move |_| {
            let data = data.clone();
            let tree = tree.clone();
            let treeview  = treeview.clone();
            on_update(data, tree, treeview);
            gtk::Continue(true)
        });
        

        win.show_all();
    });
    thread::spawn(move || {
        let tx = irx.recv().unwrap();
        loop {
            if let Ok(()) = rx.recv() {
                tx.send(()).unwrap();
            }
        }
    });



    app.run(&[]);
}

fn dbus_client(data: Arc<Mutex<InternalData>>, tx: Sender<()>) -> Result<(), dbus::Error> {
    let data2 = data.clone();
    let tx = Arc::new(Mutex::new(tx));
    let tx2 = tx.clone();
    let c = Connection::get_private(BusType::Session)?;
    c.register_name("fr.nightmared.tag_aggregator", NameFlag::ReplaceExisting as u32)?;
    let f = Factory::new_sync::<()>();
    let tree = f.tree(()).add(f.object_path("/fr/nightmared/tag_aggregator", ()).introspectable().add(
        f.interface("fr.nightmared.tag_aggregator", ()).add_m(
            f.method_sync("add", (), move |m| {
                let (title, url): (String, String) = m.msg.read2()?;
                add_entry(m.msg, &data, "default".into(), title, url, &tx)
            }).inarg::<String, _>("title")
              .inarg::<String, _>("url")
              .outarg::<bool,_>("success")
        ).add_m(
            f.method_sync("add_with_category", (), move |m| {
                let (category, title, url): (String, String, String) = m.msg.read3()?;
                add_entry(m.msg, &data2, category, title, url, &tx2)
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

fn main() -> std::io::Result<()> {
    let config_base_dir = env::var_os("XDG_CONFIG_HOME")
        .unwrap_or(env::var_os("HOME")
            .unwrap_or(env::current_dir()
                .expect("Couldn't get the current directory")
                    .into_os_string()));
    let config_file_path = Path::new(&config_base_dir).join("tag-aggregator/config.json");
    let mut fs = File::open(config_file_path)?;
    let mut conf_buf = Vec::new();
    fs.read_to_end(&mut conf_buf)?;
    let conf = InternalData::from(match serde_json::from_slice::<InternalData>(&conf_buf) {
        Ok(x) => x,
        Err(e) => {
            eprintln!("Unable to parse the config.json file: {:?}", e);
            return Err(std::io::Error::from(std::io::ErrorKind::InvalidData));
        }
    });


    let data = Arc::new(Mutex::new(conf));
    let (tx, rx) = channel();

    let dbus_client_data = data.clone();
    thread::spawn(move || dbus_client(dbus_client_data, tx));

    let gtk_app_data = data.clone();
    gtk_handler(gtk_app_data, rx);

    
    Ok(())
}
