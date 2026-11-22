//! S3's multipart upload as WinSCP sends a file (`TS3FileSystem::Sink`): more than 5 MiB goes up in parts of
//! 5 MiB (larger when 10 000 parts would not hold it) - CreateMultipartUpload at the first write, each part
//! a PUT of its length (UploadPart), CompleteMultipartUpload with the parts' ETags at the end, and
//! AbortMultipartUpload when it fails.

use tokio::io::AsyncWriteExt;

use super::{Extra, S3File, Sending, io, split};
use crate::s3_xml;

/// WinSCP's `S3MinMultiPartChunkSize` and `S3MaxMultiPartChunks`.
pub const MIN_PART: u64 = 5 * 1024 * 1024;
pub const MAX_PARTS: u64 = 10_000;

/// A multipart upload's state.
#[derive(Debug)]
pub struct Multipart {
    /// The upload's id once created.
    id: Option<String>,
    part_size: u64,
    total: u64,
    /// The bytes given to the parts so far.
    sent: u64,
    /// The bytes the current part still takes.
    left: u64,
    /// The parts' ETags, in their order.
    pub(super) etags: Vec<String>,
}

/// WinSCP's parts for `size` bytes: their count and size.
pub fn parts_of(size: u64) -> (u64, u64) {
    let parts = size.div_ceil(MIN_PART).clamp(1, MAX_PARTS);
    (parts, MIN_PART.max(size.div_ceil(parts)))
}

impl Multipart {
    /// The upload of `size` bytes in parts, when it takes more than one.
    pub fn for_size(size: u64) -> Option<Self> {
        let (parts, part_size) = parts_of(size);
        (parts > 1).then_some(Self {
            id: None,
            part_size,
            total: size,
            sent: 0,
            left: 0,
            etags: Vec::new(),
        })
    }
}

/// CompleteMultipartUpload's body.
pub fn completion(etags: &[String]) -> String {
    let parts: Vec<String> = (etags.iter().enumerate())
        .map(|(at, etag)| {
            format!(
                "<Part><PartNumber>{}</PartNumber><ETag>{etag}</ETag></Part>",
                at + 1
            )
        })
        .collect();
    format!(
        "<CompleteMultipartUpload>{}</CompleteMultipartUpload>",
        parts.concat()
    )
}

impl S3File {
    /// The upload's id, created at the first part.
    async fn upload_id(&mut self) -> std::io::Result<String> {
        if let Some(id) = self.parts.as_ref().and_then(|parts| parts.id.clone()) {
            return Ok(id);
        }
        let extra = Extra {
            query: &[("uploads", "")],
            ..Extra::default()
        };
        let response = self
            .s3
            .send("POST", split(&self.path), extra)
            .await
            .map_err(io)?;
        let text = response
            .text()
            .await
            .map_err(|error| io(error.to_string()))?;
        let id =
            s3_xml::upload_id(&text).ok_or_else(|| io(format!("{}: no upload id", self.path)))?;
        if let Some(parts) = self.parts.as_mut() {
            parts.id = Some(id.clone());
        }
        Ok(id)
    }

    /// The next part's PUT started.
    async fn start_part(&mut self) -> std::io::Result<()> {
        let id = self.upload_id().await?;
        let Some(parts) = self.parts.as_mut() else {
            return Ok(());
        };
        let length = parts.part_size.min(parts.total - parts.sent);
        parts.left = length;
        let query = vec![
            (
                "partNumber".to_string(),
                (parts.etags.len() + 1).to_string(),
            ),
            ("uploadId".to_string(), id),
        ];
        self.sending = Some(Sending::start(&self.s3, &self.path, length, query));
        Ok(())
    }

    /// The current part's PUT ended: its ETag kept.
    async fn end_part(&mut self) -> std::io::Result<()> {
        if let Some(sending) = self.sending.take() {
            let etag = sending.finished().await?;
            if let Some(parts) = self.parts.as_mut() {
                parts.etags.push(etag);
            }
        }
        Ok(())
    }

    /// `bytes` written into the parts; the upload aborted when that fails.
    pub(super) async fn write_parts(&mut self, bytes: &[u8]) -> std::io::Result<()> {
        let done = self.written(bytes).await;
        if done.is_err() {
            self.abort().await;
        }
        done
    }

    /// `bytes` written into the parts, each ended when full.
    async fn written(&mut self, mut bytes: &[u8]) -> std::io::Result<()> {
        while !bytes.is_empty() {
            if self.sending.is_none() {
                self.start_part().await?;
            }
            let Some(parts) = self.parts.as_mut() else {
                return Ok(());
            };
            if parts.left == 0 {
                return Err(io(format!("{}: more bytes than the file has", self.path)));
            }
            let taken =
                usize::try_from(parts.left).map_or(bytes.len(), |left| left.min(bytes.len()));
            parts.left -= taken as u64;
            parts.sent += taken as u64;
            let full = parts.left == 0;
            if let Some(sending) = self.sending.as_mut() {
                sending.writer.write_all(&bytes[..taken]).await?;
            }
            bytes = &bytes[taken..];
            if full {
                self.end_part().await?;
            }
        }
        Ok(())
    }

    /// The last part ended and the upload completed; aborted when that fails.
    pub(super) async fn finish_parts(&mut self) -> std::io::Result<()> {
        let done = self.complete().await;
        if done.is_err() {
            self.abort().await;
        }
        done
    }

    async fn complete(&mut self) -> std::io::Result<()> {
        if (self.parts.as_ref()).is_some_and(|parts| parts.sent != parts.total) {
            return Err(io(format!("{}: the file ended early", self.path)));
        }
        self.end_part().await?;
        let id = self.upload_id().await?;
        let Some(parts) = self.parts.as_ref() else {
            return Ok(());
        };
        let extra = Extra {
            query: &[("uploadId", id.as_str())],
            body: Some(reqwest::Body::from(completion(&parts.etags))),
            ..Extra::default()
        };
        let response = self
            .s3
            .send("POST", split(&self.path), extra)
            .await
            .map_err(io)?;
        // S3 may fail a completion with 200 and an error in its body.
        let text = response
            .text()
            .await
            .map_err(|error| io(error.to_string()))?;
        match s3_xml::error(&text) {
            Some(error) => Err(io(error)),
            None => Ok(()),
        }
    }

    /// AbortMultipartUpload, its failure left unsaid (the upload's own error is told).
    async fn abort(&mut self) {
        self.sending = None;
        let Some(id) = self.parts.as_ref().and_then(|parts| parts.id.clone()) else {
            return;
        };
        let extra = Extra {
            query: &[("uploadId", id.as_str())],
            ..Extra::default()
        };
        let _ = self.s3.send("DELETE", split(&self.path), extra).await;
    }
}
