use std::{env,
          ffi::OsString,
          fs as stdfs,
          path::PathBuf};

use crate::{common::ui::UI,
            hcore::{crypto::CACHE_KEY_PATH_ENV_VAR,
                    env as henv,
                    fs}};

use crate::{config,
            error::Result};

pub const ARTIFACT_PATH_ENVVAR: &str = "ARTIFACT_PATH";

const AUTH_TOKEN_ENVVAR: &str = "HAB_AUTH_TOKEN";
const BLDR_URL_ENVVAR: &str = "HAB_BLDR_URL";
const CTL_SECRET_ENVVAR: &str= "HAB_CTL_SECRET";
const ORIGIN_ENVVAR: &str = "HAB_ORIGIN";
const STUDIO_CMD: &str = "hab-studio";
const STUDIO_CMD_ENVVAR: &str = "HAB_STUDIO_BINARY";
const STUDIO_PACKAGE_IDENT: &str = "core/hab-studio";

pub fn start(ui: &mut UI, args: &[OsString]) -> Result<()> {
    if henv::var(AUTH_TOKEN_ENVVAR).is_err() {
        let config = config::load()?;
        if let Some(auth_token) = config.auth_token {
            debug!("Setting {}={} via config file", AUTH_TOKEN_ENVVAR, &auth_token);
            env::set_var("HAB_AUTH_TOKEN", auth_token);
        }
    }

    if henv::var(BLDR_URL_ENVVAR).is_err() {
        let config = config::load()?;
        if let Some(bldr_url) = config.bldr_url {
            debug!("Setting {}={} via config file", BLDR_URL_ENVVAR, &bldr_url);
            env::set_var("HAB_BLDR_URL", bldr_url);
        }
    }

    if henv::var(CTL_SECRET_ENVVAR).is_err() {
        let config = config::load()?;
        if let Some(ctl_secret) = config.ctl_secret {
            debug!("Setting {}={} via config file", CTL_SECRET_ENVVAR, &ctl_secret);
            env::set_var("CTL_SECRET_ENVVAR", ctl_secret);
        }
    }

    if henv::var(ORIGIN_ENVVAR).is_err() {
        let config = config::load()?;
        if let Some(default_origin) = config.origin {
            debug!("Setting default origin {} via CLI config", &default_origin);
            env::set_var("HAB_ORIGIN", default_origin);
        }
    }

    if henv::var(CACHE_KEY_PATH_ENV_VAR).is_err() {
        let path = fs::cache_key_path(None::<&str>);
        debug!("Setting {}={}", CACHE_KEY_PATH_ENV_VAR, path.display());
        env::set_var(CACHE_KEY_PATH_ENV_VAR, &path);
    };

    let artifact_path = match henv::var(ARTIFACT_PATH_ENVVAR) {
        Ok(p) => PathBuf::from(p),
        Err(_) => {
            let path = fs::cache_artifact_path(None::<&str>);
            debug!("Setting {}={}", ARTIFACT_PATH_ENVVAR, path.display());
            env::set_var(ARTIFACT_PATH_ENVVAR, &path);
            path
        }
    };
    if !artifact_path.is_dir() {
        debug!("Creating artifact_path at: {}", artifact_path.display());
        stdfs::create_dir_all(&artifact_path)?;
    }

    inner::start(ui, args)
}

#[cfg(target_os = "linux")]
mod inner {
    use crate::{command::studio::docker,
                common::ui::{UIWriter,
                             UI},
                error::{Error,
                        Result},
                exec,
                hcore::{crypto::init,
                        env as henv,
                        fs::{am_i_root,
                             find_command},
                        os::process,
                        package::{PackageIdent,
                                  PackageInstall},
                        users::linux as group},
                VERSION};
    use std::{env,
              ffi::OsString,
              path::PathBuf,
              str::FromStr};

    const SUDO_CMD: &str = "sudo";

    pub fn start(ui: &mut UI, args: &[OsString]) -> Result<()> {
        rerun_with_sudo_if_needed(ui, &args)?;
        if is_docker_studio(&args) {
            docker::start_docker_studio(ui, args)
        } else {
            let command = match henv::var(super::STUDIO_CMD_ENVVAR) {
                Ok(command) => PathBuf::from(command),
                Err(_) => {
                    init();
                    let version: Vec<&str> = VERSION.split('/').collect();
                    let ident = PackageIdent::from_str(&format!("{}/{}",
                                                                super::STUDIO_PACKAGE_IDENT,
                                                                version[0]))?;
                    // This is a duplicate of the code in `hab pkg exec` and
                    // should be refactored as part of or after:
                    // https://github.com/habitat-sh/habitat/issues/6633
                    // https://github.com/habitat-sh/habitat/issues/6634
                    let pkg_install = PackageInstall::load(&ident, None)?;
                    let cmd_env = pkg_install.environment_for_command()?;
                    for (key, value) in cmd_env.into_iter() {
                        debug!("Setting: {}='{}'", key, value);
                        env::set_var(key, value);
                    }

                    let mut display_args = super::STUDIO_CMD.to_string();
                    for arg in args {
                        display_args.push(' ');
                        display_args.push_str(arg.to_string_lossy().as_ref());
                    }
                    debug!("Running: {}", display_args);

                    exec::command_from_min_pkg(ui, super::STUDIO_CMD, &ident)?
                }
            };

            if let Some(cmd) = find_command(command.to_string_lossy().as_ref()) {
                process::become_command(cmd, args)?;
                Ok(())
            } else {
                Err(Error::ExecCommandNotFound(command))
            }
        }
    }

    fn is_docker_studio(args: &[OsString]) -> bool {
        if cfg!(not(target_os = "linux")) {
            return false;
        }

        for arg in args.iter() {
            let str_arg = arg.to_string_lossy();
            if str_arg == "-D" {
                return true;
            }
        }

        false
    }

    fn has_docker_group() -> bool {
        let current_user = group::get_current_username().unwrap();
        let docker_members = group::get_members_by_groupname("docker");
        docker_members.map_or(false, |d| d.contains(&current_user))
    }

    fn rerun_with_sudo_if_needed(ui: &mut UI, args: &[OsString]) -> Result<()> {
        // If I have root permissions or if I am executing a docker studio
        // and have the appropriate group - early return, we are done.
        if am_i_root() || (is_docker_studio(args) && has_docker_group()) {
            return Ok(());
        }

        // Otherwise we will try to re-run this program using `sudo`
        match find_command(SUDO_CMD) {
            Some(sudo_prog) => {
                let mut args: Vec<OsString> = vec!["-p".into(),
                                                   "[sudo hab-studio] password for %u: ".into(),
                                                   "-E".into(),];
                args.append(&mut env::args_os().collect());
                process::become_command(sudo_prog, &args)?;
                Ok(())
            }
            None => {
                ui.warn(format!("Could not find the `{}' command, is it in your PATH?",
                                SUDO_CMD))?;
                ui.warn("Running Habitat Studio requires root or administrator privileges. \
                         Please retry this command as a super user or use a privilege-granting \
                         facility such as sudo.")?;
                ui.br()?;
                Err(Error::RootRequired)
            }
        }
    }
}

#[cfg(not(target_os = "linux"))]
mod inner {
    use crate::{command::studio::docker,
                common::ui::UI,
                error::{Error,
                        Result},
                exec,
                hcore::{crypto::init,
                        env as henv,
                        fs::find_command,
                        os::process,
                        package::PackageIdent},
                VERSION};
    use std::{ffi::OsString,
              path::PathBuf,
              str::FromStr};

    pub fn start(_ui: &mut UI, args: &[OsString]) -> Result<()> {
        if is_windows_studio(&args) {
            start_windows_studio(_ui, args)
        } else {
            docker::start_docker_studio(_ui, args)
        }
    }

    pub fn start_windows_studio(ui: &mut UI, args: &[OsString]) -> Result<()> {
        let command = match henv::var(super::STUDIO_CMD_ENVVAR) {
            Ok(command) => PathBuf::from(command),
            Err(_) => {
                init();
                let version: Vec<&str> = VERSION.split('/').collect();
                let ident = PackageIdent::from_str(&format!("{}/{}",
                                                            super::STUDIO_PACKAGE_IDENT,
                                                            version[0]))?;
                exec::command_from_min_pkg(ui, super::STUDIO_CMD, &ident)?
            }
        };

        if let Some(cmd) = find_command(command.to_string_lossy().as_ref()) {
            process::become_command(cmd, args)?;
        } else {
            return Err(Error::ExecCommandNotFound(command));
        }
        Ok(())
    }

    fn is_windows_studio(args: &[OsString]) -> bool {
        if cfg!(not(target_os = "windows")) {
            return false;
        }

        for arg in args.iter() {
            let str_arg = arg.to_string_lossy();
            if str_arg == "-D" {
                return false;
            }
        }

        // -w/--windows is deprecated and should be removed in a post 0.64.0 release
        for arg in args.iter() {
            let str_arg = arg.to_string_lossy().to_lowercase();
            if str_arg == "--windows" || str_arg == "-w" {
                return true;
            }
        }

        true
    }
}
