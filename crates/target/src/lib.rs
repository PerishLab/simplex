pub mod allocate;
pub mod apple;
pub mod arm;
pub mod intel;
pub mod linux;
pub mod machine;
pub mod windows;

use simplex_ir::Module;
use simplex_object::Artifact;
use std::fmt;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Platform {
    Linux,
    Windows,
    Apple,
}

impl Platform {
    pub const fn host() -> Self {
        #[cfg(target_os = "windows")]
        {
            Self::Windows
        }
        #[cfg(target_os = "macos")]
        {
            Self::Apple
        }
        #[cfg(not(any(target_os = "windows", target_os = "macos")))]
        {
            Self::Linux
        }
    }

    pub fn parse(name: &str) -> Option<Self> {
        match name {
            "linux" | "linux-x86_64" => Some(Self::Linux),
            "windows" | "windows-x86_64" => Some(Self::Windows),
            "apple" | "macos" | "darwin-aarch64" => Some(Self::Apple),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Fault {
    Unsupported(String),
}

impl fmt::Display for Fault {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unsupported(detail) => write!(out, "unsupported target construct: {detail}"),
        }
    }
}

impl std::error::Error for Fault {}

pub fn compile(platform: Platform, name: &str, module: &Module) -> Result<Artifact, Fault> {
    match platform {
        Platform::Linux => linux::compile(name, module),
        Platform::Windows => windows::compile(name, module),
        Platform::Apple => apple::compile(name, module),
    }
}
