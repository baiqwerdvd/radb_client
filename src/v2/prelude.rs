use simple_cmd::CommandBuilder;

use crate::v2::types::{Adb, Client, ConnectionType};

pub(crate) trait CommandBuilderExt {
	fn client<C>(self, client: C) -> Self
	where
		C: Into<Client>;

	fn addr<C>(self, addr: C) -> Self
	where
		C: Into<ConnectionType>;

	fn shell<C>(self, client: C) -> Self
	where
		C: Into<Client>;

	fn adb(adb: &Adb) -> CommandBuilder;
}

impl CommandBuilderExt for CommandBuilder {
	fn client<C>(self, client: C) -> Self
	where
		C: Into<Client>,
	{
		self.args(client.into().addr)
	}

	fn addr<C>(self, addr: C) -> Self
	where
		C: Into<ConnectionType>,
	{
		self.args(addr.into())
	}

	fn shell<C>(self, client: C) -> Self
	where
		C: Into<Client>,
	{
		self.client(client).arg("shell")
	}

	fn adb(adb: &Adb) -> CommandBuilder {
		CommandBuilder::new(adb)
	}
}
