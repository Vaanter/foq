use std::sync::Arc;

use tokio::fs::File;
use tokio::sync::{Mutex, OwnedMutexGuard};
use tracing::{debug, info};

use crate::handlers::connection_handler::AsyncReadWrite;
use crate::data_channels::data_channel_wrapper::DataChannelWrapper;
use crate::io::error::IoError;
use crate::commands::reply::Reply;
use crate::commands::reply_code::ReplyCode;

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
      info!("Data channel is not available! {e}");
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

pub(crate) fn get_open_file_result(file: Result<File, IoError>) -> Result<File, Reply> {
  debug!("Checking file open result.");
  match file {
    Ok(f) => Ok(f),
    Err(IoError::UserError) => Err(Reply::new(
      ReplyCode::NotLoggedIn,
      IoError::UserError.to_string(),
    )),
    Err(IoError::NotFoundError(m)) | Err(IoError::InvalidPathError(m)) => {
      Err(Reply::new(ReplyCode::FileUnavailable, m))
    }
    Err(IoError::PermissionError) => Err(Reply::new(
      ReplyCode::FileUnavailable,
      IoError::PermissionError.to_string(),
    )),
    Err(IoError::NotAFileError) => Err(Reply::new(
      ReplyCode::SyntaxErrorInParametersOrArguments,
      IoError::NotAFileError.to_string(),
    )),
    Err(IoError::OsError(_)) => Err(Reply::new(
      ReplyCode::RequestedActionAborted,
      "Requested action aborted: local error in processing.",
    )),
    Err(_) => unreachable!(),
  }
}
