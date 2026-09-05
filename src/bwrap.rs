use anyhow::Context;
use memfd::{Memfd, MemfdOptions};
use std::{
    ffi::{OsStr, OsString},
    io::{Seek, SeekFrom, Write},
    iter,
    os::{fd::AsRawFd, unix::ffi::OsStrExt},
    process::Command,
};

#[derive(Debug)]
pub struct BwrapBuilder {
    command: Command,
    args: OsString,
    data: BwrapData,
}

impl BwrapBuilder {
    pub fn new() -> Self {
        Self {
            command: Command::new("bwrap"),
            data: BwrapData::default(),
            args: OsString::new(),
        }
    }

    fn arg(&mut self, arg: impl AsRef<OsStr>) -> &mut Self {
        if !self.args.is_empty() {
            self.args.push("\0");
        }
        self.args.push(arg);
        self
    }

    pub fn tmpfs(&mut self, path: impl AsRef<OsStr>) -> &mut Self {
        self.arg("--tmpfs").arg(path)
    }

    pub fn bind(&mut self, source: impl AsRef<OsStr>, dest: impl AsRef<OsStr>) -> &mut Self {
        self.arg("--bind").arg(source).arg(dest)
    }

    pub fn ro_bind(&mut self, source: impl AsRef<OsStr>, dest: impl AsRef<OsStr>) -> &mut Self {
        self.arg("--ro-bind").arg(source).arg(dest)
    }

    pub fn symlink(&mut self, source: impl AsRef<OsStr>, dest: impl AsRef<OsStr>) -> &mut Self {
        self.arg("--symlink").arg(source).arg(dest)
    }

    pub fn set_env(&mut self, key: impl AsRef<OsStr>, value: impl AsRef<OsStr>) -> &mut Self {
        self.arg("--setenv").arg(key).arg(value)
    }

    pub fn unset_env(&mut self, key: impl AsRef<OsStr>) -> &mut Self {
        self.arg("--unsetenv").arg(key)
    }

    pub fn dev_bind(&mut self, source: impl AsRef<OsStr>, dest: impl AsRef<OsStr>) -> &mut Self {
        self.arg("--dev-bind").arg(source).arg(dest)
    }

    pub fn dir(&mut self, path: impl AsRef<OsStr>) -> &mut Self {
        self.arg("--dir").arg(path)
    }

    pub fn ro_bind_data(
        &mut self,
        path: impl AsRef<OsStr>,
        contents: &[u8],
    ) -> anyhow::Result<&mut Self> {
        let tempfile_fd = self.tempfile(contents)?;
        Ok(self
            .arg("--ro-bind-data")
            .arg(tempfile_fd.to_string())
            .arg(path))
    }

    fn tempfile(&mut self, contents: &[u8]) -> anyhow::Result<i32> {
        let mut file = MemfdOptions::new()
            .allow_sealing(true)
            .close_on_exec(false)
            .create("memfd-data")
            .context("Could not create memfd")?
            .into_file();

        file.write_all(contents)
            .context("Could not write to file")?;
        file.seek(SeekFrom::Start(0))
            .context("Could not seek file")?;

        let memfd = Memfd::try_from_file(file).expect("File is transferrable back to memfd");
        let raw_fd = memfd.as_raw_fd();

        self.data.mem_fds.push(memfd);

        Ok(raw_fd)
    }

    pub fn wrap_apparmor_unconfined(mut self) -> Self {
        let args = ["-p", "unconfined"]
            .into_iter()
            .map(OsStr::new)
            .chain(iter::once(self.command.get_program()))
            .chain(self.command.get_args());

        let mut new_cmd = Command::new("aa-exec");
        new_cmd.args(args);

        self.command = new_cmd;

        self
    }

    pub fn finish(mut self) -> anyhow::Result<(Command, BwrapData)> {
        let args = self.args.as_bytes().to_vec();
        let args_fd = self.tempfile(&args)?;

        self.command
            .arg("--args")
            .arg(args_fd.to_string())
            .arg("--");

        Ok((self.command, self.data))
    }
}

#[derive(Debug, Default)]
pub struct BwrapData {
    mem_fds: Vec<Memfd>,
}
