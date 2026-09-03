use std::{
	io,
	time::Duration,
};

use fb_config::vfs::ServiceSftp;
// use russh::keys::PrivateKeyWithHashAlg;  // COMMENTED OUT: depends on russh (447 crates)

#[derive(Clone, Copy)]
#[allow(dead_code)]  // fields used when russh SFTP is enabled
pub(super) struct Conn {
	pub(super) name: &'static str,
	pub(super) config: &'static ServiceSftp,
}

// COMMENTED OUT: entire russh SSH handler (depends on russh — 447 transitive crates)
// macro_rules! cfg_err {
// 	($($args:tt)*) => {
// 		russh::Error::InvalidConfig(format!($($args)*))
// 	};
// }

// impl russh::client::Handler for Conn {
// 	type Error = russh::Error;
//
// 	async fn check_server_key(
// 		&mut self,
// 		_server_public_key: &russh::keys::PublicKey,
// 	) -> Result<bool, Self::Error> {
// 		Ok(true)
// 	}
// }

impl deadpool::managed::Manager for Conn {
	type Error = io::Error;
	type Type = fb_sftp::Operator;

	async fn create(&self) -> Result<Self::Type, Self::Error> {
		// let channel = self.connect().await.map_err(|e| {
		// 	io::Error::other(format!("Failed to connect to SFTP server `{}`: {e}", self.name))
		// })?;
		//
		// let mut op = fb_sftp::Operator::make(channel.into_stream());
		// op.init().await?;
		// Ok(op)
		Err(io::Error::other("SFTP disabled: russh removed (447 crates)"))
	}

	async fn recycle(
		&self,
		obj: &mut Self::Type,
		_metrics: &deadpool::managed::Metrics,
	) -> deadpool::managed::RecycleResult<Self::Error> {
		if obj.is_closed() {
			Err(deadpool::managed::RecycleError::Message("Channel closed".into()))
		} else {
			Ok(())
		}
	}
}

impl Conn {
	pub(super) async fn roll(self) -> io::Result<deadpool::managed::Object<Self>> {
		use deadpool::managed::PoolError;

		let pool = *super::CONN.lock().entry(self.config).or_insert_with(|| {
			Box::leak(Box::new(
				deadpool::managed::Pool::builder(self)
					.runtime(deadpool::Runtime::Tokio1)
					.max_size(8)
					.create_timeout(Some(Duration::from_secs(45)))
					.build()
					.unwrap(),
			))
		});

		pool.get().await.map_err(|e| match e {
			PoolError::Timeout(_) => io::Error::new(io::ErrorKind::TimedOut, e.to_string()),
			PoolError::Backend(e) => e,
			PoolError::Closed | PoolError::NoRuntimeSpecified | PoolError::PostCreateHook(_) => {
				io::Error::other(e.to_string())
			}
		})
	}

	// COMMENTED OUT: all SSH connect methods depend on russh (447 crates)
	// async fn connect(self) -> Result<russh::Channel<russh::client::Msg>, russh::Error> { ... }
	// async fn connect_by_password(self, ...) -> Result<russh::client::Handle<Self>, russh::Error> { ... }
	// async fn connect_by_key(self, ...) -> Result<russh::client::Handle<Self>, russh::Error> { ... }
	// async fn connect_by_key_and_cert(self, ...) -> Result<russh::client::Handle<Self>, russh::Error> { ... }
	// async fn connect_by_agent(self, ...) -> Result<russh::client::Handle<Self>, russh::Error> { ... }
}
