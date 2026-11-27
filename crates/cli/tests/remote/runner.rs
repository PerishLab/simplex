use crate::armor::{self, Record};
use crate::{late, memory, standalone};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

pub struct Workspace(tempfile::TempDir);

impl Workspace {
    pub fn open() -> Self {
        Self(tempfile::tempdir().expect("tempdir"))
    }

    pub fn join(&self, relative: &str) -> PathBuf {
        self.0.path().join(relative)
    }

    pub fn output(&self, name: &str) -> PathBuf {
        let folder = self.join("out");
        fs::create_dir_all(&folder).expect("mkdir out");
        folder.join(name)
    }

    pub fn asset(&self, relative: &str, bytes: &[u8]) {
        let target = self.join(relative);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).expect("mkdir");
        }
        fs::write(&target, bytes).expect("write asset");
    }

    pub fn stage(&self, relative: &str, source: &str) -> PathBuf {
        let target = self.join(relative);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).expect("mkdir");
        }
        fs::write(&target, source).expect("write source");
        let name = target.file_name().and_then(|item| item.to_str());
        if let Err(finding) = simplex::inspect(name, target.parent(), source) {
            panic!("invalid specimen {relative}: {finding}");
        }
        target
    }

    pub fn compile(&self, target: &str, entry: &Path, output: &Path) {
        self.invoke(target, entry, output, false);
    }

    pub fn shared(&self, target: &str, entry: &Path, output: &Path) {
        self.invoke(target, entry, output, true);
    }

    fn invoke(&self, target: &str, entry: &Path, output: &Path, dynamic: bool) {
        let mut command = Command::new(env!("CARGO_BIN_EXE_simplex"));
        command.current_dir(self.0.path());
        command.arg("build").arg(entry);
        command.arg("--target").arg(target);
        if dynamic {
            command.arg("--dylib");
        }
        command.arg("-o").arg(output);
        let result = command.output().expect("spawn simplex");
        let detail = String::from_utf8_lossy(&result.stderr);
        assert!(result.status.success(), "build failed: {detail}");
    }
}

struct Stream(Vec<String>);

impl Stream {
    fn new() -> Self {
        Self(Vec::new())
    }

    fn put(&mut self, name: &str, executable: bool, path: &Path) {
        let bytes = fs::read(path).expect("artifact bytes");
        let flag = if executable { "1" } else { "0" };
        let encoded = armor::base(&bytes);
        self.0.push(format!("PUT {name} {flag} {encoded}"));
    }

    fn run(&mut self, label: &str, binary: &str) {
        self.0.push(format!("RUN {label} {binary}"));
    }

    fn finish(self) -> String {
        self.0.join("\n") + "\n"
    }
}

pub fn linux() -> Vec<Record> {
    let workspace = Workspace::open();
    let mut cases = standalone::stage(&workspace);
    cases.extend(memory::stage(&workspace));
    cases.extend(late::stage(&workspace));
    let mut records = Vec::new();
    for item in &cases {
        let binary = workspace.output(item.label);
        workspace.compile("linux-x86_64", &item.entry, &binary);
        let actual = local(item.label, &binary, &workspace.join("out"));
        assert_eq!(actual, item.expected);
        records.push(actual);
    }
    records.extend(swap(&workspace));
    let mut reference = standalone::expected();
    reference.extend(memory::expected());
    reference.extend(late::expected());
    assert_eq!(records, reference);
    records
}

fn swap(workspace: &Workspace) -> Vec<Record> {
    let unit = workspace.stage("codec.sx", &late::library(10, 3));
    let entry = workspace.stage("main.sx", late::caller());
    let library = workspace.output("codec.so");
    let runner = workspace.output("runner");
    let folder = workspace.join("out");
    workspace.shared("linux-x86_64", &unit, &library);
    workspace.compile("linux-x86_64", &entry, &runner);
    let snapshot = fs::read(&runner).expect("runner bytes");
    let first = local("initial", &runner, &folder);
    workspace.stage("codec.sx", &late::library(30, 11));
    workspace.shared("linux-x86_64", &unit, &library);
    assert_eq!(fs::read(&runner).expect("runner unchanged"), snapshot);
    let second = local("swapped", &runner, &folder);
    vec![first, second]
}

fn local(label: &str, binary: &Path, directory: &Path) -> Record {
    let result = Command::new(binary)
        .current_dir(directory)
        .env("LD_LIBRARY_PATH", directory)
        .output()
        .expect("execute local");
    Record {
        label: label.to_owned(),
        code: result.status.code().expect("exit code"),
        output: result.stdout,
    }
}

pub fn prepare(target: &str, suffix: &str, library: &str) -> String {
    let workspace = Workspace::open();
    let mut cases = standalone::stage(&workspace);
    cases.extend(memory::stage(&workspace));
    cases.extend(late::stage(&workspace));
    let mut stream = Stream::new();
    for item in &cases {
        let name = format!("{}{suffix}", item.label);
        let binary = workspace.output(&name);
        workspace.compile(target, &item.entry, &binary);
        stream.put(&name, true, &binary);
        stream.run(item.label, &name);
    }
    let unit = workspace.stage("codec.sx", &late::library(10, 3));
    let entry = workspace.stage("main.sx", late::caller());
    let first = workspace.output("first.lib");
    let second = workspace.output("second.lib");
    let runner = format!("runner{suffix}");
    let host = workspace.output(&runner);
    workspace.shared(target, &unit, &first);
    workspace.compile(target, &entry, &host);
    let snapshot = fs::read(&host).expect("host bytes");
    workspace.stage("codec.sx", &late::library(30, 11));
    workspace.shared(target, &unit, &second);
    assert_eq!(fs::read(&host).expect("host unchanged"), snapshot);
    stream.put(library, false, &first);
    stream.put(&runner, true, &host);
    stream.run("initial", &runner);
    stream.put(library, false, &second);
    stream.run("swapped", &runner);
    stream.finish()
}

pub fn windows(stream: &str) -> Vec<Record> {
    let encoded = armor::wide(POWERSHELL);
    let command = format!("powershell -NoProfile -NonInteractive -EncodedCommand {encoded}");
    remote("windows.home.liberte.host", &command, stream)
}

pub fn apple(stream: &str) -> Vec<Record> {
    let encoded = armor::base(POSIX.as_bytes());
    let command = format!("eval \"$(printf %s {encoded} | base64 -d)\"");
    remote("macmini.home.liberte.host", &command, stream)
}

fn remote(host: &str, command: &str, stream: &str) -> Vec<Record> {
    let mut child = Command::new("runseal")
        .args([":", "ssh", host, command])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn runseal ssh");
    child
        .stdin
        .take()
        .expect("stdin")
        .write_all(stream.as_bytes())
        .expect("stream payload");
    let result = child.wait_with_output().expect("wait ssh");
    let detail = String::from_utf8_lossy(&result.stderr);
    assert!(result.status.success(), "remote {host} failed: {detail}");
    let text = String::from_utf8_lossy(&result.stdout);
    armor::parse(&text)
}

const POWERSHELL: &str = "\
$ProgressPreference = 'SilentlyContinue'
$ErrorActionPreference = 'Stop'
$dir = Join-Path $env:TEMP ('simplex-' + [Guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $dir | Out-Null
try {
  $reader = New-Object System.IO.StreamReader([Console]::OpenStandardInput(), [System.Text.Encoding]::ASCII)
  while ($null -ne ($line = $reader.ReadLine())) {
    if ($line.Length -eq 0) { continue }
    $parts = $line.Split(' ')
    if ($parts[0] -eq 'PUT') {
      $dest = Join-Path $dir $parts[1]
      if (Test-Path -LiteralPath $dest) { Remove-Item -LiteralPath $dest -Force }
      [System.IO.File]::WriteAllBytes($dest, [Convert]::FromBase64String($parts[3]))
    } elseif ($parts[0] -eq 'RUN') {
      $psi = New-Object System.Diagnostics.ProcessStartInfo
      $psi.FileName = (Join-Path $dir $parts[2])
      $psi.WorkingDirectory = $dir
      $psi.UseShellExecute = $false
      $psi.RedirectStandardOutput = $true
      $proc = [System.Diagnostics.Process]::Start($psi)
      $ms = New-Object System.IO.MemoryStream
      $proc.StandardOutput.BaseStream.CopyTo($ms)
      $proc.WaitForExit()
      $hex = [BitConverter]::ToString($ms.ToArray()).Replace('-', '').ToLowerInvariant()
      [Console]::Out.WriteLine(($parts[1] + ':' + $proc.ExitCode + ':' + $hex))
    }
  }
} finally {
  if (Test-Path -LiteralPath $dir) {
    Remove-Item -LiteralPath $dir -Recurse -Force -ErrorAction SilentlyContinue
  }
}
";

const POSIX: &str = "\
set -eu
dir=$(mktemp -d /tmp/simplex-XXXXXX)
trap 'rm -rf \"$dir\"' EXIT
while read -r op first second third; do
  case \"$op\" in
    PUT)
      rm -f \"$dir/$first\"
      printf '%s' \"$third\" | base64 -d > \"$dir/$first\"
      if [ \"$second\" = \"1\" ]; then chmod 755 \"$dir/$first\"; fi
      codesign -vvv \"$dir/$first\" >/dev/null 2>&1
      ;;
    RUN)
      set +e
      (cd \"$dir\" && DYLD_LIBRARY_PATH=\"$dir\" \"$dir/$second\" > \"$dir/out\")
      code=$?
      set -e
      hex=$(xxd -p \"$dir/out\" | tr -d '\\n ')
      printf '%s:%d:%s\\n' \"$first\" \"$code\" \"$hex\"
      ;;
  esac
done
";
