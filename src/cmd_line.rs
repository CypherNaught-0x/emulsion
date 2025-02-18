use crate::Version;
use clap::{parser::ValueSource, value_parser, Arg, Command};
use std::path::Path;

pub struct Args {
	pub file_path: Option<String>,
	pub displayed_folders: Option<u32>,
}

/// Parses the command-line arguments and returns the file path
pub fn parse_args(config_path: &Path, cache_path: &Path) -> Args {
	// It's okay to leak this, because this code should only be executed once.
	let config: &'static str = Box::leak(
		format!(
			"CONFIGURATION:\n    config file: {}\n    cache file:  {}",
			config_path.to_string_lossy(),
			cache_path.to_string_lossy(),
		)
		.into_boxed_str(),
	);
	let version: &'static str =
		Box::leak(Version::cargo_pkg_version().to_string().into_boxed_str());

	let matches = Command::new("emulsion")
		.version(version)
		.author("Christoph Hess <admin@ctrl-consulting.com>")
		.about(
			"A fast and minimalistic image viewer\n\
			https://github.com/ChristophHess/emulsion",
		)
		.after_help(config)
		.arg(
			Arg::new("FOLDER_COUNT")
				.long("folders")
				.short('f')
				.help("Number of folders to display in the filepath")
				.num_args(1)
				.value_parser(value_parser!(u32)),
		)
		.arg(
			Arg::new("absolute")
				.long("absolute")
				.short('a')
				.help("Display all folders in the filepath, all the way to the root")
				.num_args(0)
				.conflicts_with("FOLDER_COUNT"),
		)
		.arg(Arg::new("PATH").help("The file path of the image").index(1))
		.get_matches();

	let file_path = matches.get_one::<String>("PATH").cloned();

	let is_absolute = matches.value_source("absolute") == Some(ValueSource::CommandLine);
	let displayed_folders = if is_absolute {
		// Subtract one because we later want to add one to this value, and we don't want
		// an overflow
		Some(std::u32::MAX - 1)
	} else {
		matches.get_one::<u32>("FOLDER_COUNT").copied()
	};

	Args { file_path, displayed_folders }
}
