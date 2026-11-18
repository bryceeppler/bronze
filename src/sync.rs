mod address;
mod code;
mod engine;
mod file;
mod key;
mod remote;
mod seal;
mod status;
#[cfg(test)]
pub mod test_server;
#[cfg(test)]
mod tests;

use std::io;
use std::path::Path;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;

pub use code::{Setup, SyncCode};
pub use engine::Delivery;
pub use status::Status;

use engine::{Engine, Event};
use file::SyncFile;

use crate::board::Replica;

pub struct Syncing {
    file: Option<SyncFile>,
    running: Option<Running>,
}

struct Running {
    code: SyncCode,
    events: Sender<Event>,
    deliveries: Receiver<Delivery>,
    status: Status,
}

impl Syncing {
    pub fn unsaved() -> Syncing {
        Syncing { file: None, running: None }
    }

    pub fn saved_in(folder: &Path) -> Syncing {
        Syncing { file: Some(SyncFile::in_folder(folder)), running: None }
    }

    pub fn resume(&mut self, local: &Replica, wake: impl Fn() + Send + 'static) {
        let Some(file) = &self.file else { return };
        match file.read() {
            Ok(Some(code)) => self.running = Some(Running::start(code, local, wake)),
            Ok(None) => {}
            Err(why) => eprintln!("bronze: {} is unreadable ({why}); sync stays off", file.path().display()),
        }
    }

    pub fn start(&mut self, code: SyncCode, local: &Replica, wake: impl Fn() + Send + 'static) -> io::Result<()> {
        let file = self.file.as_ref().ok_or_else(|| io::Error::other("notes stay in memory, so sync has no folder"))?;
        file.write(&code)?;
        self.running = Some(Running::start(code, local, wake));
        Ok(())
    }

    pub fn stop(&mut self) -> io::Result<()> {
        if let Some(file) = &self.file {
            file.remove()?;
        }
        self.running = None;
        Ok(())
    }

    pub fn status(&self) -> Status {
        self.running.as_ref().map_or(Status::Off, |running| running.status.clone())
    }

    pub fn code(&self) -> Option<&SyncCode> {
        self.running.as_ref().map(|running| &running.code)
    }

    pub fn edited(&self, local: &Replica) {
        if let Some(running) = &self.running {
            _ = running.events.send(Event::Edited(local.clone()));
        }
    }

    pub fn arrived(&mut self) -> Vec<Delivery> {
        let Some(running) = &mut self.running else { return Vec::new() };
        let arrived: Vec<Delivery> = running.deliveries.try_iter().collect();
        for delivery in &arrived {
            if let Delivery::Status(status) = delivery {
                running.status = status.clone();
            }
        }
        arrived
    }
}

impl Running {
    fn start(code: SyncCode, local: &Replica, wake: impl Fn() + Send + 'static) -> Running {
        let (events, inbox) = mpsc::channel();
        let (deliver, deliveries) = mpsc::channel();
        let engine = Engine::new(&code, local.clone(), events.clone(), move |delivery| {
            if deliver.send(delivery).is_ok() {
                wake();
            }
        });
        thread::Builder::new()
            .name("bronze-sync".into())
            .spawn(move || engine.run(&inbox))
            .expect("spawn the sync thread");
        Running { code, events, deliveries, status: Status::Connecting }
    }
}

impl Drop for Running {
    fn drop(&mut self) {
        _ = self.events.send(Event::Stop);
    }
}
