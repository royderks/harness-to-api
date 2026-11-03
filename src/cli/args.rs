use std::collections::HashMap;

pub struct Flags {
    values: HashMap<String, String>,
    pub rest: Vec<String>,
}
impl Flags {
    pub fn parse(args: &[String], strings: &[&str], bools: &[&str]) -> anyhow::Result<Self> {
        let mut values = HashMap::new();
        let mut i = 0;
        while i < args.len() {
            let arg = &args[i];
            if arg == "--" {
                i += 1;
                break;
            }
            if !arg.starts_with('-') || arg == "-" {
                break;
            }
            let trimmed = arg
                .strip_prefix("--")
                .or_else(|| arg.strip_prefix('-'))
                .unwrap();
            let (name, inline) = trimmed
                .split_once('=')
                .map_or((trimmed, None), |(n, v)| (n, Some(v)));
            if bools.contains(&name) {
                let value = inline.unwrap_or("true");
                let value = match value {
                    "1" | "t" | "T" | "true" | "TRUE" | "True" => "true",
                    "0" | "f" | "F" | "false" | "FALSE" | "False" => "false",
                    _ => anyhow::bail!("invalid boolean flag --{name}"),
                };
                values.insert(name.to_string(), value.to_string());
            } else if strings.contains(&name) {
                let value = match inline {
                    Some(s) => s.to_string(),
                    None => {
                        i += 1;
                        args.get(i)
                            .ok_or_else(|| anyhow::anyhow!("flag --{name} requires a value"))?
                            .clone()
                    }
                };
                values.insert(name.to_string(), value);
            } else {
                anyhow::bail!("unknown flag --{name}");
            }
            i += 1;
        }
        Ok(Self {
            values,
            rest: args[i..].to_vec(),
        })
    }
    pub fn get<'a>(&'a self, key: &str, default: &'a str) -> &'a str {
        self.values.get(key).map(String::as_str).unwrap_or(default)
    }
    pub fn seen(&self, key: &str) -> bool {
        self.values.contains_key(key)
    }
    pub fn boolean(&self, key: &str) -> bool {
        self.get(key, "false") == "true"
    }
    pub fn only_flags(&self) -> anyhow::Result<()> {
        anyhow::ensure!(self.rest.is_empty(), "unexpected positional argument");
        Ok(())
    }
    pub fn number(&self, key: &str, default: &str, min: u64, max: u64) -> anyhow::Result<u64> {
        let value = self
            .get(key, default)
            .parse::<u64>()
            .map_err(|_| anyhow::anyhow!("--{key} must be an integer"))?;
        anyhow::ensure!((min..=max).contains(&value), "--{key} must be {min}–{max}");
        Ok(value)
    }
}
pub fn help(args: &[String]) -> bool {
    args.iter()
        .take_while(|s| s.as_str() != "--")
        .any(|s| ["-h", "--help", "-help"].contains(&s.as_str()))
}
