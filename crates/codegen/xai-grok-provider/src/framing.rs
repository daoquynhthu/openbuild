use bytes::Bytes;
use futures::Stream;

pub trait Framing<Frame>: Send + Sync + core::fmt::Debug {
    fn id(&self) -> &str;
    fn frame(
        &self,
        bytes: Box<dyn Stream<Item = Result<Bytes, String>> + Send + Unpin>,
    ) -> Box<dyn Stream<Item = Frame> + Send + Unpin>;
    fn clone_box(&self) -> Box<dyn Framing<Frame>>;
}

#[derive(Debug)]
pub struct SseFraming;

impl Framing<String> for SseFraming {
    fn id(&self) -> &str {
        "sse"
    }

    fn clone_box(&self) -> Box<dyn Framing<String>> {
        Box::new(SseFraming)
    }

    fn frame(
        &self,
        bytes: Box<dyn Stream<Item = Result<Bytes, String>> + Send + Unpin>,
    ) -> Box<dyn Stream<Item = String> + Send + Unpin> {
        use futures::StreamExt;

        let stream = bytes
            .map(|result| match result {
                Ok(b) => String::from_utf8_lossy(&b).to_string(),
                Err(e) => e,
            })
            .flat_map(|text| {
                let events: Vec<String> = text
                    .lines()
                    .filter_map(|line| {
                        let line = line.trim();
                        line.strip_prefix("data: ").and_then(|data| {
                            let data = data.trim();
                            if data.is_empty() || data == "[DONE]" {
                                None
                            } else {
                                Some(data.to_string())
                            }
                        })
                    })
                    .collect();
                futures::stream::iter(events)
            });
        Box::new(stream)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::StreamExt;

    fn stream_from_chunks(
        chunks: Vec<&str>,
    ) -> Box<dyn Stream<Item = Result<Bytes, String>> + Send + Unpin> {
        let items: Vec<Result<Bytes, String>> = chunks
            .into_iter()
            .map(|s| Ok(Bytes::copy_from_slice(s.as_bytes())))
            .collect();
        Box::new(futures::stream::iter(items))
    }

    #[tokio::test]
    async fn sse_framing_decodes_data_lines() {
        let input = stream_from_chunks(vec![
            "data: {\"hello\":\"world\"}\n\ndata: {\"foo\":\"bar\"}\n",
        ]);
        let framing = SseFraming;
        let events: Vec<String> = framing.frame(input).collect().await;
        assert_eq!(events.len(), 2);
        assert_eq!(events[0], r#"{"hello":"world"}"#);
        assert_eq!(events[1], r#"{"foo":"bar"}"#);
    }

    #[tokio::test]
    async fn sse_framing_filters_done() {
        let input = stream_from_chunks(vec!["data: hello\n\ndata: [DONE]\n\ndata: world\n"]);
        let framing = SseFraming;
        let events: Vec<String> = framing.frame(input).collect().await;
        assert_eq!(events.len(), 2);
        assert_eq!(events[0], "hello");
        assert_eq!(events[1], "world");
    }

    #[tokio::test]
    async fn sse_framing_empty_stream() {
        let input = stream_from_chunks(vec![]);
        let framing = SseFraming;
        let events: Vec<String> = framing.frame(input).collect().await;
        assert!(events.is_empty());
    }
}
