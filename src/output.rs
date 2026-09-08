use serde::Serialize;
use serde_json::{json, Value};

#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum, Default)]
pub enum Format {
    #[default]
    Json,
    Table,
}

#[derive(Debug, Clone, Serialize)]
pub struct Page {
    pub no: u32,
    pub size: u32,
    pub total: u64,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct Context {
    pub profile: String,
    pub org: Option<String>,
    pub app: Option<String>,
}

#[derive(Debug)]
pub enum Output {
    Item(Value),
    List {
        data: Vec<Value>,
        page: Option<Page>,
    },
    Message(String),
}

pub struct Rendered {
    pub output: Output,
    pub context: Option<Context>,
}

impl Output {
    pub fn item(v: Value) -> Rendered {
        Rendered {
            output: Output::Item(v),
            context: None,
        }
    }
    pub fn list(data: Vec<Value>, page: Option<Page>) -> Rendered {
        Rendered {
            output: Output::List { data, page },
            context: None,
        }
    }
    pub fn message(m: impl Into<String>) -> Rendered {
        Rendered {
            output: Output::Message(m.into()),
            context: None,
        }
    }
}

impl Rendered {
    pub fn with_context(mut self, c: Context) -> Self {
        self.context = Some(c);
        self
    }

    pub fn to_json(&self) -> Value {
        let mut v = match &self.output {
            Output::Item(x) => json!({ "data": x }),
            Output::List { data, page } => {
                let mut o = json!({ "data": data });
                if let Some(p) = page {
                    o["page"] = serde_json::to_value(p).unwrap_or(Value::Null);
                }
                o
            }
            Output::Message(m) => json!({ "data": { "message": m } }),
        };
        if let Some(c) = &self.context {
            v["context"] = serde_json::to_value(c).unwrap_or(Value::Null);
        }
        v
    }

    pub fn print(&self, format: Format) {
        match format {
            Format::Json => println!(
                "{}",
                serde_json::to_string_pretty(&self.to_json()).unwrap_or_default()
            ),
            Format::Table => println!("{}", self.to_table()),
        }
    }

    /// Minimal table: one row per object, columns = union of top-level scalar keys.
    fn to_table(&self) -> String {
        let rows: Vec<&Value> = match &self.output {
            Output::Item(v) => vec![v],
            Output::List { data, .. } => data.iter().collect(),
            Output::Message(m) => return m.clone(),
        };
        let mut cols: Vec<String> = Vec::new();
        for r in &rows {
            if let Some(o) = r.as_object() {
                for (k, v) in o {
                    if !v.is_object() && !v.is_array() && !cols.contains(k) {
                        cols.push(k.clone());
                    }
                }
            }
        }
        let cell = |v: &Value| match v {
            Value::String(s) => s.clone(),
            Value::Null => String::new(),
            other => other.to_string(),
        };
        let mut widths: Vec<usize> = cols.iter().map(|c| c.len()).collect();
        let body: Vec<Vec<String>> = rows
            .iter()
            .map(|r| cols.iter().map(|c| cell(&r[c])).collect())
            .collect();
        for row in &body {
            for (i, s) in row.iter().enumerate() {
                widths[i] = widths[i].max(s.chars().count());
            }
        }
        let line = |cells: &[String]| {
            cells
                .iter()
                .enumerate()
                .map(|(i, s)| format!("{:<w$}", s, w = widths[i]))
                .collect::<Vec<_>>()
                .join("  ")
        };
        let mut out = vec![line(&cols)];
        out.extend(body.iter().map(|r| line(r)));
        out.join("\n")
    }
}
