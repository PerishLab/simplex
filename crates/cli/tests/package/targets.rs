use crate::consumer;
use crate::fixture;
use crate::harness::{Client, Sandbox};
use crate::server::Server;
use std::env;
use std::fs;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

fn prepare() -> Sandbox {
    let server = Server::start("cross", "auto", false);
    let client = Client::signed(&server, "auto");
    let mut sandbox = Sandbox::open();
    consumer::upstream(&sandbox, &client);
    sandbox.fresh();
    consumer::consumer(&sandbox, "canary");
    for (target, output) in [
        ("windows-x86_64", "win/app.exe"),
        ("darwin-aarch64", "mac/app"),
    ] {
        let path = sandbox.join(output);
        let built = sandbox.run(
            &[
                "build",
                "app",
                "--target",
                target,
                "-o",
                path.to_str().expect("path"),
            ],
            Some(&client),
        );
        assert!(
            built.passed,
            "{target} cross build failed: {}",
            built.stderr
        );
    }
    sandbox
}

#[test]
fn portable() {
    let sandbox = prepare();
    let stem = fixture::stem("math", "1.0.0", "canary");
    let symbol = format!("sx${stem}$metric$combine");
    let windows = format!("{stem}.dll");
    assert_eq!(
        fixture::listing(&sandbox.join("win")),
        fixture::sorted(vec![windows.clone(), String::from("app.exe")])
    );
    pe(&sandbox.join("win/app.exe"), false, windows.as_bytes());
    pe(
        &sandbox.join(&format!("win/{windows}")),
        true,
        symbol.as_bytes(),
    );

    let apple = format!("{stem}.dylib");
    assert_eq!(
        fixture::listing(&sandbox.join("mac")),
        fixture::sorted(vec![apple.clone(), String::from("app")])
    );
    let install = format!("@executable_path/{apple}\0");
    mach(&sandbox.join("mac/app"), false, install.as_bytes());
    let dylib = sandbox.join(&format!("mac/{apple}"));
    mach(&dylib, true, install.as_bytes());
    mach(&dylib, true, format!("_{symbol}").as_bytes());
}

fn pe(path: &Path, shared: bool, needle: &[u8]) {
    let bytes = fs::read(path).expect("read pe");
    assert_eq!(&bytes[0..2], b"MZ");
    let header = u32::from_le_bytes(bytes[0x3c..0x40].try_into().expect("pe offset")) as usize;
    assert_eq!(&bytes[header..header + 4], b"PE\0\0");
    assert_eq!(
        u16::from_le_bytes([bytes[header + 4], bytes[header + 5]]),
        0x8664
    );
    let flags = u16::from_le_bytes([bytes[header + 22], bytes[header + 23]]);
    assert_eq!((flags & 0x2000) != 0, shared);
    assert!(contains(&bytes, needle));
}

fn mach(path: &Path, shared: bool, needle: &[u8]) {
    let bytes = fs::read(path).expect("read mach");
    assert_eq!(
        u32::from_le_bytes(bytes[0..4].try_into().expect("magic")),
        0xfeed_facf
    );
    assert_eq!(
        u32::from_le_bytes(bytes[4..8].try_into().expect("cpu")),
        0x0100_000c
    );
    let expected = if shared { 6 } else { 2 };
    assert_eq!(
        u32::from_le_bytes(bytes[12..16].try_into().expect("kind")),
        expected
    );
    assert!(contains(&bytes, &0xfade_0cc0_u32.to_be_bytes()));
    assert!(contains(&bytes, needle));
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack.windows(needle.len()).any(|part| part == needle)
}

fn enabled(target: &str) -> bool {
    let Some(raw) = env::var_os("SIMPLEX_REMOTE") else {
        return false;
    };
    match raw.to_string_lossy().trim() {
        "" | "0" => false,
        "windows" => target == "windows",
        "apple" => target == "apple",
        _ => true,
    }
}

#[test]
fn windows() {
    if !enabled("windows") {
        return;
    }
    let sandbox = prepare();
    let library = format!("{}.dll", fixture::stem("math", "1.0.0", "canary"));
    let executable = base(&fs::read(sandbox.join("win/app.exe")).expect("exe"));
    let dynamic = base(&fs::read(sandbox.join(&format!("win/{library}"))).expect("dll"));
    let stream =
        format!("PUT {library} 0 {dynamic}\nPUT app.exe 1 {executable}\nRUN package app.exe\n");
    let command = format!(
        "powershell -NoProfile -NonInteractive -EncodedCommand {}",
        wide(POWERSHELL)
    );
    let output = remote("windows.home.liberte.host", &command, &stream);
    assert_eq!(output.trim(), "package:105:4f4b");
}

#[test]
fn apple() {
    if !enabled("apple") {
        return;
    }
    let sandbox = prepare();
    let library = format!("{}.dylib", fixture::stem("math", "1.0.0", "canary"));
    let executable = base(&fs::read(sandbox.join("mac/app")).expect("mach app"));
    let dynamic = base(&fs::read(sandbox.join(&format!("mac/{library}"))).expect("dylib"));
    let stream = format!("PUT {library} 0 {dynamic}\nPUT app 1 {executable}\nRUN package app\n");
    let command = format!(
        "eval \"$(printf %s {} | base64 -d)\"",
        base(POSIX.as_bytes())
    );
    let output = remote("macmini.home.liberte.host", &command, &stream);
    assert_eq!(output.trim(), "package:105:4f4b");
}

fn remote(host: &str, command: &str, stream: &str) -> String {
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
        .expect("write stream");
    let result = child.wait_with_output().expect("wait ssh");
    let detail = String::from_utf8_lossy(&result.stderr);
    assert!(result.status.success(), "remote {host} failed: {detail}");
    String::from_utf8_lossy(&result.stdout).into_owned()
}

fn base(bytes: &[u8]) -> String {
    let mut text = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let first = chunk[0];
        let second = chunk.get(1).copied().unwrap_or(0);
        let third = chunk.get(2).copied().unwrap_or(0);
        let triple = (u32::from(first) << 16) | (u32::from(second) << 8) | u32::from(third);
        text.push(char::from(TABLE[((triple >> 18) & 63) as usize]));
        text.push(char::from(TABLE[((triple >> 12) & 63) as usize]));
        text.push(pad(chunk.len() >= 2, (triple >> 6) & 63));
        text.push(pad(chunk.len() >= 3, triple & 63));
    }
    text
}

fn pad(present: bool, index: u32) -> char {
    if present {
        char::from(TABLE[index as usize])
    } else {
        '='
    }
}

fn wide(script: &str) -> String {
    let mut bytes = Vec::with_capacity(script.len() * 2);
    for unit in script.encode_utf16() {
        bytes.extend_from_slice(&unit.to_le_bytes());
    }
    base(&bytes)
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
