use serde_json::{Value, json};
use std::collections::{BTreeMap, HashSet};

#[derive(Clone)]
pub(super) struct Bridge {
    tools: Vec<Tool>,
    choice: Choice,
    parallel: bool,
}
#[derive(Clone)]
struct Tool {
    name: String,
    schema: Schema,
    definition: Value,
}
#[derive(Clone)]
enum Choice {
    None,
    Auto,
    Required,
    Named(String),
}

#[derive(Clone)]
pub(super) enum Outcome {
    Text(String),
    Calls(Vec<Call>),
}
#[derive(Clone)]
pub(super) struct Call {
    id: String,
    name: String,
    arguments: String,
}
impl Call {
    pub fn wire(&self) -> Value {
        json!({"id":self.id,"type":"function","function":{"name":self.name,"arguments":self.arguments}})
    }
}
impl Outcome {
    pub fn message(&self) -> Value {
        match self {
            Self::Text(text) => json!({"role":"assistant","content":text}),
            Self::Calls(calls) => {
                json!({"role":"assistant","content":null,"tool_calls":calls.iter().map(Call::wire).collect::<Vec<_>>()})
            }
        }
    }
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
pub(super) fn name(v: Option<&Value>) -> Result<String, &'static str> {
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
#[derive(Clone)]
struct Schema {
    kind: Kind,
    enumeration: Option<Vec<Value>>,
    any_of: Vec<Schema>,
}
#[derive(Clone)]
enum Kind {
    Any,
    Object {
        properties: BTreeMap<String, Schema>,
        required: HashSet<String>,
        additional: bool,
    },
    Array(Box<Schema>),
    String,
    Number,
    Integer,
    Boolean,
    Null,
}
// Compare decimal values without rounding large integers through f64.
fn decimal(number: &serde_json::Number) -> (bool, String, i32) {
    let text = number.to_string();
    let negative = text.starts_with('-');
    let unsigned = text.trim_start_matches('-');
    let (mantissa, exponent) = unsigned.split_once('e').unwrap_or((unsigned, "0"));
    let fractional = mantissa.split_once('.').map_or(0, |(_, s)| s.len());
    let mut exponent = exponent.parse::<i32>().unwrap() - fractional as i32;
    let mut digits = mantissa.replace('.', "").trim_start_matches('0').to_owned();
    if digits.is_empty() {
        return (false, String::new(), 0);
    }
    while digits.ends_with('0') {
        digits.pop();
        exponent += 1;
    }
    (negative, digits, exponent)
}

fn enum_equal(left: &Value, right: &Value, budget: &mut usize) -> bool {
    if *budget == 0 {
        return false;
    }
    *budget -= 1;
    match (left, right) {
        (Value::Number(a), Value::Number(b)) => decimal(a) == decimal(b),
        (Value::Array(a), Value::Array(b)) => {
            a.len() == b.len() && a.iter().zip(b).all(|(a, b)| enum_equal(a, b, budget))
        }
        (Value::Object(a), Value::Object(b)) => {
            a.len() == b.len()
                && a.iter()
                    .all(|(key, a)| b.get(key).is_some_and(|b| enum_equal(a, b, budget)))
        }
        _ => left == right,
    }
}

impl Schema {
    fn parse(v: &Value, depth: usize, strict: bool) -> Result<Self, &'static str> {
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
        let enumeration = o
            .get("enum")
            .map(|v| {
                let a = v.as_array().ok_or("invalid enum")?;
                if a.is_empty() || a.len() > 128 {
                    return Err("invalid enum");
                }
                Ok(a.clone())
            })
            .transpose()?;
        let any_of = if let Some(v) = o.get("anyOf") {
            let a = v.as_array().ok_or("invalid anyOf")?;
            if a.is_empty() || a.len() > 16 {
                return Err("invalid anyOf");
            }
            a.iter()
                .map(|v| Self::parse(v, depth + 1, strict))
                .collect::<Result<Vec<_>, _>>()?
        } else {
            vec![]
        };
        let typ = o
            .get("type")
            .map(|v| v.as_str().ok_or("invalid schema type"))
            .transpose()?;
        if typ.is_none() && enumeration.is_none() && any_of.is_empty() {
            return Err("schema requires type, enum, or anyOf");
        }
        if typ != Some("object")
            && (o.contains_key("properties")
                || o.contains_key("required")
                || o.contains_key("additionalProperties"))
        {
            return Err("object keywords require object");
        }
        if typ != Some("array") && o.contains_key("items") {
            return Err("items requires array");
        }
        let kind = match typ {
            None => Kind::Any,
            Some("object") => {
                let mut properties = BTreeMap::new();
                if let Some(p) = o.get("properties") {
                    let p = p.as_object().ok_or("invalid properties")?;
                    if p.len() > 128 {
                        return Err("too many properties");
                    }
                    for (name, v) in p {
                        properties.insert(name.clone(), Self::parse(v, depth + 1, strict)?);
                    }
                }
                let mut required = HashSet::new();
                if let Some(r) = o.get("required") {
                    for v in r.as_array().ok_or("invalid required")? {
                        let name = v.as_str().ok_or("invalid required")?;
                        if !properties.contains_key(name) || !required.insert(name.to_owned()) {
                            return Err("invalid required");
                        }
                    }
                }
                let additional = match o.get("additionalProperties") {
                    None => true,
                    Some(Value::Bool(b)) => *b,
                    _ => return Err("invalid additionalProperties"),
                };
                if strict && (additional || properties.keys().any(|k| !required.contains(k))) {
                    return Err(
                        "strict objects require all properties and additionalProperties=false",
                    );
                }
                Kind::Object {
                    properties,
                    required,
                    additional,
                }
            }
            Some("array") => Kind::Array(Box::new(Self::parse(
                o.get("items").ok_or("array requires items")?,
                depth + 1,
                strict,
            )?)),
            Some("string") => Kind::String,
            Some("number") => Kind::Number,
            Some("integer") => Kind::Integer,
            Some("boolean") => Kind::Boolean,
            Some("null") => Kind::Null,
            _ => return Err("invalid schema type"),
        };
        Ok(Self {
            kind,
            enumeration,
            any_of,
        })
    }
    fn matches(&self, v: &Value, budget: &mut usize) -> bool {
        if *budget == 0 {
            return false;
        }
        *budget -= 1;
        if self
            .enumeration
            .as_ref()
            .is_some_and(|e| !e.iter().any(|candidate| enum_equal(candidate, v, budget)))
            || (!self.any_of.is_empty() && !self.any_of.iter().any(|s| s.matches(v, budget)))
        {
            return false;
        }
        match &self.kind {
            Kind::Any => true,
            Kind::Object {
                properties,
                required,
                additional,
            } => v.as_object().is_some_and(|o| {
                required.iter().all(|k| o.contains_key(k))
                    && o.iter().all(|(k, v)| match properties.get(k) {
                        Some(s) => s.matches(v, budget),
                        None => *additional,
                    })
            }),
            Kind::Array(items) => v
                .as_array()
                .is_some_and(|a| a.iter().all(|v| items.matches(v, budget))),
            Kind::String => v.is_string(),
            Kind::Number => v.is_number(),
            Kind::Integer => {
                v.as_i64().is_some()
                    || v.as_u64().is_some()
                    || v.as_f64().is_some_and(|x| x.fract() == 0.0)
            }
            Kind::Boolean => v.is_boolean(),
            Kind::Null => v.is_null(),
        }
    }
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
                let schema = Schema::parse(s, 0, strict)?;
                parsed.push(Tool {
                    name: n,
                    schema,
                    definition: t.clone(),
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
    pub fn has_tools(&self) -> bool {
        !self.tools.is_empty()
    }
    pub fn prompt_data(&self, conversation: Vec<Value>) -> Value {
        let choice = match &self.choice {
            Choice::None => json!("none"),
            Choice::Auto => json!("auto"),
            Choice::Required => json!("required"),
            Choice::Named(name) => json!({"type":"function","function":{"name":name}}),
        };
        json!({"conversation":conversation,"tools":self.tools.iter().map(|t| &t.definition).collect::<Vec<_>>(),"tool_choice":choice,"parallel_tool_calls":self.parallel})
    }
    pub fn validate(&self, bytes: &[u8]) -> Result<Outcome, &'static str> {
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
                Ok(Outcome::Text(o["content"].as_str().unwrap().to_owned()))
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
                let mut ids = HashSet::new();
                let mut budget = 100_000;
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
                    if !a.is_object() || !t.schema.matches(a, &mut budget) {
                        return Err("invalid output");
                    }
                    let id = loop {
                        let id = format!("call_{}", super::random_id());
                        if ids.insert(id.clone()) {
                            break id;
                        }
                    };
                    result.push(Call {
                        id,
                        name: n,
                        arguments: serde_json::to_string(a).unwrap(),
                    });
                }
                Ok(Outcome::Calls(result))
            }
            _ => Err("invalid output"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enum_numbers_compare_by_value_recursively_without_integer_rounding() {
        for (schema, value) in [
            (json!({"type":"integer","enum":[1]}), json!(1.0)),
            (json!({"enum":[1.0]}), json!(1)),
            (json!({"enum":[0]}), json!(-0.0)),
            (json!({"enum":[1000]}), json!(1e3)),
            (
                json!({"enum":[{"values":[1,-2,0]}]}),
                json!({"values":[1.0,-2.0,-0.0]}),
            ),
        ] {
            assert!(
                Schema::parse(&schema, 0, false)
                    .unwrap()
                    .matches(&value, &mut 100_000)
            );
        }
        for (expected, actual) in [
            (json!(1), json!(1.5)),
            (json!(9007199254740993_u64), json!(9007199254740992.0)),
            (
                json!(18446744073709551615_u64),
                json!(18446744073709551614_u64),
            ),
            (json!({"values":[1,2]}), json!({"values":[1.0,3.0]})),
        ] {
            let schema = Schema::parse(&json!({"enum":[expected]}), 0, false).unwrap();
            assert!(!schema.matches(&actual, &mut 100_000));
        }
        let schema = Schema::parse(&json!({"enum":[[1,2,3]]}), 0, false).unwrap();
        assert!(!schema.matches(&json!([1.0, 2.0, 3.0]), &mut 3));
    }

    #[test]
    fn compiled_schema_validates_nested_values_and_rejects_unsupported_keywords() {
        let schema = json!({"type":"object","properties":{
            "items":{"type":"array","items":{"anyOf":[{"type":"integer","enum":[1,2]},{"type":"null"}]}},
            "flag":{"type":"boolean"},"number":{"type":"number"},"text":{"type":"string"}
        },"required":["items","flag","number","text"],"additionalProperties":false});
        let compiled = Schema::parse(&schema, 0, true).unwrap();
        assert!(compiled.matches(
            &json!({"items":[1,null,2],"flag":true,"number":1.5,"text":"ok"}),
            &mut 100_000
        ));
        for value in [
            json!({"items":[3],"flag":true,"number":1,"text":"ok"}),
            json!({"items":[],"flag":true,"number":1,"text":"ok","extra":1}),
            json!({"items":[],"flag":true,"number":1}),
            json!({"items":[1.5],"flag":true,"number":1,"text":"ok"}),
        ] {
            assert!(!compiled.matches(&value, &mut 100_000));
        }
        for keyword in [
            "$ref", "$defs", "format", "pattern", "oneOf", "allOf", "minimum", "default",
        ] {
            let mut invalid = schema.clone();
            invalid[keyword] = json!("x");
            assert!(Schema::parse(&invalid, 0, true).is_err(), "{keyword}");
        }
        let mut invalid = schema.clone();
        invalid["properties"]["items"]["items"]["anyOf"][0]["type"] = json!("bogus");
        assert!(Schema::parse(&invalid, 0, true).is_err());
        assert!(!compiled.matches(
            &json!({"items":[1],"flag":true,"number":1,"text":"ok"}),
            &mut 0
        ));
    }
}
