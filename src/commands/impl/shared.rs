use std::sync::Arc;
use std::time::Duration;

use tokio::fs::File;
use tokio::io;
use tokio::io::{AsyncBufRead, AsyncWrite, AsyncWriteExt};
use tokio::time::timeout;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, warn};

use crate::commands::reply::Reply;
use crate::commands::reply_code::ReplyCode;
use crate::data_channels::data_channel_wrapper::{DataChannel, DataChannelWrapper};
use crate::io::entry_data::EntryData;
use crate::io::error::IoError;

#[cfg(not(test))]
pub const ACQUIRE_TIMEOUT: u64 = 15;
#[cfg(test)]
pub const ACQUIRE_TIMEOUT: u64 = 3;

pub const TRANSFER_BUFFER_SIZE: usize = 131072; // 2^17

pub(crate) async fn acquire_data_channel(
  data_wrapper: Arc<dyn DataChannelWrapper>,
) -> Result<(DataChannel, CancellationToken), Reply> {
  debug!("Acquiring data stream!");
  let error_reply = Reply::new(
    ReplyCode::BadSequenceOfCommands,
    "Data channel must be open first!",
  );

  // we wait a bit so the data channel can open shortly after it's required
  match timeout(Duration::from_secs(ACQUIRE_TIMEOUT), data_wrapper.acquire()).await {
    Ok(Ok((data_channel_option, token))) => Ok((data_channel_option, token)),
    Ok(Err(e)) => {
      info!("Error: {e}");
      Err(error_reply)
    }
    Err(e) => {
      info!("Data channel is not available! {e}");
      Err(error_reply)
    }
  }
}

pub(crate) fn get_transfer_reply(success: &Result<(), io::Error>) -> Reply {
  if success.is_ok() {
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
  listing.map_err(map_error_to_reply)
}

fn map_error_to_reply(error: IoError) -> Reply {
  match error {
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
  }
}

pub(crate) fn get_create_directory_reply(result: Result<String, IoError>) -> Reply {
  match result {
    Ok(new_path) => Reply::new(ReplyCode::PathnameCreated, format!("\"{}\"", new_path)),
    Err(error) => map_error_to_reply(error),
  }
}

pub(crate) fn get_change_directory_reply(cd_result: Result<bool, IoError>) -> Reply {
  match cd_result {
    Ok(true) => Reply::new(ReplyCode::RequestedFileActionOkay, "Path changed"),
    Ok(false) => Reply::new(ReplyCode::RequestedFileActionOkay, "Path not changed"),
    Err(e) => map_error_to_reply(e),
  }
}

pub(crate) fn get_delete_reply(dele_result: Result<(), IoError>, directory: bool) -> Reply {
  match (dele_result, directory) {
    (Ok(_), true) => Reply::new(ReplyCode::RequestedFileActionOkay, "File deleted"),
    (Ok(_), false) => Reply::new(ReplyCode::RequestedFileActionOkay, "Folder deleted"),
    (Err(e), _) => map_error_to_reply(e),
  }
}

pub(crate) async fn copy_data<F, T>(from: &mut F, to: &mut T) -> Result<(), io::Error>
where
  F: AsyncBufRead + Unpin,
  T: AsyncWrite + Unpin,
{
  match io::copy_buf(from, to).await {
    Ok(_) => {
      debug!("Flushing data to target");
      if let Err(e) = to.flush().await {
        warn!("Failed to flush data to target! {e}");
      }
      Ok(())
    }
    Err(e) => {
      error!("Write to target failed! {e}");
      Err(e)
    }
  }
}
