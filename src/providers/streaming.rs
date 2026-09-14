//! Bounded SSE decoding. Never replay a request after response data has begun.
use super::{Failure, MAX_REQUEST_BYTES, failure, usage};
use anyhow::Result;
use serde_json::Value;
use tokio_util::sync::CancellationToken;

#[derive(Default)]
struct Decoder {
    pending: Vec<u8>,
    event: Vec<String>,
    received: usize,
}
impl Decoder {
    fn push(&mut self, bytes: &[u8]) -> Result<Vec<String>> {
        self.received += bytes.len();
        if self.received > MAX_REQUEST_BYTES {
            return Err(failure(
                Failure::InvalidResponse,
                "model stream exceeds size limit",
            ));
        }
        self.pending.extend_from_slice(bytes);
        let mut events = Vec::new();
        while let Some(end) = self.pending.iter().position(|b| *b == b'\n') {
            let bytes: Vec<_> = self.pending.drain(..=end).collect();
            let line = std::str::from_utf8(&bytes[..bytes.len() - 1])
                .map_err(|_| failure(Failure::InvalidResponse, "invalid stream encoding"))?
                .trim_end_matches('\r');
            if line.is_empty() {
                if !self.event.is_empty() {
                    events.push(self.event.join("\n"));
                    self.event.clear();
                }
            } else if let Some(data) = line.strip_prefix("data:") {
                self.event
                    .push(data.strip_prefix(' ').unwrap_or(data).to_owned());
            }
        }
        Ok(events)
    }
}

pub(super) async fn consume(
    mut response: reqwest::Response,
    gemini: bool,
    cancel: &CancellationToken,
    report: &mut usage::Attempt,
    delta: &mut super::TextDelta<'_>,
) -> Result<String> {
    if !response
        .headers()
        .get("content-type")
        .and_then(|h| h.to_str().ok())
        .is_some_and(|v| v.starts_with("text/event-stream"))
    {
        return Err(failure(
            Failure::Unsupported,
            "endpoint did not return an SSE stream",
        ));
    }
    let mut decoder = Decoder::default();
    let mut text = String::new();
    let mut complete = false;
    loop {
        let chunk = tokio::select! { biased;
            _ = cancel.cancelled() => return Err(failure(Failure::Cancelled,"operation cancelled")),
            chunk = response.chunk() => chunk.map_err(|_|failure(Failure::Unavailable,"model stream interrupted"))?,
        };
        let Some(chunk) = chunk else { break };
        for data in decoder.push(&chunk)? {
            if data == "[DONE]" {
                continue;
            }
            let value: Value = serde_json::from_str(&data)
                .map_err(|_| failure(Failure::InvalidResponse, "invalid model stream event"))?;
            if value.get("error").is_some() {
                return Err(failure(Failure::Rejected, "model stream reported an error"));
            }
            if value.get("usageMetadata").is_some()
                || value.get("usage").is_some_and(|v| !v.is_null())
            {
                report.response(&value);
            }
            if gemini {
                if value
                    .get("promptFeedback")
                    .and_then(|v| v.get("blockReason"))
                    .is_some()
                {
                    return Err(failure(Failure::Rejected, "model blocked the response"));
                }
                if let Some(candidate) = value["candidates"].as_array().and_then(|a| a.first()) {
                    if let Some(reason) = candidate["finishReason"].as_str() {
                        if reason != "STOP" {
                            return Err(failure(
                                Failure::InvalidResponse,
                                "model stream did not finish normally",
                            ));
                        }
                        complete = true;
                    }
                    if let Some(parts) = candidate["content"]["parts"].as_array() {
                        for part in parts {
                            if part["thought"] == true {
                                continue;
                            }
                            if let Some(piece) = part["text"].as_str() {
                                delta(piece)?;
                                text.push_str(piece);
                            }
                        }
                    }
                }
            } else if let Some(choice) = value["choices"].as_array().and_then(|a| a.first()) {
                if choice["delta"]["refusal"].as_str().is_some() {
                    return Err(failure(Failure::Rejected, "model refused the response"));
                }
                if let Some(reason) = choice["finish_reason"].as_str() {
                    if reason != "stop" {
                        return Err(failure(
                            Failure::InvalidResponse,
                            "model stream did not finish normally",
                        ));
                    }
                    complete = true;
                }
                if let Some(piece) = choice["delta"]["content"].as_str() {
                    delta(piece)?;
                    text.push_str(piece);
                }
            }
        }
    }
    if !complete
        || text.trim().is_empty()
        || !decoder.pending.is_empty()
        || !decoder.event.is_empty()
    {
        return Err(failure(
            Failure::InvalidResponse,
            "model stream ended before a complete response",
        ));
    }
    Ok(text)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn split_utf8_crlf_comments_and_multiline_events() {
        let input = ": heartbeat\r\ndata: {\"text\":\"中文\"}\r\n\r\ndata: one\ndata: two\n\n";
        let mut parser = Decoder::default();
        let mut result = Vec::new();
        for byte in input.as_bytes() {
            result.extend(parser.push(&[*byte]).unwrap());
        }
        assert_eq!(result, ["{\"text\":\"中文\"}", "one\ntwo"]);
    }
}
