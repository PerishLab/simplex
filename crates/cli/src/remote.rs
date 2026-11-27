use clap::Args;
use simplex_package::{Credentials, Endpoint, Store};
use std::env;
use std::path::{Path, PathBuf};

#[derive(Args, Clone)]
pub struct Context {
    #[arg(long, env = "SIMPLEX_HOME")]
    pub home: Option<PathBuf>,
    #[arg(long, env = "SIMPLEX_S3_ENDPOINT")]
    pub endpoint: Option<String>,
    #[arg(long, env = "SIMPLEX_S3_BUCKET")]
    pub bucket: Option<String>,
    #[arg(long, env = "SIMPLEX_S3_REGION")]
    pub region: Option<String>,
}

impl Context {
    pub fn store(&self) -> Result<Store, String> {
        let cache = self.cache()?;
        let endpoint = self.remote()?;
        Ok(Store::new(cache, endpoint))
    }

    fn cache(&self) -> Result<PathBuf, String> {
        if let Some(home) = self
            .home
            .as_ref()
            .filter(|value| !value.as_os_str().is_empty())
        {
            return Ok(home.join("cache"));
        }
        let Some(base) = env::var_os("HOME").or_else(|| env::var_os("USERPROFILE")) else {
            return Err("SIMPLEX_HOME or HOME is required".into());
        };
        Ok(PathBuf::from(base).join(".simplex").join("cache"))
    }

    fn remote(&self) -> Result<Option<Endpoint>, String> {
        let url = present(self.endpoint.as_deref());
        let bucket = present(self.bucket.as_deref());
        match (url, bucket) {
            (None, None) => Ok(None),
            (Some(url), Some(bucket)) => {
                let region = present(self.region.as_deref()).unwrap_or_else(|| "auto".to_owned());
                let credentials = credentials()?;
                Ok(Some(Endpoint {
                    url,
                    bucket,
                    region,
                    credentials,
                }))
            }
            _ => Err("both --endpoint and --bucket are required".into()),
        }
    }
}

pub fn parent(path: &Path) -> PathBuf {
    path.parent()
        .filter(|part| !part.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."))
        .to_path_buf()
}

pub fn locate(path: &Path) -> PathBuf {
    if path.is_dir() {
        path.to_path_buf()
    } else {
        parent(path)
    }
}

fn credentials() -> Result<Option<Credentials>, String> {
    let identity = variable("SIMPLEX_S3_ACCESS_KEY");
    let secret = variable("SIMPLEX_S3_SECRET_KEY");
    let token = variable("SIMPLEX_S3_SESSION_TOKEN");
    match (identity, secret) {
        (None, None) => Ok(None),
        (Some(identity), Some(secret)) => Ok(Some(Credentials {
            identity,
            secret,
            token,
        })),
        _ => Err("incomplete S3 credentials".into()),
    }
}

fn present(value: Option<&str>) -> Option<String> {
    let trimmed = value?.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_owned())
    }
}

fn variable(key: &str) -> Option<String> {
    let raw = env::var(key).ok()?;
    present(Some(&raw))
}
