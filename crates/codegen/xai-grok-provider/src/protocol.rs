use std::collections::HashSet;
use std::marker::PhantomData;

use crate::error::ProviderError;
use crate::events::LLMEvent;
use crate::types::LLMRequest;

pub use xai_grok_sampling_types::ProtocolId;

/// Schema for validating and decoding provider-native types.
/// The `validate` function pointer returns `Ok(())` or a `ProviderError`.
pub struct Schema<T> {
    pub validate: fn(&T) -> Result<(), ProviderError>,
}

impl<T> core::fmt::Debug for Schema<T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "Schema<{}>", std::any::type_name::<T>())
    }
}

impl<T> Clone for Schema<T> {
    fn clone(&self) -> Self {
        Self {
            validate: self.validate,
        }
    }
}

/// Body construction for a [`Protocol`]: schema validation + request lowering.
#[non_exhaustive]
pub struct ProtocolBody<Body> {
    pub schema: Schema<Body>,
    pub from: fn(LLMRequest) -> Result<Body, ProviderError>,
}

impl<Body> core::fmt::Debug for ProtocolBody<Body> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("ProtocolBody")
            .field("schema", &self.schema)
            .finish()
    }
}

/// Stream processing for a [`Protocol`]: event schema + per-step dispatch.
#[non_exhaustive]
pub struct ProtocolStream<Frame, Event, State> {
    pub event: Schema<Event>,
    pub initial: fn(LLMRequest) -> State,
    pub step: fn(&mut State, Event) -> Result<Vec<LLMEvent>, ProviderError>,
    pub terminal: Option<fn(&Event) -> bool>,
    pub on_halt: Option<fn(&State) -> Vec<LLMEvent>>,
    _frame: PhantomData<Frame>,
}

impl<Frame, Event, State> core::fmt::Debug for ProtocolStream<Frame, Event, State> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("ProtocolStream")
            .field("event", &self.event)
            .field("terminal", &self.terminal.is_some())
            .finish()
    }
}

#[non_exhaustive]
pub struct Protocol<Body, Frame, Event, State> {
    pub id: ProtocolId,
    pub body: ProtocolBody<Body>,
    pub stream: ProtocolStream<Frame, Event, State>,
    _frame: PhantomData<Frame>,
}

impl<Body, Frame, Event, State> core::fmt::Debug for Protocol<Body, Frame, Event, State> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Protocol")
            .field("id", &self.id)
            .field("body", &self.body)
            .field("stream", &self.stream)
            .finish()
    }
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
    protocols: HashSet<ProtocolId>,
}

impl ProtocolTable {
    pub fn new() -> Self {
        Self {
            protocols: HashSet::new(),
        }
    }

    pub fn register(&mut self, id: impl Into<ProtocolId>) {
        self.protocols.insert(id.into());
    }

    pub fn contains(&self, id: &str) -> bool {
        let key = ProtocolId(id.to_owned());
        self.protocols.contains(&key)
    }

    pub fn all_ids(&self) -> Vec<ProtocolId> {
        self.protocols.iter().cloned().collect()
    }
}

/// Return the table of all known inference protocols for V1.
pub fn known_protocols() -> &'static ProtocolTable {
    use std::sync::OnceLock;
    static TABLE: OnceLock<ProtocolTable> = OnceLock::new();
    TABLE.get_or_init(|| {
        let mut t = ProtocolTable::new();
        t.register("chat_completions");
        t.register("responses");
        t.register("messages");
        t
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn protocol_new_sets_id() {
        let body = ProtocolBody::<()> {
            schema: Schema {
                validate: |_| Ok(()),
            },
            from: |_| Ok(()),
        };
        let stream = ProtocolStream::<(), (), ()> {
            event: Schema {
                validate: |_| Ok(()),
            },
            initial: |_| (),
            step: |_, _| Ok(vec![]),
            terminal: None,
            on_halt: None,
            _frame: PhantomData,
        };
        let protocol = Protocol::new("test", body, stream);
        assert_eq!(protocol.id, "test");
    }

    #[test]
    fn protocol_table_register_and_contains() {
        let mut table = ProtocolTable::new();
        assert!(!table.contains("chat"));
        table.register("chat");
        assert!(table.contains("chat"));
        table.register("responses");
        assert_eq!(table.all_ids().len(), 2);
    }

    #[test]
    fn schema_validate_passes() {
        let s = Schema::<()> {
            validate: |_| Ok(()),
        };
        assert!((s.validate)(&()).is_ok());
    }

    #[test]
    fn schema_validate_fails() {
        let s = Schema::<String> {
            validate: |v| {
                if v.is_empty() {
                    Err(ProviderError::Protocol("empty".to_string()))
                } else {
                    Ok(())
                }
            },
        };
        assert!((s.validate)(&String::new()).is_err());
        assert!((s.validate)(&"ok".to_owned()).is_ok());
    }
}
