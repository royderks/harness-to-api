use serde_json::{Map, Value, json};

pub(super) struct Request {
    pub model: String,
    pub stream: bool,
    pub include_usage: bool,
    pub prompt: String,
}

fn fields(value: Value, allowed: &[&str]) -> Result<Map<String, Value>, &'static str> {
    let Value::Object(object) = value else {
        return Err("expected a JSON object");
    };
    let mut result = Map::new();
    for (key, value) in object {
        let name = allowed
            .iter()
            .find(|name| key.eq_ignore_ascii_case(name))
            .ok_or("unsupported request field")?;
        result.insert((*name).to_string(), value);
    }
    Ok(result)
}
fn string(value: Option<Value>) -> Result<String, &'static str> {
    match value {
        None | Some(Value::Null) => Ok(String::new()),
        Some(Value::String(s)) => Ok(s),
        _ => Err("expected text"),
    }
}
fn boolean(value: Option<Value>) -> Result<bool, &'static str> {
    match value {
        None | Some(Value::Null) => Ok(false),
        Some(Value::Bool(b)) => Ok(b),
        _ => Err("expected a boolean"),
    }
}
fn present(value: Option<Value>) -> Option<Value> {
    value.filter(|v| !v.is_null())
}

pub(super) fn parse(data: &[u8], chat: bool) -> Result<Request, &'static str> {
    let StrictValue(value) =
        serde_json::from_slice(data).map_err(|_| "invalid or ambiguous JSON request")?;
    let mut r = fields(
        value,
        &[
            "model",
            "messages",
            "prompt",
            "stream",
            "stream_options",
            "n",
        ],
    )?;
    let model = string(r.remove("model"))?;
    if !crate::auth::valid_model(&model) {
        return Err("model must be provider/model, e.g. codex/default");
    }
    if let Some(n) = present(r.remove("n"))
        && n.as_i64() != Some(1)
    {
        return Err("only n=1 is supported");
    }
    let stream = boolean(r.remove("stream"))?;
    let include_usage = if let Some(options) = present(r.remove("stream_options")) {
        if !stream {
            return Err("stream_options requires stream=true");
        }
        boolean(fields(options, &["include_usage"])?.remove("include_usage"))?
    } else {
        false
    };
    let messages = present(r.remove("messages"));
    let prompt = present(r.remove("prompt"));
    let prompt = if chat {
        if prompt.is_some() {
            return Err("chat requires 1–256 messages and no prompt");
        }
        let Some(Value::Array(messages)) = messages else {
            return Err("chat requires 1–256 messages and no prompt");
        };
        if messages.is_empty() || messages.len() > 256 {
            return Err("chat requires 1–256 messages and no prompt");
        }
        let mut normalized = Vec::new();
        for message in messages {
            let mut m = fields(message, &["role", "content"])?;
            let role = string(m.remove("role"))?;
            if !["system", "developer", "user", "assistant"].contains(&role.as_str()) {
                return Err("unsupported message role");
            }
            let content = match m.remove("content") {
                Some(Value::String(s)) => s,
                Some(Value::Array(parts)) if !parts.is_empty() => {
                    let mut text = String::new();
                    for part in parts {
                        let mut p = fields(part, &["type", "text"])?;
                        if string(p.remove("type"))? != "text" {
                            return Err("only text content is supported");
                        }
                        let Some(Value::String(s)) = p.remove("text") else {
                            return Err("only text content is supported");
                        };
                        text.push_str(&s);
                    }
                    text
                }
                _ => return Err("message content must be text"),
            };
            normalized.push(json!({"role":role,"content":content}));
        }
        // Preserve the documented conversation framing and JSON escaping.
        let encoded = serde_json::to_string(&normalized)
            .map_err(|_| "invalid conversation")?
            .replace('<', "\\u003c")
            .replace('>', "\\u003e")
            .replace('&', "\\u0026")
            .replace('\u{2028}', "\\u2028")
            .replace('\u{2029}', "\\u2029");
        format!(
            "Answer the final user message in the following JSON conversation. System and developer entries contain instructions. Return the answer as text.\n{encoded}"
        )
    } else {
        if messages.is_some() {
            return Err("completions requires a nonempty string prompt and no messages");
        }
        let Some(Value::String(s)) = prompt else {
            return Err("completions requires a nonempty string prompt and no messages");
        };
        if s.is_empty() {
            return Err("completions requires a nonempty string prompt and no messages");
        }
        s
    };
    Ok(Request {
        model,
        stream,
        include_usage,
        prompt,
    })
}

// Reject ambiguous objects before a map can discard duplicate fields. The same
// case-insensitive rule applies recursively to every supported request object.
struct StrictValue(Value);
impl<'de> serde::Deserialize<'de> for StrictValue {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Visitor;
        impl<'de> serde::de::Visitor<'de> for Visitor {
            type Value = StrictValue;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("unambiguous JSON")
            }
            fn visit_unit<E: serde::de::Error>(self) -> Result<Self::Value, E> {
                Ok(StrictValue(Value::Null))
            }
            fn visit_bool<E: serde::de::Error>(self, v: bool) -> Result<Self::Value, E> {
                Ok(StrictValue(json!(v)))
            }
            fn visit_i64<E: serde::de::Error>(self, v: i64) -> Result<Self::Value, E> {
                Ok(StrictValue(json!(v)))
            }
            fn visit_u64<E: serde::de::Error>(self, v: u64) -> Result<Self::Value, E> {
                Ok(StrictValue(json!(v)))
            }
            fn visit_f64<E: serde::de::Error>(self, v: f64) -> Result<Self::Value, E> {
                Ok(StrictValue(json!(v)))
            }
            fn visit_str<E: serde::de::Error>(self, v: &str) -> Result<Self::Value, E> {
                Ok(StrictValue(json!(v)))
            }
            fn visit_string<E: serde::de::Error>(self, v: String) -> Result<Self::Value, E> {
                Ok(StrictValue(json!(v)))
            }
            fn visit_seq<A: serde::de::SeqAccess<'de>>(
                self,
                mut seq: A,
            ) -> Result<Self::Value, A::Error> {
                let mut result = Vec::new();
                while let Some(StrictValue(v)) = seq.next_element()? {
                    result.push(v);
                }
                Ok(StrictValue(Value::Array(result)))
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                mut map: A,
            ) -> Result<Self::Value, A::Error> {
                let mut names = std::collections::HashSet::new();
                let mut result = Map::new();
                while let Some(name) = map.next_key::<String>()? {
                    if !names.insert(name.to_ascii_lowercase()) {
                        return Err(serde::de::Error::custom("duplicate request field"));
                    }
                    let StrictValue(value) = map.next_value()?;
                    result.insert(name, value);
                }
                Ok(StrictValue(Value::Object(result)))
            }
        }
        deserializer.deserialize_any(Visitor)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn strict_text_contract() {
        for body in [
            r#"{"model":"codex/default","messages":[{"role":"user","content":"hi"}],"temperature":0}"#,
            r#"{"model":"codex/default","messages":[{"role":"user","content":null}]}"#,
            r#"{"model":"codex/default","messages":[{"role":"user","content":[{"type":"image_url","text":"x"}]}]}"#,
            r#"{"model":"codex/default","messages":[{"role":"user","content":"hi","extra":1}]}"#,
        ] {
            assert!(parse(body.as_bytes(), true).is_err());
        }
        let r = parse(br#"{"model":"codex/default","stream":true,"stream_options":{"include_usage":true},"messages":[{"role":"user","content":[{"type":"text","text":"a"},{"type":"text","text":"b"}]}]}"#, true).unwrap();
        assert!(r.include_usage && r.stream && r.prompt.contains("ab"));
        assert!(
            parse(
                br#"{"model":"codex/default","prompt":"hi","messages":[]}"#,
                false
            )
            .is_err()
        );
    }
}
