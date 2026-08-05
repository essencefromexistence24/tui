fb_macro::mod_flat!(absolute conn gate metadata read_dir sftp);

static CONN: fb_shared::RoCell<
	parking_lot::Mutex<
		hashbrown::HashMap<
			&'static fb_config::vfs::ServiceSftp,
			&'static deadpool::managed::Pool<Conn>,
		>,
	>,
> = fb_shared::RoCell::new();

pub(super) fn init() { CONN.init(Default::default()); }

