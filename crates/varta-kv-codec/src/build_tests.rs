//! A yielding heap must follow the exact synchronous construction sequence,
//! without replaying allocations or losing the single root before suspension.
use super::build::*;
use alloc::{vec, vec::Vec};
use varta_common::{
    abi,
    json::{JsonField, JsonSchema, JsonType, JsonValue, build::Heap},
    limits::DataLimits,
    value::*,
};
#[derive(Default, PartialEq, Debug)]
struct Memory {
    bytes: Vec<u8>,
    events: Vec<(u8, usize, Vec<u8>)>,
    roots: Vec<u64>,
    allocations: usize,
}
impl Heap for Memory {
    type Error = &'static str;
    fn allocate(&mut self, len: usize) -> Result<usize, Self::Error> {
        if self.allocations != 0 {
            assert_eq!(
                self.roots.len(),
                1,
                "the root precedes every child allocation"
            );
        }
        self.allocations += 1;
        let at = self
            .bytes
            .len()
            .max(abi::OWNED_STRING_BASE)
            .next_multiple_of(HEAP_ALIGNMENT);
        let size = (abi::STRING_HEADER_BYTES + len).next_multiple_of(HEAP_ALIGNMENT);
        self.bytes.resize(at + size, 0);
        self.bytes[at..at + 4].copy_from_slice(&(size as u32).to_le_bytes());
        self.bytes[at + HEAP_FLAGS_OFFSET..at + HEAP_FLAGS_OFFSET + 4]
            .copy_from_slice(&HEAP_ALLOCATED_FLAG.to_le_bytes());
        self.events.push((0, len, vec![]));
        Ok(at + abi::STRING_HEADER_BYTES)
    }
    fn write(&mut self, at: usize, data: &[u8]) -> Result<(), Self::Error> {
        self.bytes[at..at + data.len()].copy_from_slice(data);
        self.events.push((1, at, data.to_vec()));
        Ok(())
    }
    fn zero(&mut self, at: usize, len: usize) -> Result<(), Self::Error> {
        self.bytes[at..at + len].fill(0);
        Ok(())
    }
    fn retain(&mut self, value: u64) -> Result<(), Self::Error> {
        self.roots.push(value);
        self.events.push((2, 0, value.to_le_bytes().to_vec()));
        Ok(())
    }
}
async fn suspend() {
    let mut once = false;
    core::future::poll_fn(move |cx| {
        if once {
            core::task::Poll::Ready(())
        } else {
            once = true;
            cx.waker().wake_by_ref();
            core::task::Poll::Pending
        }
    })
    .await
}
impl AsyncHeap for Memory {
    async fn allocate_async(&mut self, len: usize) -> Result<usize, Self::Error> {
        suspend().await;
        self.allocate(len)
    }
    async fn retain_async(&mut self, value: u64) -> Result<(), Self::Error> {
        suspend().await;
        self.retain(value)
    }
}
#[test]
fn yielding_builder_keeps_sync_order_bytes_and_root_with_nested_containers() {
    let field = |name: &str, ty| JsonField {
        name: name.into(),
        ty,
        optional: false,
    };
    let schema = JsonSchema {
        version: varta_common::json::HASHED_DICTIONARY_SCHEMA_VERSION,
        root: 3,
        types: vec![
            JsonType::String,
            JsonType::Array { element: 0 },
            JsonType::Dictionary {
                descriptor: 128,
                value: 1,
            },
            JsonType::Record {
                descriptor: 256,
                fields: vec![field("text", 0), field("data", 2)],
            },
        ],
    };
    let item = JsonValue::Record {
        ty: 3,
        fields: vec![
            JsonValue::String("root🦀".into()),
            JsonValue::Dictionary {
                ty: 2,
                entries: vec![(
                    vec![0xd800],
                    JsonValue::Array {
                        ty: 1,
                        values: vec![
                            JsonValue::String("one".into()),
                            JsonValue::String("two".into()),
                        ],
                    },
                )],
            },
        ],
    };
    let mut sync = Memory::default();
    let expected = value(&mut sync, &schema, &item, DataLimits::V38).unwrap();
    let mut asynchronous = Memory::default();
    let mut pending = 0;
    let actual = {
        let mut future = core::pin::pin!(value_async(
            &mut asynchronous,
            &schema,
            &item,
            DataLimits::V38
        ));
        let mut cx = core::task::Context::from_waker(core::task::Waker::noop());
        loop {
            match core::future::Future::poll(future.as_mut(), &mut cx) {
                core::task::Poll::Ready(value) => break value.unwrap(),
                core::task::Poll::Pending => pending += 1,
            }
        }
    };
    assert_eq!(pending, sync.allocations + sync.roots.len());
    assert_eq!(actual, expected);
    assert_eq!(asynchronous, sync);
}

impl crate::guest::Guest for Memory {
    type Memory = [u8];
    fn memory(&self) -> &[u8] {
        &self.bytes
    }
    fn ready(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }
    fn deadline(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }
    fn charge(&mut self, _: u64) -> Result<(), Self::Error> {
        Ok(())
    }
    fn throw(&mut self, _: u64) -> Result<(), Self::Error> {
        Ok(())
    }
    fn result_length(&mut self, _: i32) -> Result<(), Self::Error> {
        Ok(())
    }
    fn dispose(&mut self, value: JsonValue) -> Result<(), Self::Error> {
        drop(value);
        Ok(())
    }
    fn json_fatal(&self, _: alloc::string::String) -> Self::Error {
        "json"
    }
    fn kv_fatal(&self, _: &'static str) -> Self::Error {
        "kv"
    }
    fn abi_fatal(&self, _: ValueAbiError) -> Self::Error {
        "abi"
    }
    fn deadline_error(&self) -> Self::Error {
        "deadline"
    }
}
#[test]
fn cancelled_or_unpolled_deep_materialization_drops_on_a_small_stack() {
    for polled in [false, true] {
        let mut value = JsonValue::Array {
            ty: 0,
            values: vec![],
        };
        for _ in 0..8192 {
            value = JsonValue::Array {
                ty: 0,
                values: vec![value],
            };
        }
        std::thread::Builder::new()
            .stack_size(64 * 1024)
            .spawn(move || {
                let schema = JsonSchema {
                    version: varta_common::json::HASHED_DICTIONARY_SCHEMA_VERSION,
                    root: 0,
                    types: vec![JsonType::Array { element: 0 }],
                };
                let module = ModuleBounds::v38(abi::StaticRegion::SINGLE_PAGE);
                let mut heap = Memory::default();
                {
                    let future = crate::guest::materialize_async(&mut heap, module, &schema, value);
                    let mut future = core::pin::pin!(future);
                    if polled {
                        let mut cx = core::task::Context::from_waker(core::task::Waker::noop());
                        assert!(core::future::Future::poll(future.as_mut(), &mut cx).is_pending());
                    }
                }
                assert!(heap.events.is_empty());
            })
            .unwrap()
            .join()
            .unwrap();
    }
}
