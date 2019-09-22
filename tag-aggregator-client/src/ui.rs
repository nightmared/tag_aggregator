use std::process::{Command, Stdio};
use std::sync::{Mutex, Arc};
use std::sync::mpsc;

use gtk::prelude::*;
use gio::prelude::*;
use gtk::{Application, ApplicationWindow, TreeView, TreeViewColumn, TreeStore, CellRendererText};
use gtk::Type;

use libtag_aggregator::InternalData;

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
		col_name_view.set_resizable(true);

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
		tree.set_value(&_default, 0, &"Default category".to_value());

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

pub fn run_ui(data: &Arc<Mutex<InternalData>>, tx: mpsc::Sender<glib::Sender<()>>) {
	let app = Application::new(Some("fr.nightmared.tag_aggregator.gtk"), Default::default()).expect("starting the gtk application failed");

	app.connect_activate(gtk_handler(data.clone(), tx));

	app.run(&[]);
}

