use std::marker::PhantomData;

use crate::events::LLMEvent;

pub type ProtocolId = String;

pub struct ProtocolBody<Body> {
    pub from: fn(crate::types::LLMRequest) -> Result<Body, String>,
}

pub struct ProtocolStream<Frame, Event, State> {
    pub initial: fn(crate::types::LLMRequest) -> State,
    pub step: fn(&mut State, Event) -> Result<Vec<LLMEvent>, String>,
    pub terminal: Option<fn(&Event) -> bool>,
    pub on_halt: Option<fn(&State) -> Vec<LLMEvent>>,
    _frame: PhantomData<Frame>,
}

pub struct Protocol<Body, Frame, Event, State> {
    pub id: ProtocolId,
    pub body: ProtocolBody<Body>,
    pub stream: ProtocolStream<Frame, Event, State>,
    _frame: PhantomData<Frame>,
}

impl<B, F, E, S> Protocol<B, F, E, S> {
    pub fn new(
        id: impl Into<ProtocolId>,
        body: ProtocolBody<B>,
        stream: ProtocolStream<F, E, S>,
    ) -> Self {
        Self {
            id: id.into(),
            body,
            stream,
            _frame: PhantomData,
        }
    }
}

#[derive(Debug, Default)]
pub struct ProtocolTable {
    protocols: Vec<String>,
}

impl ProtocolTable {
    pub fn new() -> Self {
        Self {
            protocols: Vec::new(),
        }
    }

    pub fn register(&mut self, id: impl Into<ProtocolId>) {
        self.protocols.push(id.into());
    }

    pub fn contains(&self, id: &ProtocolId) -> bool {
        self.protocols.contains(id)
    }
}
