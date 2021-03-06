pub use anyhow::Result as AnyResult;

use std::{
	env,
	fs::{self, File},
	path::Path,
	process::{Command, Stdio},
};

use clap::{ArgEnum, Parser};

use subwasmlib::Subwasm;
use wasm_loader::Source;

macro_rules! match_runtimes {
	($self:ident, $a:expr, $b:expr) => {
		match $self {
			Runtime::Darwinia | Runtime::Crab => $a,
			Runtime::Pangoro | Runtime::Pangolin => $b,
		}
	};
}

#[derive(Clone, Debug, ArgEnum)]
pub enum Runtime {
	Darwinia,
	Crab,
	Pangoro,
	Pangolin,
}
impl Runtime {
	fn name(&self) -> String {
		format!("{:?}", self)
	}

	fn lowercase_name(&self) -> String {
		self.name().to_ascii_lowercase()
	}

	fn github(&self) -> String {
		format!("https://github.com/darwinia-network/{}", self.repository())
	}
	fn repository(&self) -> &str {
		match_runtimes!(self, "darwinia", "darwinia-common")
	}

	fn path(&self) -> String {
		format!(
			"{}/{}",
			match_runtimes!(self, "runtime", "node/runtime"),
			self.lowercase_name()
		)
	}
}

#[derive(Debug, Parser)]
struct Cli {
	/// Specific runtime (non case sensitive)
	#[clap(
		arg_enum,
		short,
		long,
		ignore_case = true,
		required = true,
		takes_value = true,
		value_name = "CHAIN"
	)]
	runtime: Runtime,
	/// Specific branch/commit/tag.
	#[clap(
		short,
		long,
		takes_value = true,
		value_name = "VALUE",
		default_value = "main"
	)]
	target: String,
}

fn main() -> AnyResult<()> {
	let Cli { runtime, target } = Cli::parse();
	let runtime_source_code_path = format!("build/{}", runtime.repository());

	// TODO: check if the folder is empty
	if !Path::new(&runtime_source_code_path).exists() {
		run(
			"git",
			&["clone", &runtime.github(), &runtime_source_code_path],
		)?;
	}

	env::set_current_dir(runtime_source_code_path)?;

	// TODO: switch to the workspace, use their toolchain configs
	let runtime_manifest = format!("{}/Cargo.toml", runtime.path());
	let runtime_lowercase_name = runtime.lowercase_name();

	run("git", &["fetch", "--all"])?;
	run("git", &["checkout", &target])?;
	run(
		"cargo",
		&[
			"clean",
			"--release",
			"--manifest-path",
			&runtime_manifest,
			"-p",
			&format!("{}-runtime", runtime_lowercase_name),
		],
	)?;
	run(
		"cargo",
		&[
			"b",
			"--release",
			"--manifest-path",
			&runtime_manifest,
			"--features",
			"evm-tracing",
		],
	)?;

	env::set_current_dir("../../")?;

	let name_prefix = format!("{}-{}-tracing-runtime", runtime_lowercase_name, target);
	let wasms_dir = format!("overridden-runtimes/{}/wasms", runtime_lowercase_name);
	let digests_dir = format!("overridden-runtimes/{}/digests", runtime_lowercase_name);

	create_dir_unchecked(&wasms_dir)?;
	create_dir_unchecked(&digests_dir)?;

	let wasm_path = format!("{}/{}.compact.compressed.wasm", wasms_dir, name_prefix);
	let digest_path = format!("{}/{}.json", digests_dir, name_prefix);

	fs::rename(
		format!(
			"build/{}/target/release/wbuild/{}-runtime/{}_runtime.compact.compressed.wasm",
			runtime.repository(),
			runtime_lowercase_name,
			runtime_lowercase_name,
		),
		&wasm_path,
	)?;

	let wasm = Subwasm::new(&Source::File(wasm_path.clone().into()));
	let runtime_info = File::create(&digest_path)?;

	serde_json::to_writer(runtime_info, wasm.runtime_info())?;

	println!("Generated WASM:   {}", wasm_path);
	println!("Generated digest: {}", digest_path);

	Ok(())
}

fn create_dir_unchecked(path: &str) -> AnyResult<()> {
	if !Path::new(path).exists() {
		fs::create_dir_all(path)?;
	}

	Ok(())
}

fn run(program: &str, args: &[&str]) -> AnyResult<()> {
	Command::new(program)
		.args(args)
		.stderr(Stdio::inherit())
		.output()?;

	Ok(())
}
