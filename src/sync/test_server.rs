use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::OnceLock;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use super::address::ServerAddress;

pub struct TestServer {
    child: Option<Child>,
    data: tempfile::TempDir,
    listen: String,
    max_vaults: u32,
    pub address: ServerAddress,
}

impl TestServer {
    pub fn start() -> TestServer {
        TestServer::holding(16)
    }

    pub fn holding(max_vaults: u32) -> TestServer {
        let data = tempfile::tempdir().expect("a temporary data folder");
        let (child, listen) = spawn(data.path(), "127.0.0.1:0", max_vaults);
        let address = ServerAddress::parse(&format!("http://{listen}")).expect("a local address");
        TestServer { child: Some(child), data, listen, max_vaults, address }
    }

    pub fn stop(&mut self) {
        if let Some(mut child) = self.child.take() {
            _ = child.kill();
            _ = child.wait();
        }
    }

    pub fn resume(&mut self) {
        let (child, _) = spawn(self.data.path(), &self.listen, self.max_vaults);
        self.child = Some(child);
    }

    pub fn wipe(&mut self) {
        self.stop();
        std::fs::remove_dir_all(self.data.path()).expect("wipe the data folder");
        self.resume();
    }
}

impl Drop for TestServer {
    fn drop(&mut self) {
        self.stop();
    }
}

fn spawn(data: &Path, listen: &str, max_vaults: u32) -> (Child, String) {
    let mut child = Command::new(binary())
        .env_clear()
        .env("BRONZE_SYNC_LISTEN", listen)
        .env("BRONZE_SYNC_DATA", data)
        .env("BRONZE_SYNC_MAX_VAULTS", max_vaults.to_string())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .expect("start bronze-sync");
    let stderr = BufReader::new(child.stderr.take().expect("piped stderr"));
    let (heard, listening) = mpsc::channel();
    thread::spawn(move || {
        for line in stderr.lines().map_while(Result::ok) {
            eprintln!("{line}");
            if let Some(rest) = line.strip_prefix("bronze-sync listening on ") {
                _ = heard.send(rest.split(',').next().unwrap_or_default().to_owned());
            }
        }
    });
    let listen = listening.recv_timeout(Duration::from_secs(10)).expect("bronze-sync says where it listens");
    (child, listen)
}

fn binary() -> &'static Path {
    static BUILT: OnceLock<PathBuf> = OnceLock::new();
    BUILT.get_or_init(|| {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let target = root.join("server/target");
        let built = Command::new(env!("CARGO"))
            .args(["build", "--quiet", "--locked", "--manifest-path"])
            .arg(root.join("server/Cargo.toml"))
            .arg("--target-dir")
            .arg(&target)
            .status()
            .expect("run cargo build for bronze-sync");
        assert!(built.success(), "bronze-sync did not build");
        target.join("debug").join(format!("bronze-sync{}", std::env::consts::EXE_SUFFIX))
    })
}
