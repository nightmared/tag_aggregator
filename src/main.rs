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
use std::sync::mpsc;
use std::process::{Command, Stdio};

use gtk::prelude::*;
use gio::prelude::*;
use gtk::{Application, ApplicationWindow, TreeView, TreeViewColumn, TreeStore, CellRendererText};
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

fn add_entry(msg: &dbus::Message, data: &Mutex<InternalData>, category: String, title: String, url: String, tx: &Mutex<mpsc::Sender<()>>) -> dbus::tree::MethodResult {
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

fn on_click(view: &TreeView, path: &gtk::TreePath, col_view: &TreeViewColumn) {
    if col_view.get_name() == Some("url_col".into()) {
        if let Some((model, _)) = view.get_selection().get_selected() {
            if let Some(iter) = model.get_iter(path) {
                if let Some(url) = model.get_value(&iter, 1).get::<String>() {
                    if url.starts_with("file://") || url.starts_with("http://") || url.starts_with("https://") {
                        Command::new("xdg-open")
                            .arg(url)
                            .stdout(Stdio::null())
                            .stdin(Stdio::null())
                            .stderr(Stdio::null())
                            .spawn()
                            .expect("Couldn't spawn xdg-open");
                    }
                }

            }
        }
    }
}

fn gtk_handler(data: Arc<Mutex<InternalData>>, itx: mpsc::Sender<glib::Sender<()>>) -> impl Fn(&Application) {
    move |app| {
        let win = ApplicationWindow::new(app);
        win.set_title("Tab Aggregator");

        let sw = gtk::ScrolledWindow::new(None::<&gtk::Adjustment>, None::<&gtk::Adjustment>);
        //sw.set_shadow_type(gtk::ShadowType::EtchedIn);
        sw.set_policy(gtk::PolicyType::Automatic, gtk::PolicyType::Automatic);

        let tree = TreeStore::new(&[Type::String, Type::String]);

        let treeview = TreeView::new_with_model(&tree);
        treeview.set_vexpand(true);
        treeview.set_reorderable(true);
        treeview.set_activate_on_single_click(true);

        let col_name_renderer = CellRendererText::new();
        let col_name_view = TreeViewColumn::new();
        col_name_view.pack_start(&col_name_renderer, true);
        col_name_view.set_title("Name");
        col_name_view.add_attribute(&col_name_renderer, "text", 0);

        treeview.append_column(&col_name_view);

        let col_url_renderer = CellRendererText::new();
        let col_url_view = TreeViewColumn::new();
        col_url_view.pack_start(&col_url_renderer, true);
        col_url_view.set_title("Url");
        col_url_view.set_name("url_col");
        col_url_view.add_attribute(&col_url_renderer, "text", 1);
        col_url_renderer.set_property_foreground(Some(&"blue"));
        col_url_renderer.set_property_underline(pango::Underline::Single);

        treeview.connect_row_activated(on_click);

        treeview.append_column(&col_url_view);

        let _default = tree.append(None);
        tree.set(&_default, &[0, 1], &[&"Main", &"lol"]);

        sw.add(&treeview);

        win.add(&sw);

        let (gtx, grx) = glib::MainContext::channel(glib::PRIORITY_DEFAULT);
        let itx = itx.clone();
        itx.send(gtx).unwrap();

        let data = data.clone();
        grx.attach(None, move |_| {
            on_update(data.clone(), tree.clone(), treeview.clone());
            gtk::Continue(true)
        });
        

        win.show_all();
    }
}

fn dbus_client(data: Arc<Mutex<InternalData>>, tx: mpsc::Sender<()>) -> Result<(), dbus::Error> {
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
    let (tx, rx) = mpsc::channel();

    let dbus_client_data = data.clone();
    thread::spawn(move || dbus_client(dbus_client_data, tx));

    let app = Application::new(Some("fr.nightmared.tag_aggregator.gtk"), Default::default()).expect("starting the gtk application failed");

    // temporary handles that allows the gtk thread to send a glib channel to the "proxy" thread
    // that forwards update messages to the UI
    let (itx, irx) = mpsc::channel();
    app.connect_activate(gtk_handler(data, itx));
    thread::spawn(move || {
        let tx = irx.recv().expect("Couldn't obtain a handle to notify the Gtk+ thread");
        drop(irx);
        loop {
            if let Ok(()) = rx.recv() {
                tx.send(()).expect("Couldn't notify the Gtk+ thread to update its dataset");
            }
        }
    });

    app.run(&[]);
    Ok(())
}
