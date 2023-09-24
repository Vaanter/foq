use std::sync::Arc;

use tokio::fs::File;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::sync::{Mutex, OwnedMutexGuard};
use tracing::{debug, error, info, trace, warn};

use crate::commands::reply::Reply;
use crate::commands::reply_code::ReplyCode;
use crate::data_channels::data_channel_wrapper::DataChannelWrapper;
use crate::handlers::connection_handler::AsyncReadWrite;
use crate::io::entry_data::EntryData;
use crate::io::error::IoError;

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
    Err(e) => Err(map_error_to_reply(e)),
  }
}

pub(crate) fn get_listing_or_error_reply(
  listing: Result<Vec<EntryData>, IoError>,
) -> Result<Vec<EntryData>, Reply> {
  return listing.map_err(|e| map_error_to_reply(e));
}

fn map_error_to_reply(error: IoError) -> Reply {
  return match error {
    IoError::UserError => Reply::new(ReplyCode::NotLoggedIn, IoError::UserError.to_string()),
    IoError::OsError(_) | IoError::SystemError => Reply::new(
      ReplyCode::RequestedActionAborted,
      "Requested action aborted: local error in processing.",
    ),
    IoError::NotADirectoryError => Reply::new(
      ReplyCode::SyntaxErrorInParametersOrArguments,
      IoError::NotADirectoryError.to_string(),
    ),
    IoError::PermissionError => Reply::new(
      ReplyCode::FileUnavailable,
      IoError::PermissionError.to_string(),
    ),
    IoError::NotFoundError(message) | IoError::InvalidPathError(message) => {
      Reply::new(ReplyCode::FileUnavailable, message)
    }
    IoError::NotAFileError => Reply::new(
      ReplyCode::SyntaxErrorInParametersOrArguments,
      IoError::NotAFileError.to_string(),
    ),
  };
}

pub(crate) fn get_change_directory_reply(cd_result: Result<bool, IoError>) -> Reply {
  return match cd_result {
    Ok(true) => Reply::new(ReplyCode::RequestedFileActionOkay, "Path changed"),
    Ok(false) => Reply::new(ReplyCode::RequestedFileActionOkay, "Path not changed"),
    Err(e) => map_error_to_reply(e),
  };
}

pub(crate) async fn transfer_data<F, T>(from: &mut F, to: &mut T, buffer: &mut [u8]) -> bool
where
  F: AsyncRead + Unpin,
  T: AsyncWrite + Unpin,
{
  let mut success = loop {
    let result = from.read(buffer).await;
    match result {
      Ok(n) => {
        trace!("Read {n} bytes from server");
        let mut sent = 0;
        while sent < n {
          match to.write(&buffer[sent..n]).await {
            Ok(current_sent) => sent += current_sent,
            Err(e) => {
              error!("Write to client failed! {e}");
              break;
            }
          }
        }
        if n == 0 {
          break true;
        }
      }
      Err(e) => {
        error!("Failed to send server's data to client. {e}");
        break false;
      }
    }
  };
  debug!("Flushing data to client");
  if let Err(e) = to.flush().await {
    warn!("Failed to flush data to client data channel! {e}");
    success = false;
  }
  success
}
