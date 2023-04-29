use std::sync::Arc;

use tokio::fs::File;
use tokio::sync::{Mutex, OwnedMutexGuard};

use crate::handlers::connection_handler::AsyncReadWrite;
use crate::handlers::data_channel_wrapper::DataChannelWrapper;
use crate::io::error::Error;
use crate::io::reply::Reply;
use crate::io::reply_code::ReplyCode;

pub(crate) async fn get_data_channel_lock(
  data_wrapper: Arc<Mutex<dyn DataChannelWrapper>>,
) -> Result<OwnedMutexGuard<Option<Box<dyn AsyncReadWrite>>>, Reply> {
  let error_reply = Reply::new(
    ReplyCode::BadSequenceOfCommands,
    "Data channel must be open first!",
  );
  match data_wrapper.try_lock() {
    Ok(dcw) => {
      let dc = dcw.get_data_stream().await.lock_owned().await;
      if dc.is_some() {
        Ok(dc)
      } else {
        Err(error_reply)
      }
    }
    Err(e) => {
      eprintln!("Data channel is not available! {e}");
      Err(error_reply)
    }
  }
}

pub(crate) fn get_transfer_reply(success: bool) -> Reply {
  if success {
    Reply::new(ReplyCode::ClosingDataConnection, "Transfer complete!")
  } else {
    Reply::new(
      ReplyCode::ConnectionClosedTransferAborted,
      "Error occurred during transfer!",
    )
  }
}

pub(crate) fn get_open_file_result(file: Result<File, Error>) -> Result<File, Reply> {
  match file {
    Ok(f) => Ok(f),
    Err(Error::UserError) => Err(Reply::new(
      ReplyCode::NotLoggedIn,
      Error::UserError.to_string(),
    )),
    Err(Error::NotFoundError(m)) | Err(Error::InvalidPathError(m)) => {
      Err(Reply::new(ReplyCode::FileUnavailable, m))
    }
    Err(Error::PermissionError) => Err(Reply::new(
      ReplyCode::FileUnavailable,
      Error::PermissionError.to_string(),
    )),
    Err(Error::NotAFileError) => Err(Reply::new(
      ReplyCode::SyntaxErrorInParametersOrArguments,
      Error::NotAFileError.to_string(),
    )),
    Err(Error::OsError(e)) => Err(Reply::new(
      ReplyCode::RequestedActionAborted,
      "Requested action aborted: local error in processing.",
    )),
    Err(_) => unreachable!(),
  }
}
