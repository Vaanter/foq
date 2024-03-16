use std::sync::Arc;
use std::time::Duration;

use tokio::fs::File;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::time::timeout;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, trace, warn};

use crate::commands::reply::Reply;
use crate::commands::reply_code::ReplyCode;
use crate::data_channels::data_channel_wrapper::{DataChannel, DataChannelWrapper};
use crate::io::entry_data::EntryData;
use crate::io::error::IoError;

pub const ACQUIRE_TIMEOUT: u64 = 5;

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

#[tracing::instrument(skip_all)]
pub(crate) async fn transfer_data<F, T>(from: &mut F, to: &mut T, buffer: &mut [u8]) -> bool
where
  F: AsyncRead + Unpin,
  T: AsyncWrite + Unpin,
{
  'send_loop: loop {
    let result = from.read(buffer).await;
    match result {
      Ok(0) => {
        debug!("Flushing data to target");
        if let Err(e) = to.flush().await {
          warn!("Failed to flush data to target! {e}");
          break 'send_loop false;
        }
        break 'send_loop true;
      }
      Ok(n) => {
        trace!("Read {n} bytes from source");
        if let Err(e) = to.write_all(&buffer[..n]).await {
          error!("Write to target failed! {e}");
          break 'send_loop false;
        }
      }
      Err(e) => {
        error!("Failed to send data to target. {e}");
        break false;
      }
    }
  }
}
