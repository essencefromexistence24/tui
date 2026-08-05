use std::{any::TypeId, collections::HashMap, sync::Arc, time::Duration};

use parking_lot::Mutex;
// use russh::{ChannelStream, client::Msg};  // COMMENTED OUT: depends on russh (447 crates)
use serde::Serialize;
use tokio::sync::{mpsc, oneshot};

use crate::{Error, Id, Packet, Receiver, responses};

pub struct Session {
	tx: mpsc::UnboundedSender<Vec<u8>>,
	id: Id,
	pub(super) callback: Mutex<HashMap<u32, oneshot::Sender<Packet<'static>>>>,
	pub(super) extensions: Mutex<HashMap<String, String>>,
}

impl Drop for Session {
	fn drop(&mut self) {
		self.tx.send(vec![]).ok();
	}
}

impl Session {
	// COMMENTED OUT: depends on russh ChannelStream<Msg> (447 crates)
	// pub(super) fn make(stream: ChannelStream<Msg>) -> Arc<Self> {
	// 	...
	// }

	pub async fn send<'a, I, O>(self: &Arc<Self>, input: I) -> Result<O, Error>
	where
		I: Into<Packet<'a>> + Serialize,
		O: TryFrom<Packet<'static>, Error = Error> + 'static,
	{
		self.send_with_timeout(input, Duration::from_secs(45)).await
	}

	pub fn send_sync<'a, I>(self: &Arc<Self>, input: I) -> Result<Receiver, Error>
	where
		I: Into<Packet<'a>> + Serialize,
	{
		let mut request: Packet = input.into();
		if request.id() == 0 {
			request = request.with_id(self.id.next());
		}

		let id = request.id();
		let (tx, rx) = oneshot::channel();

		self.callback.lock().insert(id, tx);
		self.tx.send(crate::to_bytes(request)?)?;
		Ok(Receiver::new(self, id, rx))
	}

	pub async fn send_with_timeout<'a, I, O>(
		self: &Arc<Self>,
		input: I,
		timeout: Duration,
	) -> Result<O, Error>
	where
		I: Into<Packet<'a>> + Serialize,
		O: TryFrom<Packet<'static>, Error = Error> + 'static,
	{
		match tokio::time::timeout(timeout, self.send_sync(input)?).await?? {
			Packet::Status(status) if TypeId::of::<O>() != TypeId::of::<responses::Status>() => {
				Err(Error::Status(status))
			}
			response => response.try_into(),
		}
	}

	pub fn is_closed(self: &Arc<Self>) -> bool {
		self.tx.is_closed()
	}
}
