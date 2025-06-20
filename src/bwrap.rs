use anyhow::Context;
use std::{
    ffi::OsStr,
    fs::File,
    io::{Seek, SeekFrom, Write},
    path::PathBuf,
    process::Command,
};
use tempdir::TempDir;

pub struct BwrapBuilder {
    command: Command,
    data: BwrapData,
}

impl BwrapBuilder {
    pub fn new() -> Self {
        Self {
            command: Command::new("bwrap"),
            data: BwrapData::default(),
        }
    }

    fn arg(&mut self, arg: impl AsRef<OsStr>) -> &mut Self {
        self.command.arg(arg);
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

    /*pub fn ro_bind_data(
        &mut self,
        path: impl AsRef<OsStr>,
        contents: &[u8],
    ) -> anyhow::Result<&mut Self> {
        let memfd = MemfdOptions::new()
            .allow_sealing(true)
            .close_on_exec(false)
            .create("memfd-data")
            .context("Could not create memfd")?;

        eprintln!(
            "creating file with contents {:?}",
            std::str::from_utf8(contents)
        );

        memfd
            .as_file()
            .write_all(contents)
            .context("Could not write to memfd")?;
        memfd
            .as_file()
            .seek(SeekFrom::Start(0))
            .context("Could not seek memfd")?;

        memfd.add_seals(&[
            FileSeal::SealShrink,
            FileSeal::SealGrow,
            FileSeal::SealWrite,
            FileSeal::SealSeal,
        ])?;

        let raw_fd = memfd.as_raw_fd();
        self.data.mem_fds.push(memfd);

        Ok(self.arg("--ro-bind-data").arg(raw_fd.to_string()).arg(path))
    }*/

    pub fn ro_bind_data(
        &mut self,
        path: impl AsRef<OsStr>,
        contents: &[u8],
    ) -> anyhow::Result<&mut Self> {
        let tempfile_path = self.tempfile(contents)?;
        Ok(self.arg("--ro-bind").arg(tempfile_path).arg(path))
    }

    fn tempfile(&mut self, contents: &[u8]) -> anyhow::Result<PathBuf> {
        let tempfile_path = self
            .data
            .tempdir
            .path()
            .join(format!("tempfile-{}", self.data.files.len()));
        let mut file = File::create(&tempfile_path).context("Could not create file")?;

        file.write_all(contents)
            .context("Could not write to file")?;
        file.seek(SeekFrom::Start(0))
            .context("Could not seek file")?;

        self.data.files.push(file);

        Ok(tempfile_path)
    }

    pub fn finish(self) -> (Command, BwrapData) {
        (self.command, self.data)
    }
}

#[derive(Debug)]
pub struct BwrapData {
    // mem_fds: Vec<Memfd>,
    tempdir: TempDir,
    files: Vec<File>,
}

impl Default for BwrapData {
    fn default() -> Self {
        Self {
            // mem_fds: Default::default(),
            tempdir: TempDir::new("flatbox-setup").expect("Could not create tempdir"),
            files: Default::default(),
        }
    }
}
