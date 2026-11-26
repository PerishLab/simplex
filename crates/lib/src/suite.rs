use crate::code::{Code, Finding};
use crate::inspect;
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::{fmt, fs};

const DOMAINS: [&str; 9] = [
    "async", "borrow", "comptime", "control", "dylib", "lexical", "meta", "model", "module",
];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Report {
    pub domains: usize,
    pub passed: usize,
    pub rejected: usize,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Failure {
    Io(PathBuf),
    Manifest(PathBuf),
    Domains {
        expected: Vec<String>,
        actual: Vec<String>,
    },
    Domain {
        path: PathBuf,
        declared: String,
    },
    Duplicate {
        domain: String,
        file: String,
    },
    Missing {
        domain: String,
        file: String,
    },
    Unlisted {
        domain: String,
        file: String,
    },
    Unexpected {
        domain: String,
        file: String,
        finding: Finding,
    },
    Accepted {
        domain: String,
        file: String,
        expected: Code,
    },
    Mismatch {
        domain: String,
        file: String,
        expected: Code,
        actual: Code,
    },
}

impl fmt::Display for Failure {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(path) => write!(out, "unreadable suite path: {}", path.display()),
            Self::Manifest(path) => write!(out, "invalid suite manifest: {}", path.display()),
            Self::Domains { expected, actual } => {
                write!(out, "suite domains {actual:?} differ from {expected:?}")
            }
            Self::Domain { path, declared } => write!(
                out,
                "manifest {} declares mismatched domain {declared}",
                path.display()
            ),
            Self::Duplicate { domain, file } => {
                write!(out, "{domain}: duplicate reject entry {file}")
            }
            Self::Missing { domain, file } => {
                write!(out, "{domain}: missing reject specimen {file}")
            }
            Self::Unlisted { domain, file } => {
                write!(out, "{domain}: unlisted reject specimen {file}")
            }
            Self::Unexpected {
                domain,
                file,
                finding,
            } => write!(out, "{domain}/pass/{file}: unexpected {finding}"),
            Self::Accepted {
                domain,
                file,
                expected,
            } => write!(
                out,
                "{domain}/reject/{file}: expected {expected}, but passed"
            ),
            Self::Mismatch {
                domain,
                file,
                expected,
                actual,
            } => write!(
                out,
                "{domain}/reject/{file}: expected {expected}, got {actual}"
            ),
        }
    }
}

impl std::error::Error for Failure {}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    domain: String,
    #[serde(default)]
    reject: Vec<Rule>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Rule {
    file: String,
    code: Code,
}

struct Suite<'a> {
    dir: PathBuf,
    name: &'a str,
}

impl<'a> Suite<'a> {
    fn new(root: &Path, name: &'a str) -> Self {
        Self {
            dir: root.join(name),
            name,
        }
    }

    fn check(&self) -> Result<(usize, usize), Failure> {
        let target = self.dir.join("suite.toml");
        let raw = fs::read_to_string(&target).map_err(|_| Failure::Io(target.clone()))?;
        let manifest: Manifest =
            toml::from_str(&raw).map_err(|_| Failure::Manifest(target.clone()))?;
        if manifest.domain != self.name {
            return Err(Failure::Domain {
                path: target,
                declared: manifest.domain,
            });
        }
        let passed = self.passes()?;
        let rejected = self.rejects(&manifest.reject)?;
        Ok((passed, rejected))
    }

    fn passes(&self) -> Result<usize, Failure> {
        let sub = self.dir.join("pass");
        let files = specimens(&sub)?;
        for file in &files {
            let path = sub.join(file);
            let source = fs::read_to_string(&path).map_err(|_| Failure::Io(path))?;
            if let Err(finding) = inspect(Some(file), Some(&sub), &source) {
                return Err(Failure::Unexpected {
                    domain: self.name.into(),
                    file: file.clone(),
                    finding,
                });
            }
        }
        Ok(files.len())
    }

    fn rejects(&self, rules: &[Rule]) -> Result<usize, Failure> {
        let sub = self.dir.join("reject");
        let disk = specimens(&sub)?;
        let declared = self.catalog(rules, &disk)?;
        for file in &disk {
            let Some(expected) = declared.get(file).copied() else {
                return Err(Failure::Unlisted {
                    domain: self.name.into(),
                    file: file.clone(),
                });
            };
            let path = sub.join(file);
            let source = fs::read_to_string(&path).map_err(|_| Failure::Io(path))?;
            self.judge(file, &sub, expected, &source)?;
        }
        Ok(disk.len())
    }

    fn catalog(
        &self,
        rules: &[Rule],
        disk: &BTreeSet<String>,
    ) -> Result<BTreeMap<String, Code>, Failure> {
        let mut declared = BTreeMap::new();
        for rule in rules {
            if !rule.file.ends_with(".sx") || rule.file.contains(['/', '\\']) {
                return Err(Failure::Manifest(self.dir.join("suite.toml")));
            }
            if declared.insert(rule.file.clone(), rule.code).is_some() {
                return Err(Failure::Duplicate {
                    domain: self.name.into(),
                    file: rule.file.clone(),
                });
            }
            if !disk.contains(&rule.file) {
                return Err(Failure::Missing {
                    domain: self.name.into(),
                    file: rule.file.clone(),
                });
            }
        }
        Ok(declared)
    }

    fn judge(&self, file: &str, sub: &Path, expected: Code, source: &str) -> Result<(), Failure> {
        match inspect(Some(file), Some(sub), source) {
            Ok(_) => Err(Failure::Accepted {
                domain: self.name.into(),
                file: file.into(),
                expected,
            }),
            Err(finding) if finding.code != expected => Err(Failure::Mismatch {
                domain: self.name.into(),
                file: file.into(),
                expected,
                actual: finding.code,
            }),
            Err(_) => Ok(()),
        }
    }
}

pub fn verify(root: &Path) -> Result<Report, Failure> {
    let found = directories(root)?;
    let expected: Vec<String> = DOMAINS.iter().map(|name| (*name).to_owned()).collect();
    if found != expected {
        return Err(Failure::Domains {
            expected,
            actual: found,
        });
    }
    let mut report = Report {
        domains: 0,
        passed: 0,
        rejected: 0,
    };
    for domain in DOMAINS {
        let (passed, rejected) = Suite::new(root, domain).check()?;
        report.domains += 1;
        report.passed += passed;
        report.rejected += rejected;
    }
    Ok(report)
}

fn directories(root: &Path) -> Result<Vec<String>, Failure> {
    let entries = fs::read_dir(root).map_err(|_| Failure::Io(root.to_path_buf()))?;
    let mut names = BTreeSet::new();
    for entry in entries {
        let entry = entry.map_err(|_| Failure::Io(root.to_path_buf()))?;
        let kind = entry.file_type().map_err(|_| Failure::Io(entry.path()))?;
        if !kind.is_dir() {
            return Err(Failure::Io(entry.path()));
        }
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| Failure::Io(entry.path()))?;
        names.insert(name);
    }
    Ok(names.into_iter().collect())
}

fn specimens(dir: &Path) -> Result<BTreeSet<String>, Failure> {
    if !dir.exists() {
        return Ok(BTreeSet::new());
    }
    let entries = fs::read_dir(dir).map_err(|_| Failure::Io(dir.to_path_buf()))?;
    let mut files = BTreeSet::new();
    for entry in entries {
        let entry = entry.map_err(|_| Failure::Io(dir.to_path_buf()))?;
        let kind = entry.file_type().map_err(|_| Failure::Io(entry.path()))?;
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| Failure::Io(entry.path()))?;
        if !kind.is_file() || !name.ends_with(".sx") {
            return Err(Failure::Io(entry.path()));
        }
        files.insert(name);
    }
    Ok(files)
}
