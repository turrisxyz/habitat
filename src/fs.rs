// Copyright:: Copyright (c) 2015-2016 The Habitat Maintainers
//
// The terms of the Evaluation Agreement (Habitat) between Chef Software Inc.
// and the party accessing this file ("Licensee") apply to Licensee's use of
// the Software until such time that the Software is made available under an
// open source license such as the Apache 2.0 License.

use std::env;
use std::path::PathBuf;

pub const ROOT_PATH: &'static str = "/hab";
/// The default download root path for package artifacts, used on package installation
pub const CACHE_ARTIFACT_PATH: &'static str = "/hab/cache/artifacts";
/// The default path where cryptographic keys are stored
pub const CACHE_KEY_PATH: &'static str = "/hab/cache/keys";
/// The default path where source artifacts are downloaded, extracted, & compiled
pub const CACHE_SRC_PATH: &'static str = "/hab/cache/src";
/// The root path containing all locally installed packages
pub const PKG_PATH: &'static str = "/hab/pkgs";
/// The root path containing all runtime service directories and files
const SVC_PATH: &'static str = "/hab/svc";

/// Returns the root path for a given service's configuration, files, and data.
pub fn svc_path(service_name: &str) -> PathBuf {
    PathBuf::from(SVC_PATH).join(service_name)
}

/// Returns the path to a given service's configuration.
pub fn svc_config_path(service_name: &str) -> PathBuf {
    svc_path(service_name).join("config")
}

/// Returns the path to a given service's data.
pub fn svc_data_path(service_name: &str) -> PathBuf {
    svc_path(service_name).join("data")
}

/// Returns the path to a given service's gossiped config files.
pub fn svc_files_path(service_name: &str) -> PathBuf {
    svc_path(service_name).join("files")
}

/// Returns the path to a given service's hooks.
///
/// Note that this path is internal to the Supervisor and should not be directly accessed under
/// normal circumstances.
pub fn svc_hooks_path(service_name: &str) -> PathBuf {
    svc_path(service_name).join("hooks")
}

/// Returns the path to a given service's static content.
pub fn svc_static_path(service_name: &str) -> PathBuf {
    svc_path(service_name).join("static")
}

/// Returns the path to a given service's variable state.
pub fn svc_var_path(service_name: &str) -> PathBuf {
    svc_path(service_name).join("var")
}

/// Returns the absolute path for a given command, if it exists, by searching the `PATH`
/// environment variable.
///
/// If the command represents an absolute path, then the `PATH` seaching will not be performed. If
/// no absolute path can be found for the command, then `None` is returned.
pub fn find_command(command: &str) -> Option<PathBuf> {
    // If the command path is absolute and a file exists, then use that.
    let candidate = PathBuf::from(command);
    if candidate.is_absolute() && candidate.is_file() {
        return Some(candidate);
    }

    // Find the command by checking each entry in `PATH`. If we still can't find it, give up and
    // return `None`.
    match env::var_os("PATH") {
        Some(paths) => {
            for path in env::split_paths(&paths) {
                let candidate = PathBuf::from(&path).join(command);
                if candidate.is_file() {
                    return Some(candidate);
                }
            }
            None
        }
        None => None,
    }
}
