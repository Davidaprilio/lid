//! FTP downloads the way urllib's FTP handler did them in 1.x: one connection, no range, binary mode,
//! the user and password from the URL (anonymous otherwise), the size when the server tells it.

use std::pin::Pin;
use std::time::Duration;

use bytes::Bytes;
use reqwest::header::HeaderMap;
use suppaftp::tokio::AsyncFtpStream;
use suppaftp::types::FileType;
use tokio::io::{AsyncRead, AsyncReadExt};

use crate::http::{Body, OpenError, Response};
use crate::pyutil::unquote;

const READ: usize = 256 * 1024;

/// An FTP transfer in progress. The control connection stays open until the body is dropped.
pub struct FtpBody {
    _control: AsyncFtpStream,
    data: Pin<Box<dyn AsyncRead + Send>>,
    buf: Vec<u8>,
}

impl FtpBody {
    pub async fn chunk(&mut self, timeout: Duration) -> Result<Option<Bytes>, OpenError> {
        match tokio::time::timeout(timeout, self.data.read(&mut self.buf)).await {
            Err(_) => Err(OpenError::Network("timed out".into())),
            Ok(Err(e)) => Err(OpenError::Network(e.to_string())),
            Ok(Ok(0)) => Ok(None),
            Ok(Ok(n)) => Ok(Some(Bytes::copy_from_slice(&self.buf[..n]))),
        }
    }
}

fn ftp_error(e: suppaftp::FtpError) -> OpenError {
    match e {
        suppaftp::FtpError::ConnectionError(io) => OpenError::Network(io.to_string()),
        suppaftp::FtpError::UnexpectedResponse(r) => OpenError::Network(format!(
            "ftp error: {}",
            String::from_utf8_lossy(&r.body).trim()
        )),
        other => OpenError::Network(format!("ftp error: {other}")),
    }
}

/// Connect, log in, change to the file's folder and start `RETR`.
pub async fn open(url: &str, timeout: Duration) -> Result<Response, OpenError> {
    let parsed = reqwest::Url::parse(url)
        .map_err(|e| OpenError::Invalid(format!("Not a valid address: {e}")))?;
    let host = parsed
        .host_str()
        .ok_or_else(|| OpenError::Invalid("ftp error: no host given".into()))?
        .to_string();
    let port = parsed.port().unwrap_or(21);
    let user = unquote(parsed.username(), "utf-8");
    let password = unquote(parsed.password().unwrap_or(""), "utf-8");
    let (user, password) = if user.is_empty() {
        ("anonymous".to_string(), "anonymous@".to_string())
    } else {
        (user, password)
    };
    let path = unquote(parsed.path(), "utf-8");
    let mut parts: Vec<&str> = path.split('/').collect();
    let file = parts.pop().unwrap_or("").to_string();
    if file.is_empty() {
        return Err(OpenError::Invalid(
            "ftp error: the address names a folder, not a file".into(),
        ));
    }

    let addr = format!("{host}:{port}");
    let connect = AsyncFtpStream::connect(addr.as_str());
    let mut ftp = tokio::time::timeout(timeout, connect)
        .await
        .map_err(|_| OpenError::Network("timed out".into()))?
        .map_err(ftp_error)?;
    let steps = async {
        ftp.login(user.as_str(), password.as_str()).await?;
        ftp.transfer_type(FileType::Binary).await?;
        for dir in parts.iter().filter(|d| !d.is_empty()) {
            ftp.cwd(dir).await?;
        }
        let size = ftp.size(&file).await.ok();
        let stream = ftp.retr_as_stream(&file).await?;
        Ok::<_, suppaftp::FtpError>((size, stream))
    };
    let (size, stream) = tokio::time::timeout(timeout, steps)
        .await
        .map_err(|_| OpenError::Network("timed out".into()))?
        .map_err(ftp_error)?;
    Ok(Response {
        status: 200,
        url: url.to_string(),
        headers: HeaderMap::new(),
        length: size.map(|s| s as u64),
        body: Body::Ftp(Box::new(FtpBody {
            _control: ftp,
            data: Box::pin(stream),
            buf: vec![0; READ],
        })),
    })
}
