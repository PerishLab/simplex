use crate::server::Server;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Clone, Debug)]
pub struct Outcome {
    pub passed: bool,
    pub code: i32,
    pub stdout: Vec<u8>,
    pub stderr: String,
}

#[derive(Clone, Debug)]
pub struct Client {
    pub endpoint: String,
    pub bucket: String,
    pub region: String,
    pub signed: bool,
}

impl Client {
    pub fn signed(server: &Server, region: &str) -> Self {
        Self {
            endpoint: server.endpoint().to_owned(),
            bucket: server.bucket().to_owned(),
            region: region.to_owned(),
            signed: true,
        }
    }

    pub fn unsigned(server: &Server, region: &str) -> Self {
        Self {
            endpoint: server.endpoint().to_owned(),
            bucket: server.bucket().to_owned(),
            region: region.to_owned(),
            signed: false,
        }
    }
}

pub struct Sandbox {
    handle: tempfile::TempDir,
    home: PathBuf,
    generation: usize,
}

impl Sandbox {
    pub fn open() -> Self {
        let handle = tempfile::tempdir().expect("tempdir");
        let home = handle.path().join("home-0");
        fs::create_dir_all(&home).expect("mkdir home");
        Self {
            handle,
            home,
            generation: 0,
        }
    }

    pub fn home(&self) -> &Path {
        &self.home
    }

    pub fn fresh(&mut self) {
        self.generation += 1;
        self.home = self.handle.path().join(format!("home-{}", self.generation));
        fs::create_dir_all(&self.home).expect("mkdir fresh home");
    }

    pub fn join(&self, relative: &str) -> PathBuf {
        self.handle.path().join(relative)
    }

    pub fn write(&self, relative: &str, content: &str) -> PathBuf {
        let target = self.join(relative);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).expect("mkdir parent");
        }
        fs::write(&target, content).expect("write text");
        target
    }

    pub fn asset(&self, relative: &str, bytes: &[u8]) -> PathBuf {
        let target = self.join(relative);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).expect("mkdir parent");
        }
        fs::write(&target, bytes).expect("write asset");
        target
    }

    pub fn run(&self, arguments: &[&str], client: Option<&Client>) -> Outcome {
        let mut command = self.base();
        if let Some(remote) = client {
            command.env("SIMPLEX_S3_ENDPOINT", &remote.endpoint);
            command.env("SIMPLEX_S3_BUCKET", &remote.bucket);
            command.env("SIMPLEX_S3_REGION", &remote.region);
            if remote.signed {
                command.env("SIMPLEX_S3_ACCESS_KEY", "simplexkey");
                command.env("SIMPLEX_S3_SECRET_KEY", "simplexsecret");
            }
        }
        command.args(arguments);
        collect(command)
    }

    pub fn custom(&self, arguments: &[&str], variables: &[(&str, &str)]) -> Outcome {
        let mut command = self.base();
        for (name, value) in variables {
            command.env(name, value);
        }
        command.args(arguments);
        collect(command)
    }

    pub fn execute(&self, binary: &Path, directory: &Path) -> Outcome {
        let mut command = Command::new(binary);
        command.current_dir(directory);
        command.env("LD_LIBRARY_PATH", directory);
        collect(command)
    }

    fn base(&self) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_simplex"));
        command.current_dir(self.handle.path());
        for variable in [
            "SIMPLEX_S3_ENDPOINT",
            "SIMPLEX_S3_BUCKET",
            "SIMPLEX_S3_REGION",
            "SIMPLEX_S3_ACCESS_KEY",
            "SIMPLEX_S3_SECRET_KEY",
            "SIMPLEX_S3_SESSION_TOKEN",
        ] {
            command.env_remove(variable);
        }
        command.env("SIMPLEX_HOME", &self.home);
        command
    }
}

fn collect(mut command: Command) -> Outcome {
    let output = command.output().expect("spawn process");
    Outcome {
        passed: output.status.success(),
        code: output.status.code().unwrap_or(-1),
        stdout: output.stdout,
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    }
}
