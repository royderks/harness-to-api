use serde_json::{Value, json};
use std::collections::HashSet;

#[derive(Clone)]
pub(super) struct Bridge {
    tools: Vec<Tool>,
    choice: Choice,
    parallel: bool,
}
#[derive(Clone)]
struct Tool {
    name: String,
    schema: Value,
}
#[derive(Clone)]
enum Choice {
    None,
    Auto,
    Required,
    Named(String),
}

fn object<'a>(
    v: &'a Value,
    keys: &[&str],
) -> Result<&'a serde_json::Map<String, Value>, &'static str> {
    let o = v.as_object().ok_or("expected object")?;
    if o.keys().any(|k| !keys.contains(&k.as_str())) {
        return Err("unsupported object field");
    }
    Ok(o)
}
fn name(v: Option<&Value>) -> Result<String, &'static str> {
    let s = v.and_then(Value::as_str).ok_or("invalid function name")?;
    if s.is_empty()
        || s.len() > 64
        || !s
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-')
    {
        return Err("invalid function name");
    }
    Ok(s.to_owned())
}
fn schema(v: &Value, depth: usize, strict: bool) -> Result<(), &'static str> {
    if depth > 16 {
        return Err("schema too deep");
    }
    let o = object(
        v,
        &[
            "type",
            "properties",
            "required",
            "additionalProperties",
            "items",
            "enum",
            "anyOf",
            "description",
            "title",
        ],
    )?;
    for k in ["description", "title"] {
        if o.get(k).is_some_and(|v| !v.is_string()) {
            return Err("invalid schema annotation");
        }
    }
    if let Some(e) = o.get("enum") {
        let a = e.as_array().ok_or("invalid enum")?;
        if a.is_empty() || a.len() > 128 {
            return Err("invalid enum");
        }
    }
    if let Some(a) = o.get("anyOf") {
        let a = a.as_array().ok_or("invalid anyOf")?;
        if a.is_empty() || a.len() > 16 {
            return Err("invalid anyOf");
        }
        for v in a {
            schema(v, depth + 1, strict)?;
        }
    }
    let typ = o
        .get("type")
        .map(|v| v.as_str().ok_or("invalid schema type"))
        .transpose()?;
    if typ.is_none() && !o.contains_key("anyOf") && !o.contains_key("enum") {
        return Err("schema requires type, enum, or anyOf");
    }
    if let Some(t) = typ {
        if ![
            "object", "array", "string", "number", "integer", "boolean", "null",
        ]
        .contains(&t)
        {
            return Err("invalid schema type");
        }
    }
    let props = if let Some(p) = o.get("properties") {
        if typ != Some("object") {
            return Err("properties require object");
        }
        let p = p.as_object().ok_or("invalid properties")?;
        if p.len() > 128 {
            return Err("too many properties");
        }
        for v in p.values() {
            schema(v, depth + 1, strict)?;
        }
        Some(p)
    } else {
        None
    };
    let required = if let Some(r) = o.get("required") {
        if typ != Some("object") {
            return Err("required requires object");
        }
        let a = r.as_array().ok_or("invalid required")?;
        let mut seen = HashSet::new();
        for x in a {
            let s = x.as_str().ok_or("invalid required")?;
            if !seen.insert(s) || !props.is_some_and(|p| p.contains_key(s)) {
                return Err("invalid required");
            }
        }
        Some(seen)
    } else {
        None
    };
    if let Some(a) = o.get("additionalProperties") {
        if typ != Some("object") || !a.is_boolean() {
            return Err("invalid additionalProperties");
        }
    }
    if strict
        && typ == Some("object")
        && (o.get("additionalProperties") != Some(&Value::Bool(false))
            || props.is_some_and(|p| {
                p.keys()
                    .any(|k| !required.as_ref().is_some_and(|r| r.contains(k.as_str())))
            }))
    {
        return Err("strict objects require all properties and additionalProperties=false");
    }
    if let Some(i) = o.get("items") {
        if typ != Some("array") {
            return Err("items requires array");
        }
        schema(i, depth + 1, strict)?;
    } else if typ == Some("array") {
        return Err("array requires items");
    }
    Ok(())
}
fn matches(v: &Value, s: &Value, budget: &mut usize) -> bool {
    if *budget == 0 {
        return false;
    }
    *budget -= 1;
    let o = s.as_object().unwrap();
    if let Some(e) = o.get("enum")
        && !e.as_array().unwrap().contains(v)
    {
        return false;
    }
    if let Some(a) = o.get("anyOf")
        && !a.as_array().unwrap().iter().any(|s| matches(v, s, budget))
    {
        return false;
    }
    if let Some(t) = o.get("type").and_then(Value::as_str) {
        let good = match t {
            "object" => v.is_object(),
            "array" => v.is_array(),
            "string" => v.is_string(),
            "number" => v.is_number(),
            "integer" => {
                v.as_i64().is_some()
                    || v.as_u64().is_some()
                    || v.as_f64().is_some_and(|x| x.fract() == 0.0)
            }
            "boolean" => v.is_boolean(),
            "null" => v.is_null(),
            _ => false,
        };
        if !good {
            return false;
        }
    }
    if let Some(obj) = v.as_object() {
        if let Some(req) = o.get("required")
            && req
                .as_array()
                .unwrap()
                .iter()
                .any(|k| !obj.contains_key(k.as_str().unwrap()))
        {
            return false;
        }
        let props = o.get("properties").and_then(Value::as_object);
        for (k, v) in obj {
            if let Some(s) = props.and_then(|p| p.get(k)) {
                if !matches(v, s, budget) {
                    return false;
                }
            } else if o.get("additionalProperties") == Some(&Value::Bool(false)) {
                return false;
            }
        }
    }
    if let Some(a) = v.as_array()
        && let Some(i) = o.get("items")
        && a.iter().any(|v| !matches(v, i, budget))
    {
        return false;
    }
    true
}
impl Bridge {
    pub fn parse(
        tools: Option<Value>,
        choice: Option<Value>,
        parallel: Option<Value>,
    ) -> Result<Self, &'static str> {
        let mut parsed = Vec::new();
        let mut names = HashSet::new();
        if let Some(v) = tools.filter(|v| !v.is_null()) {
            let arr = v.as_array().ok_or("tools must be an array")?;
            if arr.len() > 64 {
                return Err("too many tools");
            }
            for t in arr {
                let o = object(t, &["type", "function"])?;
                if o.get("type").and_then(Value::as_str) != Some("function") {
                    return Err("unsupported tool type");
                }
                let f = object(
                    o.get("function").ok_or("missing function")?,
                    &["name", "description", "parameters", "strict"],
                )?;
                let n = name(f.get("name"))?;
                if !names.insert(n.clone()) {
                    return Err("duplicate function");
                }
                if f.get("description")
                    .is_some_and(|v| !v.is_string() && !v.is_null())
                {
                    return Err("invalid description");
                }
                let strict = match f.get("strict") {
                    None | Some(Value::Null) | Some(Value::Bool(false)) => false,
                    Some(Value::Bool(true)) => true,
                    _ => return Err("invalid strict"),
                };
                let s = f.get("parameters").ok_or("missing parameters")?;
                if s.get("type").and_then(Value::as_str) != Some("object") {
                    return Err("parameters must be an object schema");
                }
                schema(s, 0, strict)?;
                parsed.push(Tool {
                    name: n,
                    schema: s.clone(),
                });
            }
        }
        let choice = match choice {
            None | Some(Value::Null) => {
                if parsed.is_empty() {
                    Choice::None
                } else {
                    Choice::Auto
                }
            }
            Some(Value::String(s)) => match s.as_str() {
                "none" => Choice::None,
                "auto" => Choice::Auto,
                "required" => Choice::Required,
                _ => return Err("invalid tool choice"),
            },
            Some(v) => {
                let o = object(&v, &["type", "function"])?;
                if o.get("type").and_then(Value::as_str) != Some("function") {
                    return Err("invalid tool choice");
                }
                let f = object(o.get("function").ok_or("invalid tool choice")?, &["name"])?;
                Choice::Named(name(f.get("name"))?)
            }
        };
        if matches!(choice, Choice::Required) && parsed.is_empty()
            || matches!(&choice, Choice::Named(n) if !names.contains(n))
        {
            return Err("tool choice requires a supplied function");
        }
        let parallel = match parallel {
            None | Some(Value::Null) => true,
            Some(Value::Bool(b)) => b,
            _ => return Err("invalid parallel_tool_calls"),
        };
        Ok(Self {
            tools: parsed,
            choice,
            parallel,
        })
    }
    pub fn validate(&self, bytes: &[u8]) -> Result<Value, &'static str> {
        let v = super::request::strict_json(bytes)?;
        let o = v.as_object().ok_or("invalid output")?;
        if o.get("version") != Some(&json!(1)) {
            return Err("invalid output");
        }
        match o.get("kind").and_then(Value::as_str) {
            Some("text")
                if o.len() == 3
                    && o.get("content").is_some_and(Value::is_string)
                    && matches!(self.choice, Choice::None | Choice::Auto) =>
            {
                Ok(json!({"role":"assistant","content":o["content"]}))
            }
            Some("tool_calls") if o.len() == 3 && !matches!(self.choice, Choice::None) => {
                let calls = o
                    .get("calls")
                    .and_then(Value::as_array)
                    .ok_or("invalid output")?;
                if calls.is_empty() || calls.len() > 64 || (!self.parallel && calls.len() != 1) {
                    return Err("invalid output");
                }
                let mut result = Vec::new();
                for c in calls {
                    let c = object(c, &["name", "arguments"])?;
                    if c.len() != 2 {
                        return Err("invalid output");
                    }
                    let n = name(c.get("name"))?;
                    if matches!(&self.choice,Choice::Named(expected) if expected != &n) {
                        return Err("invalid output");
                    }
                    let t = self
                        .tools
                        .iter()
                        .find(|t| t.name == n)
                        .ok_or("invalid output")?;
                    let a = c.get("arguments").ok_or("invalid output")?;
                    if !a.is_object() || !matches(a, &t.schema, &mut 100_000) {
                        return Err("invalid output");
                    }
                    result.push(json!({"id":format!("call_{}",super::random_id()),"type":"function","function":{"name":n,"arguments":serde_json::to_string(a).unwrap()}}));
                }
                Ok(json!({"role":"assistant","content":null,"tool_calls":result}))
            }
            _ => Err("invalid output"),
        }
    }
}
