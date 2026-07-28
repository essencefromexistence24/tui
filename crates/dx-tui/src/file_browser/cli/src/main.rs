#![allow(unsafe_code)]
#![allow(clippy::all)]
#![allow(clippy::pedantic)]
#![allow(clippy::nursery)]
#![allow(clippy::cargo)]

fb_macro::mod_pub!(package shared);

fb_macro::mod_flat!(args);

use std::process::ExitCode;

use clap::Parser;
use fb_macro::{errln, outln};
use fb_shared::LOCAL_SET;

#[tokio::main]
async fn main() -> ExitCode {
	fb_shared::init();
	fb_fs::init();

	match LOCAL_SET.run_until(run()).await {
		Ok(()) => ExitCode::SUCCESS,
		Err(e) => {
			for cause in e.chain() {
				if let Some(ioerr) = cause.downcast_ref::<std::io::Error>()
					&& ioerr.kind() == std::io::ErrorKind::BrokenPipe
				{
					return ExitCode::from(0);
				}
			}
			errln!("{e:#}").ok();
			ExitCode::FAILURE
		}
	}
}

async fn run() -> anyhow::Result<()> {
	if std::env::args_os().nth(1).is_some_and(|s| s == "-V" || s == "--version") {
		outln!(
			"Ya {} ({} {})",
			env!("CARGO_PKG_VERSION"),
			env!("VERGEN_GIT_SHA"),
			env!("VERGEN_BUILD_DATE")
		)?;
		return Ok(());
	}

	match Args::parse().command {
		Command::Emit(cmd) => {
			fb_boot::init_default();
			fb_dds::init();
			if let Err(e) = fb_dds::Client::shot("dds-emit", CommandPub::receiver()?, &cmd.body()?).await
			{
				errln!("Cannot emit command: {e}")?;
				std::process::exit(1);
			}
		}

		Command::EmitTo(cmd) => {
			fb_boot::init_default();
			fb_dds::init();
			if let Err(e) = fb_dds::Client::shot("dds-emit", cmd.receiver, &cmd.body()?).await {
				errln!("Cannot emit command: {e}")?;
				std::process::exit(1);
			}
		}

		Command::Pkg(cmd) => {
			package::init()?;

			let mut pkg = package::Package::load().await?;
			match cmd {
				CommandPkg::Add { ids } => pkg.add_many(&ids).await?,
				CommandPkg::Delete { ids } => pkg.delete_many(&ids).await?,
				CommandPkg::Install => pkg.install().await?,
				CommandPkg::List => pkg.print()?,
				CommandPkg::Upgrade { ids } => pkg.upgrade_many(&ids).await?,
			}
		}

		Command::Pub(cmd) => {
			fb_boot::init_default();
			fb_dds::init();
			if let Err(e) = fb_dds::Client::shot(&cmd.kind, CommandPub::receiver()?, &cmd.body()?).await {
				errln!("Cannot send message: {e}")?;
				std::process::exit(1);
			}
		}

		Command::PubTo(cmd) => {
			fb_boot::init_default();
			fb_dds::init();
			if let Err(e) = fb_dds::Client::shot(&cmd.kind, cmd.receiver, &cmd.body()?).await {
				errln!("Cannot send message: {e}")?;
				std::process::exit(1);
			}
		}

		Command::Sub(cmd) => {
			fb_boot::init_default();
			fb_dds::init();
			fb_dds::Client::draw(cmd.kinds.split(',').collect()).await?;

			tokio::signal::ctrl_c().await?;
		}
	}

	Ok(())
}
