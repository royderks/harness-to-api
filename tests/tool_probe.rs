use serde_json::{Value, json};
use std::{
    io::{Read, Write},
    net::TcpListener,
    process::Command,
    thread,
    time::Duration,
};

fn probe(body: Value, status: u16, tool_call: bool) -> (std::process::Output, Value) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(10)))
            .unwrap();
        let mut bytes = Vec::new();
        let mut buffer = [0; 4096];
        let request = loop {
            let count = stream.read(&mut buffer).unwrap();
            assert_ne!(count, 0);
            bytes.extend_from_slice(&buffer[..count]);
            if let Some(end) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
                let headers = String::from_utf8_lossy(&bytes[..end]);
                assert!(headers.starts_with("POST /v1/chat/completions "));
                let size: usize = headers
                    .lines()
                    .find_map(|line| {
                        let (name, value) = line.split_once(':')?;
                        name.eq_ignore_ascii_case("content-length")
                            .then(|| value.trim().parse().unwrap())
                    })
                    .unwrap();
                if bytes.len() >= end + 4 + size {
                    break serde_json::from_slice::<Value>(&bytes[end + 4..end + 4 + size])
                        .unwrap();
                }
            }
        };
        let body = body.to_string();
        write!(stream, "HTTP/1.1 {status} Test\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
        request
    });
    let mut command = Command::new(env!("CARGO_BIN_EXE_h2a"));
    command
        .args([
            "probe",
            "--model",
            "codex/default",
            "--base-url",
            &format!("http://{address}/v1"),
        ])
        .env("OPENAI_API_KEY", "test-probe-secret");
    if tool_call {
        command.arg("--tool-call");
    }
    let output = command.output().unwrap();
    let request = server.join().unwrap();
    for bytes in [&output.stdout, &output.stderr] {
        let text = String::from_utf8_lossy(bytes);
        assert!(!text.contains("test-probe-secret"));
        assert!(!text.contains("PRIVATE_RESPONSE"));
    }
    (output, request)
}

fn tool_response() -> Value {
    json!({"object":"chat.completion","model":"codex/default","choices":[{"finish_reason":"tool_calls","message":{"role":"assistant","content":null,"tool_calls":[{"id":"call_test","type":"function","function":{"name":"h2a_probe","arguments":"{}"}}]}}]})
}

#[test]
fn explicit_tool_probe_checks_call_without_executing_it() {
    let (output, request) = probe(tool_response(), 200, true);
    assert!(output.status.success());
    let result: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(result["available"], true);
    assert_eq!(result["capability"], "tool_call");
    assert_eq!(request["tool_choice"]["function"]["name"], "h2a_probe");
    assert_eq!(request["parallel_tool_calls"], false);
    assert_eq!(
        request["tools"][0]["function"]["parameters"]["additionalProperties"],
        false
    );
}

#[test]
fn tool_probe_rejects_text_and_malformed_calls() {
    let mut cases = Vec::new();
    let mut text = tool_response();
    text["choices"][0]["finish_reason"] = json!("stop");
    text["choices"][0]["message"]["content"] = json!("PRIVATE_RESPONSE");
    cases.push(text);
    let mut mixed = tool_response();
    mixed["choices"][0]["message"]["content"] = json!("PRIVATE_RESPONSE");
    cases.push(mixed);
    for (path, value) in [("id", json!("")), ("type", json!("other"))] {
        let mut body = tool_response();
        body["choices"][0]["message"]["tool_calls"][0][path] = value;
        cases.push(body);
    }
    for (path, value) in [
        ("name", "other"),
        ("arguments", "not JSON"),
        ("arguments", "{\"secret\":\"PRIVATE_RESPONSE\"}"),
        ("arguments", "[]"),
    ] {
        let mut body = tool_response();
        body["choices"][0]["message"]["tool_calls"][0]["function"][path] = json!(value);
        cases.push(body);
    }
    for body in cases {
        let (output, _) = probe(body, 200, true);
        assert!(!output.status.success());
        let result: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(result["available"], false);
    }
    let (output, _) = probe(json!({"error":{"message":"PRIVATE_RESPONSE"}}), 429, true);
    assert!(!output.status.success());
    let result: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(result["http_status"], 429);
    assert_eq!(result["available"], false);
}

#[test]
fn ordinary_probe_remains_a_single_text_request() {
    let (output, request) = probe(
        json!({"object":"chat.completion","model":"codex/default","choices":[{"finish_reason":"stop","message":{"role":"assistant","content":"PRIVATE_RESPONSE"}}]}),
        200,
        false,
    );
    assert!(output.status.success());
    assert!(request.get("tools").is_none());
    let result: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(result.get("capability").is_none());
}
