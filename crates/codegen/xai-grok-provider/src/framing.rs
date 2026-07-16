use bytes::Bytes;
use futures::Stream;

pub trait Framing<Frame>: Send + Sync + core::fmt::Debug {
    fn id(&self) -> &str;
    fn frame(
        &self,
        _bytes: Box<dyn Stream<Item = Result<Bytes, String>> + Send + Unpin>,
    ) -> Box<dyn Stream<Item = Frame> + Send + Unpin>;
}

#[derive(Debug)]
pub struct SseFraming;

impl Framing<String> for SseFraming {
    fn id(&self) -> &str {
        "sse"
    }

    fn frame(
        &self,
        _bytes: Box<dyn Stream<Item = Result<Bytes, String>> + Send + Unpin>,
    ) -> Box<dyn Stream<Item = String> + Send + Unpin> {
        unimplemented!()
    }
}
