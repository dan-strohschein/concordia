//! Newline-delimited JSON-RPC over a byte stream (one session per stream).

use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader};

use crate::handle::EngineHandle;

/// Serves requests read line by line from `reader`, writing one response line per reply, until EOF.
pub async fn serve<R, W>(handle: &EngineHandle, reader: R, mut writer: W) -> std::io::Result<()>
where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
{
    let session = handle.open_session().await.map_err(std::io::Error::other)?;
    let mut lines = BufReader::new(reader).lines();
    let result = async {
        while let Some(line) = lines.next_line().await? {
            if line.trim().is_empty() {
                continue;
            }
            if let Some(reply) = handle.handle_jsonrpc(session, line).await {
                writer.write_all(reply.as_bytes()).await?;
                writer.write_all(b"\n").await?;
                writer.flush().await?;
            }
        }
        Ok(())
    }
    .await;
    handle.close_session(session);
    result
}
