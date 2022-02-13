#[macro_use]
extern crate log;

mod client;
mod app;
mod canvas;

use crate::client::ClientHandle;
use crate::app::start_app;

use tokio::sync::oneshot;
use tungstenite::Error;

#[tokio::main]
async fn main() {
    env_logger::init();

    let (exit, exit_listener) = oneshot::channel::<Result<(), Error>>();

    let handle = ClientHandle::new(exit_listener);

    match start_app(handle) {
        _ => () // Todo
    };

    match exit.send(Ok(())) {
        Ok(_) => (),
        _ => ()
    }
}
