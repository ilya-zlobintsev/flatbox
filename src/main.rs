mod args;
mod bwrap;
mod keyfile;

use crate::keyfile::parse_keyfile;
use anyhow::{Context, anyhow, bail};
use args::{Args, RunCommand};
use bwrap::BwrapBuilder;
use clap::Parser;
use indexmap::IndexMap;
use std::{
    collections::HashSet,
    env, fs,
    path::{Path, PathBuf},
    process::{ExitCode, Stdio},
};

const DEFAULT_INSTALL_PATH: &str = "/var/lib/flatpak";
const ROOT_USR_MERGED_DIRS: [&str; 5] = ["bin", "lib", "lib32", "lib64", "sbin"];
const FORBIDDEN_HOST_ROOT_DIRS: [&str; 5] = ["app", "usr", "run", "etc", "var"];
const FORBIDDEN_RUN_DIRS: [&str; 2] = ["flatpak", "host"];
const EXPOSED_ETC_PATHS: [&str; 3] = ["passwd", "group", "shadow"];
const EXTENSION_PREFIX: &str = "Extension ";
const PATH_BINDINDGS: [(&str, &str, bool); 6] = [
    ("/", "/run/host/root", true),
    ("/usr/share/fonts", "/run/host/fonts", false),
    ("/usr/lib/fontconfig/cache", "/run/host/fonts-cache", false),
    ("/usr/share/icons", "/run/host/share/icons", false),
    ("/etc/machine-id", "/etc/machine-id", false),
    (
        "/var/lib/dbus/machine-id",
        "/var/lib/dbus/machine-id",
        false,
    ),
];
const DEFAULT_ENV: [(&str, Option<&str>); 44] = [
    ("FLATBOX_ENV", Some("1")),
    ("PATH", Some("/app/bin:/usr/bin")),
    ("LD_LIBRARY_PATH", None),
    ("LD_PRELOAD", None),
    ("LD_AUDIT", None),
    ("XDG_CONFIG_DIRS", Some("/app/etc/xdg:/etc/xdg")),
    ("XDG_DATA_DIRS", Some("/app/share:/usr/share")),
    ("SHELL", Some("/bin/sh")),
    ("TEMP", None),
    ("TEMPDIR", None),
    ("TMP", None),
    ("TMPDIR", None),
    ("container", None),
    ("TZDIR", None),
    ("PYTHONPATH", None),
    ("PYTHONPYCACHEPREFIX", None),
    ("PERLLIB", None),
    ("PERL5LIB", None),
    ("XCURSOR_PATH", None),
    ("GST_PLUGIN_PATH_1_0", None),
    ("GST_REGISTRY", None),
    ("GST_REGISTRY_1_0", None),
    ("GST_PLUGIN_PATH", None),
    ("GST_PLUGIN_SYSTEM_PATH", None),
    ("GST_PLUGIN_SCANNER", None),
    ("GST_PLUGIN_SCANNER_1_0", None),
    ("GST_PLUGIN_SYSTEM_PATH_1_0", None),
    ("GST_PRESET_PATH", None),
    ("GST_PTP_HELPER", None),
    ("GST_PTP_HELPER_1_0", None),
    ("GST_INSTALL_PLUGINS_HELPER", None),
    ("KRB5CCNAME", None),
    ("XKB_CONFIG_ROOT", None),
    ("GIO_EXTRA_MODULES", None),
    ("GDK_BACKEND", None),
    ("VK_ADD_DRIVER_FILES", None),
    ("VK_ADD_LAYER_PATH", None),
    ("VK_DRIVER_FILES", None),
    ("VK_ICD_FILENAMES", None),
    ("VK_LAYER_PATH", None),
    ("__EGL_EXTERNAL_PLATFORM_CONFIG_DIRS", None),
    ("__EGL_EXTERNAL_PLATFORM_CONFIG_FILENAMES", None),
    ("__EGL_VENDOR_LIBRARY_DIRS", None),
    ("__EGL_VENDOR_LIBRARY_FILENAMES", None),
];

fn main() -> anyhow::Result<ExitCode> {
    let args = Args::try_parse()?;

    match args.command {
        args::Command::Run(cmd) => run(cmd, args.verbose),
    }
}

fn run(run: RunCommand, verbose: bool) -> anyhow::Result<ExitCode> {
    let user_install_dir = env::var("HOME")
        .ok()
        .map(|home| {
            Path::new(&home)
                .join(".local")
                .join("share")
                .join("flatpak")
        })
        .filter(|path| path.exists());

    let install_dirs: Vec<PathBuf> = [PathBuf::from(DEFAULT_INSTALL_PATH)]
        .into_iter()
        .chain(user_install_dir)
        .chain(run.flatpak_install_path)
        .collect();

    let available_runtimes =
        list_available_runtimes(&install_dirs).context("Could not list runtimes")?;

    let (runtime, app_files_path) = match (&run.app, run.runtime) {
        (Some(app), None) => {
            let app_path = find_install_path(app, true, &install_dirs)
                .context("Could not find app install dir")?
                .join("current")
                .join("active");
            let app_metadata_path = app_path.join("metadata");

            let raw_app_metadata =
                fs::read_to_string(app_metadata_path).context("Could not read app metadata")?;
            let app_metadata =
                parse_keyfile(&raw_app_metadata).context("Could not parse app metadata")?;

            let app_runtime = app_metadata
                .get("Application")
                .and_then(|app| app.get("runtime"))
                .context("Could not read app runtime")?;

            let app_files_path = app_path.join("files");

            (app_runtime.to_string(), Some(app_files_path))
        }
        (None, Some(runtime)) => (runtime, None),
        (Some(_), Some(_)) => bail!("Only app or runtime flags can be used at once"),
        (None, None) => bail!("Either app or runtime has to be specified"),
    };

    let runtime_path = find_install_path(&runtime, false, &install_dirs)
        .context("Could not find runtime install dir")?
        .join("active");
    let runtime_metadata_path = runtime_path.join("metadata");

    let raw_runtime_metadata =
        fs::read_to_string(runtime_metadata_path).context("Could not read runtime metadata")?;
    let runtime_metadata =
        parse_keyfile(&raw_runtime_metadata).context("Could not parse runtime metadata")?;

    let runtime_env = runtime_metadata
        .get("Environment")
        .cloned()
        .unwrap_or_default();

    let runtime_files_path = runtime_path.join("files");

    let mut bwrap = BwrapBuilder::new();

    setup_runtime(&mut bwrap, &runtime_files_path, app_files_path.as_deref())?;

    setup_host_root_dirs(&mut bwrap)?;

    setup_runtime_extensions(
        &mut bwrap,
        &runtime_metadata,
        &available_runtimes,
        &install_dirs,
    )?;

    add_ld_so_conf(&mut bwrap)?;

    setup_env(&mut bwrap, runtime_env, run.app.as_deref());

    // bwrap.bind_data("/etc/ld.so.cache", &[])?;

    let (mut cmd, _data) = bwrap.finish();
    if verbose {
        eprintln!("Generated cmd: {cmd:#?}");
    }

    // let ldconfig_status = Command::new(cmd.get_program())
    //     .args(cmd.get_args())
    //     .arg("ldconfig")
    //     .arg("-X")
    //     .output()?;
    // eprintln!(
    //     "ldconfig status: {} {}{}",
    //     ldconfig_status.status,
    //     String::from_utf8_lossy(&ldconfig_status.stdout),
    //     String::from_utf8_lossy(&ldconfig_status.stderr)
    // );

    let mut child = cmd
        .arg("sh")
        .arg("-c")
        .arg(format!(
            "ldconfig && {} {}",
            run.command,
            run.args.join(" ")
        ))
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .spawn()?;

    let out = child.wait()?;

    Ok(out
        .code()
        .and_then(|code| u8::try_from(code).ok())
        .map(ExitCode::from)
        .unwrap_or(ExitCode::SUCCESS))
}

fn setup_runtime(
    bwrap: &mut BwrapBuilder,
    runtime_files_path: &Path,
    app_files_path: Option<&Path>,
) -> anyhow::Result<()> {
    bwrap.ro_bind(runtime_files_path, "/usr");

    if let Some(app_path) = app_files_path {
        bwrap.ro_bind(app_path, "/app");
    }

    let runtime_etc = fs::read_dir(runtime_files_path.join("etc"))?;

    for entry in runtime_etc {
        let entry = entry?;
        let path = entry.path();

        let target_path = path
            .strip_prefix(runtime_files_path)
            .expect("Could not strip etc path prefix");

        if let Ok(symlink_target) = fs::read_link(&path) {
            bwrap.symlink(&symlink_target, target_path);
        } else {
            bwrap.ro_bind(&path, target_path);
        }
    }

    for dir in ROOT_USR_MERGED_DIRS {
        if runtime_files_path.join(dir).exists() {
            bwrap.symlink(Path::new("/usr").join(dir), dir);
        }
    }

    bwrap.ro_bind_data("/.flatpak-info", &[])?;

    Ok(())
}

fn setup_runtime_extensions(
    bwrap: &mut BwrapBuilder,
    runtime_metadata: &IndexMap<&str, IndexMap<&str, &str>>,
    available_runtimes: &[String],
    install_dirs: &[PathBuf],
) -> anyhow::Result<()> {
    let runtime = runtime_metadata
        .get("Runtime")
        .and_then(|runtime| runtime.get("runtime"))
        .context("Missing runtime spec")?;
    let mut runtime_split = runtime.split('/').skip(1);
    let arch = runtime_split
        .next()
        .context("Could not extract architecture from runtime id")?;
    let version = runtime_split
        .next()
        .context("Could not extract version from runtime id")?;

    for (group, metadata) in runtime_metadata {
        if let Some(extension) = group.strip_prefix(EXTENSION_PREFIX) {
            setup_extension(
                metadata,
                bwrap,
                extension,
                arch,
                version,
                available_runtimes,
                install_dirs,
            )
            .with_context(|| format!("Could not set up extension {extension}"))?;
        }
    }

    Ok(())
}

fn setup_extension(
    extension_metadata: &IndexMap<&str, &str>,
    bwrap: &mut BwrapBuilder,
    name: &str,
    arch: &str,
    runtime_version: &str,
    available_runtimes: &[String],
    install_dirs: &[PathBuf],
) -> anyhow::Result<()> {
    let directory = extension_metadata
        .get("directory")
        .context("Missing directory")?;

    let expected_prefix = format!("{name}.");
    let extension_base_mount_path = Path::new("/usr").join(directory);

    let allowed_versions = extension_metadata
        .get("versions")
        .or_else(|| extension_metadata.get("version"))
        .map(|version| version.to_string())
        .unwrap_or_else(|| runtime_version.to_owned());

    bwrap.tmpfs(&extension_base_mount_path);

    let mut mounted_paths = Vec::new();

    for extension in available_runtimes {
        let Some(extension_impl_name) = extension.strip_prefix(&expected_prefix) else {
            continue;
        };

        let enabled: bool = match extension_metadata.get("enable-if").copied() {
            Some("active-gl-driver") => match extension_impl_name {
                "default" | "host" => true,
                _ => {
                    if let Some(nvidia_version) = extension_impl_name.strip_prefix("nvidia-") {
                        fs::read_to_string("/sys/module/nvidia/version")
                            .map(|version| version.trim().replace('.', "-"))
                            .is_ok_and(|allowed_version| allowed_version == nvidia_version)
                    } else {
                        false
                    }
                }
            },
            Some(enable_if) => {
                eprintln!("Unsupported enable-if reason '{enable_if}' on extension '{name}'");
                false
            }
            None => true,
        };

        if !enabled {
            continue;
        }

        let extension_base_path = find_install_path(extension, false, install_dirs)
            .context("Could not find extension install dir")?
            .join(arch);
        for version in allowed_versions.split(';') {
            let extension_version_path = extension_base_path
                .join(version)
                .join("active")
                .join("files");
            if extension_version_path.exists() {
                let extension_mount_path = extension_base_mount_path.join(extension_impl_name);
                bwrap.ro_bind(&extension_version_path, &extension_mount_path);
                mounted_paths.push((extension_version_path, extension_mount_path));
            }
        }
    }

    let mut existing_symlinks = HashSet::new();
    for (source, target) in &mounted_paths {
        if let Some(merge_dirs) = extension_metadata.get("merge-dirs") {
            let mut processed_paths = HashSet::new();
            for merge_dir in merge_dirs.split(';') {
                if let Ok(entries) = fs::read_dir(source.join(merge_dir)) {
                    for entry in entries {
                        let entry = entry?;

                        if !entry.file_type()?.is_file() {
                            continue;
                        }

                        if processed_paths.insert(entry.path()) {
                            let symlink_source = target.join(merge_dir).join(entry.file_name());
                            let symlink_target = extension_base_mount_path
                                .join(merge_dir)
                                .join(entry.file_name());
                            if existing_symlinks.insert(symlink_target.clone()) {
                                bwrap.symlink(symlink_source, symlink_target);
                            }
                        }
                    }
                }
            }
        }

        if let Some(add_ld_path) = extension_metadata.get("add-ld-path") {
            let ld_path = target.join(add_ld_path);
            let mut ld_contents = ld_path
                .to_str()
                .context("Invalid ld path formed")?
                .to_owned();
            ld_contents.push('\n');

            let impl_name = target
                .file_name()
                .context("Invalid extension name")?
                .to_str()
                .context("Invalid extension name")?;
            let filename = format!("runtime-{name}.{impl_name}.conf");
            let ld_config_path = Path::new("/run/flatpak/ld.so.conf.d").join(filename);

            bwrap.ro_bind_data(&ld_config_path, ld_contents.as_bytes())?;
        }
    }

    Ok(())
}

fn add_ld_so_conf(bwrap: &mut BwrapBuilder) -> anyhow::Result<()> {
    let contents = "\
include /run/flatpak/ld.so.conf.d/app-*.conf
include /app/etc/ld.so.conf
/app/lib
include /run/flatpak/ld.so.conf.d/runtime-*.conf
";

    bwrap.ro_bind_data("/etc/ld.so.conf", contents.as_bytes())?;
    Ok(())
}

fn setup_host_root_dirs(bwrap: &mut BwrapBuilder) -> anyhow::Result<()> {
    let root_dirs = fs::read_dir("/").context("Could not read root dir")?;
    for entry in root_dirs {
        let entry = entry.context("Could not evaluate root dir")?;
        if let Some(filename) = entry.file_name().to_str() {
            let entry_path = entry.path();
            if FORBIDDEN_HOST_ROOT_DIRS.contains(&filename)
                || ROOT_USR_MERGED_DIRS.contains(&filename)
            {
                continue;
            }

            bwrap.bind(&entry_path, &entry_path);
        }
    }

    let run_dirs = fs::read_dir("/run").context("Could not read root dir")?;
    for entry in run_dirs {
        let entry = entry.context("Could not evaluate run dir")?;
        if let Some(filename) = entry.file_name().to_str() {
            let entry_path = entry.path();
            if FORBIDDEN_RUN_DIRS.contains(&filename) {
                continue;
            }

            bwrap.bind(&entry_path, &entry_path);
        }
    }

    for name in EXPOSED_ETC_PATHS {
        let path = Path::new("/etc").join(name);
        if path.exists() {
            bwrap.ro_bind(&path, &path);
        }
    }

    for (source, target, writable) in PATH_BINDINDGS {
        if Path::new(source).exists() {
            if writable {
                bwrap.bind(source, target);
            } else {
                bwrap.ro_bind(source, target);
            }
        }
    }

    bwrap.dev_bind("/dev", "/dev");
    bwrap.symlink("/run", "/var/run");

    Ok(())
}

fn setup_env(bwrap: &mut BwrapBuilder, runtime_env: IndexMap<&str, &str>, app_id: Option<&str>) {
    for (env, value) in DEFAULT_ENV {
        match value {
            Some(value) => bwrap.set_env(env, value),
            None => bwrap.unset_env(env),
        };
    }

    for (env, value) in runtime_env {
        bwrap.set_env(env, value);
    }

    if let Some(app) = app_id {
        if let Ok(home) = env::var("HOME") {
            let app_id_dir = Path::new(&home).join(".var").join("app").join(app);
            bwrap.set_env("XDG_DATA_HOME", app_id_dir.join("data"));
            bwrap.set_env("XDG_CONFIG_HOME", app_id_dir.join("config"));
            bwrap.set_env("XDG_CACHE_HOME", app_id_dir.join("cache"));
            bwrap.set_env("XDG_STATE_HOME", app_id_dir.join(".local").join("state"));
        }
    }
}

fn list_available_runtimes(install_dirs: &[PathBuf]) -> anyhow::Result<Vec<String>> {
    let mut output = Vec::new();

    for dir in install_dirs {
        let dir_runtimes = fs::read_dir(dir.join("runtime"))
            .into_iter()
            .flatten()
            .map(|entry| {
                entry.context("Could not read entry").and_then(|entry| {
                    entry
                        .file_name()
                        .into_string()
                        .map_err(|_| anyhow!("Invalid runtime name"))
                })
            })
            .collect::<anyhow::Result<Vec<String>>>()?;

        output.extend(dir_runtimes);
    }

    Ok(output)
}

fn find_install_path(name: &str, is_app: bool, install_dirs: &[PathBuf]) -> Option<PathBuf> {
    let infix = if is_app { "app" } else { "runtime" };
    for dir in install_dirs {
        let path = Path::new(dir).join(infix).join(name);
        if path.exists() {
            return Some(path);
        }
    }
    None
}
