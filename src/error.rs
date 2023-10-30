use thiserror::Error;
use tokio::sync::mpsc::error::SendError;

use crate::Message;

#[derive(Error, Debug)]
pub enum Error<M> {
    #[error(transparent)]
    IO(#[from] std::io::Error),
    #[error(transparent)]
    Send(#[from] SendError<Message<M>>),
}
