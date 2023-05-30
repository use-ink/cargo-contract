// Credit: https://github.com/wlezzar/jtab
use std::collections::HashMap;

use anyhow::bail;
use prettytable::{Cell, format, Row, Table};
use prettytable::format::{FormatBuilder, LinePosition, LineSeparator};
use regex::Regex;
use serde_json::Value;
use yaml_rust::{Yaml, YamlEmitter};
use yaml_rust::yaml::Hash;

#[derive(Debug)]
pub enum TableHeader {
    NamedFields { fields: Vec<String> },
    SingleUnnamedColumn,
}

#[derive(Debug)]
pub struct JsonTable {
    headers: TableHeader,
    values: Vec<Vec<Value>>,
}

impl JsonTable {
    pub fn new(headers: Option<TableHeader>, root: &Value) -> JsonTable {
        let rows: Vec<Value> = match root {
            Value::Array(arr) => arr.to_owned(), // TODO: is it possible to avoid cloning here?
            _ => vec![root.to_owned()],
        };

        let headers = headers.unwrap_or_else(|| infer_headers(&rows));
        let mut values = Vec::new();

        match &headers {
            TableHeader::NamedFields { fields } => {
                for row in rows {
                    values.push(
                        fields
                            .iter()
                            .map(|h| row.get(h).unwrap_or(&Value::Null).to_owned())
                            .collect(),
                    )
                }
            }
            TableHeader::SingleUnnamedColumn => {
                for row in rows {
                    values.push(vec![row.to_owned()])
                }
            }
        }
        JsonTable { headers, values }
    }
}

fn infer_headers(arr: &Vec<Value>) -> TableHeader {
    match arr.first() {
        Some(Value::Object(obj)) => TableHeader::NamedFields {
            fields: obj.keys().map(|h| h.to_owned()).collect(),
        },
        _ => TableHeader::SingleUnnamedColumn,
    }
}

#[derive(Debug)]
pub struct ColorizeSpec {
    field: String,
    value: String,
    style: String,
}

impl ColorizeSpec {
    pub fn parse(s: &String) -> anyhow::Result<ColorizeSpec> {
        let re = Regex::new(r"^([^:]+):(.+):([a-zA-Z]+)$")?;
        match re.captures(s) {
            Some(captures) => {
                let field = captures
                    .get(1)
                    .ok_or_else(|| anyhow::Error::msg("wrong regular expression..."))?
                    .as_str()
                    .to_string();
                let value = captures
                    .get(2)
                    .ok_or_else(|| anyhow::Error::msg("wrong regular expression..."))?
                    .as_str()
                    .to_string();
                let style = captures
                    .get(3)
                    .ok_or_else(|| anyhow::Error::msg("wrong regular expression..."))?
                    .as_str()
                    .to_string();
                Ok(ColorizeSpec {
                    field,
                    value,
                    style,
                })
            }
            _ => bail!("wrong colorize expression. Should be in the form of : 'field:value:spec'"),
        }
    }
}

pub trait Printer {
    fn print(&self, data: &JsonTable) -> anyhow::Result<()>;
}

fn json_to_yaml(value: &Value) -> Yaml {
    match value {
        Value::Object(obj) => {
            let mut hash = Hash::new();
            for (key, value) in obj {
                hash.insert(Yaml::String(key.to_owned()), json_to_yaml(value));
            }
            Yaml::Hash(hash)
        }
        Value::Array(arr) => {
            let arr = arr.iter().map(json_to_yaml).collect::<Vec<_>>();
            Yaml::Array(arr)
        }
        Value::Null => Yaml::Null,
        Value::Bool(e) => Yaml::Boolean(e.to_owned()),
        Value::Number(n) => Yaml::Real(format!("{}", n)),
        Value::String(s) => Yaml::String(s.to_owned()),
    }
}

#[derive(Debug)]
pub enum PlainTextTableFormat {
    Default,
    Markdown,
}

#[derive(Debug)]
pub enum TableFormat {
    PlainText(PlainTextTableFormat),
}

fn pprint_table_cell(value: &Value) -> anyhow::Result<String> {
    match value {
        Value::String(s) => Ok(s.to_string()),
        Value::Object(_) | Value::Array(_) => {
            let mut res = String::new();
            {
                let yaml_form = json_to_yaml(value);
                let mut emitter = YamlEmitter::new(&mut res);
                emitter.dump(&yaml_form)?;
            }
            Ok(res.trim_start_matches("---\n").to_string())
        }
        _ => Ok(serde_json::to_string(value)?),
    }
}

pub struct PlainTextTablePrinter {
    colorize: Vec<ColorizeSpec>,
    format: PlainTextTableFormat,
}

impl PlainTextTablePrinter {
    pub fn new(colorize: Vec<ColorizeSpec>, format: PlainTextTableFormat) -> PlainTextTablePrinter {
        PlainTextTablePrinter { colorize, format }
    }
}

impl Printer for PlainTextTablePrinter {
    fn print(&self, data: &JsonTable) -> anyhow::Result<()> {
        let mut table = Table::new();

        // header row
        table.set_titles(Row::new(match &data.headers {
            TableHeader::NamedFields { fields } => fields
                .iter()
                .map(|f| Cell::new(f).style_spec("bFc"))
                .collect(),
            TableHeader::SingleUnnamedColumn => vec![Cell::new("value")],
        }));

        // build colorize map
        let colorize: HashMap<usize, Vec<&ColorizeSpec>> = match &data.headers {
            TableHeader::NamedFields { fields } => {
                let mut res: HashMap<usize, Vec<&ColorizeSpec>> = HashMap::new();
                for c in self.colorize.iter() {
                    if let Some(index) = fields.iter().position(|f| c.field == *f) {
                        res.entry(index).or_default().push(c)
                    }
                }
                res
            }
            _ => HashMap::new(),
        };

        // data rows

        for value in &data.values {
            let mut row = Row::empty();
            for (idx, element) in value.iter().enumerate() {
                let formatted = pprint_table_cell(element)?;
                let formatted = formatted.as_str();
                let cell = Cell::new(formatted);
                let cell = match colorize.get(&idx) {
                    Some(styles) => match styles.iter().find(|s| s.value == *formatted) {
                        Some(style) => cell.style_spec(style.style.as_str()),
                        None => cell,
                    },
                    _ => cell,
                };

                row.add_cell(cell);
            }
            table.add_row(row);
        }

        match &self.format {
            PlainTextTableFormat::Default => table.set_format(*format::consts::FORMAT_BOX_CHARS),
            PlainTextTableFormat::Markdown => table.set_format(
                FormatBuilder::new()
                    .padding(1, 1)
                    .separator(LinePosition::Title, LineSeparator::new('-', '|', '|', '|'))
                    .column_separator('|')
                    .borders('|')
                    .build(),
            ),
        }

        table.printstd();
        Ok(())
    }
}
