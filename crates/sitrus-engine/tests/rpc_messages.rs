//! Contrato de serialização das mensagens JSON-RPC trocadas com o sidecar.

use std::time::Duration;

use serde_json::json;
use sitrus_engine::rpc::{
    self, EventParams, IncomingMessage, MethodCall, RpcRequestFrame, WireQueueItem, WireState,
};
use sitrus_engine::{EngineEvent, EngineState};

fn request_frame(id: u64, call: &MethodCall) -> serde_json::Value {
    serde_json::to_value(RpcRequestFrame {
        jsonrpc: "2.0",
        id,
        call,
    })
    .expect("request frame serializes")
}

#[test]
fn initialize_frame_shape() {
    let frame = request_frame(
        0,
        &MethodCall::Initialize {
            protocol: rpc::PROTOCOL_VERSION,
            token: "secret".to_owned(),
        },
    );
    assert_eq!(
        frame,
        json!({
            "jsonrpc": "2.0",
            "id": 0,
            "method": "initialize",
            "params": {"protocol": 1, "token": "secret"}
        })
    );
}

#[test]
fn load_frame_uses_camel_case_wire_fields() {
    let frame = request_frame(
        1,
        &MethodCall::Load {
            items: vec![WireQueueItem {
                id: "abc-1".to_owned(),
                title: None,
                duration_ms: Some(210_000),
                url: None,
            }],
        },
    );
    assert_eq!(
        frame,
        json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "load",
            "params": {
                "items": [
                    {"id": "abc-1", "title": null, "durationMs": 210_000, "url": null}
                ]
            }
        })
    );
}

#[test]
fn command_frames_with_empty_params() {
    assert_eq!(
        request_frame(2, &MethodCall::Play {}),
        json!({"jsonrpc": "2.0", "id": 2, "method": "play", "params": {}})
    );
    assert_eq!(
        request_frame(3, &MethodCall::Pause {}),
        json!({"jsonrpc": "2.0", "id": 3, "method": "pause", "params": {}})
    );
    assert_eq!(
        request_frame(
            4,
            &MethodCall::Seek {
                position_ms: 30_000,
            },
        ),
        json!({"jsonrpc": "2.0", "id": 4, "method": "seek", "params": {"positionMs": 30_000}})
    );
    assert_eq!(
        request_frame(5, &MethodCall::Volume { volume: 0.5 }),
        json!({"jsonrpc": "2.0", "id": 5, "method": "volume", "params": {"volume": 0.5}})
    );
}

#[test]
fn event_notifications_round_trip_from_wire_literals() {
    let samples = [
        (
            json!({"jsonrpc": "2.0", "method": "event", "params": {"type": "state", "value": "playing"}}),
            EventParams::State {
                value: WireState::Playing,
            },
        ),
        (
            json!({"jsonrpc": "2.0", "method": "event", "params": {"type": "position", "ms": 15_000}}),
            EventParams::Position { ms: 15_000 },
        ),
        (
            json!({"jsonrpc": "2.0", "method": "event", "params": {"type": "duration", "ms": 210_000}}),
            EventParams::Duration { ms: 210_000 },
        ),
        (
            json!({"jsonrpc": "2.0", "method": "event", "params": {"type": "ended"}}),
            EventParams::Ended {},
        ),
        (
            json!({"jsonrpc": "2.0", "method": "event", "params": {"type": "error", "message": "drm unavailable"}}),
            EventParams::Error {
                message: "drm unavailable".to_owned(),
            },
        ),
    ];
    for (wire, expected) in samples {
        let parsed: EventParams =
            serde_json::from_value(wire["params"].clone()).expect("event params parse");
        assert_eq!(parsed, expected, "round trip for {wire}");
        let reencoded = serde_json::to_value(&expected).expect("event params serialize");
        assert_eq!(reencoded, wire["params"], "re-encode for {wire}");
    }
}

#[test]
fn engine_event_conversions_preserve_payloads() {
    let event = EngineEvent::StateChanged(EngineState::Paused);
    let params = EventParams::from(&event);
    assert_eq!(
        params,
        EventParams::State {
            value: WireState::Paused
        }
    );
    let back: EngineEvent = params.try_into().expect("conversion back");
    assert_eq!(back, event);

    let event = EngineEvent::Position(Duration::from_millis(1_500));
    let params = EventParams::from(&event);
    assert_eq!(params, EventParams::Position { ms: 1_500 });

    let event = EngineEvent::Error("boom".to_owned());
    let params = EventParams::from(&event);
    assert_eq!(
        params,
        EventParams::Error {
            message: "boom".to_owned()
        }
    );
}

#[test]
fn incoming_message_parse_classifies_frames() {
    let response = IncomingMessage::parse(json!({
        "jsonrpc": "2.0", "id": 7, "result": null
    }))
    .expect("response with null result");
    match response {
        IncomingMessage::Response(frame) => {
            assert_eq!(frame.id, 7);
            assert!(frame.error.is_none());
            assert!(frame.result.is_none());
        }
        other => panic!("expected a response, got {other:?}"),
    }

    let error = IncomingMessage::parse(json!({
        "jsonrpc": "2.0", "id": 8,
        "error": {"code": -32601, "message": "unknown method"}
    }))
    .expect("response with error object");
    match error {
        IncomingMessage::Response(frame) => {
            let rpc_error = frame.error.expect("error object present");
            assert_eq!(rpc_error.code, -32601);
            assert_eq!(rpc_error.message, "unknown method");
        }
        other => panic!("expected a response, got {other:?}"),
    }

    let event = IncomingMessage::parse(json!({
        "jsonrpc": "2.0", "method": "event", "params": {"type": "ended"}
    }))
    .expect("event notification");
    match event {
        IncomingMessage::Event(EventParams::Ended {}) => {}
        other => panic!("expected an ended event, got {other:?}"),
    }
}

#[test]
fn incoming_message_rejects_frames_outside_the_protocol() {
    for frame in [
        json!({"jsonrpc": "2.0", "id": 1}), // resposta sem result/error
        json!({"jsonrpc": "2.0", "method": "mystery"}), // notificação não-event
        json!([1, 2, 3]),                   // não objeto
        json!({"jsonrpc": "2.0", "method": "event", "params": {"type": "junk"}}),
    ] {
        assert!(
            matches!(
                IncomingMessage::parse(frame),
                Err(sitrus_engine::EngineError::InvalidMessage { .. })
            ),
            "frame outside the protocol must be rejected"
        );
    }
}

#[test]
fn session_probe_frame_is_a_first_class_method() {
    let frame = request_frame(9, &MethodCall::SessionProbe {});
    assert_eq!(
        frame,
        json!({"jsonrpc": "2.0", "id": 9, "method": "session_probe", "params": {}})
    );

    // Resposta observada no modo electron (cookies.get do perfil do Electron):
    let response = IncomingMessage::parse(json!({
        "jsonrpc": "2.0", "id": 9,
        "result": {
            "cookies": [
                {"name": "citm", "value": "…", "domain": ".apple.com", "path": "/", "secure": true}
            ],
            "mediaUserToken": "media-token"
        }
    }))
    .expect("session_probe response");
    match response {
        IncomingMessage::Response(frame) => {
            assert!(frame.error.is_none());
            let result = frame.result.expect("probe carries a payload");
            assert_eq!(result["cookies"][0]["domain"], ".apple.com");
            assert_eq!(result["mediaUserToken"], "media-token");
        }
        other => panic!("expected a response, got {other:?}"),
    }
}

#[test]
fn ready_line_parser_accepts_the_exact_marker() {
    let ready =
        rpc::parse_ready_line("SITRUS_SIDECAR_READY {\"port\": 41231}").expect("valid ready line");
    assert_eq!(ready.port, 41_231);

    for broken in [
        "SITRUS_SIDECAR_READY",
        "OTHER_READY {\"port\": 1}",
        "SITRUS_SIDECAR_READY {\"port\": \"x\"}",
        "SITRUS_SIDECAR_READY {\"port\": 99999}",
        "SITRUS_SIDECAR_READY not-json",
    ] {
        assert!(
            matches!(
                rpc::parse_ready_line(broken),
                Err(sitrus_engine::EngineError::InvalidMessage { .. })
            ),
            "broken ready line must be rejected: {broken}"
        );
    }
}
