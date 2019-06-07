use super::spawning::map_exe_error;
use super::*;
use failure::*;
use lazy_static::lazy_static;
use rusqlite::types::{ToSql, ValueRef};
use rusqlite::*;
use serde::{Deserialize, Serialize};
use std::convert::TryInto;
use std::io::BufReader;
use std::process::*;

static EXTENSIONS: &[&str] = &["db", "db3", "sqlite", "sqlite3"];

lazy_static! {
    static ref METADATA: AdapterMeta = AdapterMeta {
        name: "sqlite".to_owned(),
        version: 1,
        matchers: EXTENSIONS
            .iter()
            .map(|s| Matcher::FileExtension(s.to_string()))
            .collect(),
    };
}

#[derive(Default)]
pub struct SqliteAdapter;

impl SqliteAdapter {
    pub fn new() -> SqliteAdapter {
        SqliteAdapter
    }
}
impl GetMetadata for SqliteAdapter {
    fn metadata(&self) -> &AdapterMeta {
        &METADATA
    }
}

fn format_blob(b: ValueRef) -> String {
    use ValueRef::*;
    match b {
        Null => "NULL".to_owned(),
        Integer(i) => format!("{}", i),
        Real(i) => format!("{}", i),
        Text(i) => format!("'{}'", i.replace("'", "''")),
        Blob(b) => format!(
            "[blob {}B]",
            size_format::SizeFormatterSI::new(
                // can't be larger than 2GB anyways
                b.len().try_into().unwrap()
            )
        ),
    }
}

impl FileAdapter for SqliteAdapter {
    fn adapt(&self, ai: AdaptInfo) -> Fallible<()> {
        let AdaptInfo {
            is_real_file,
            filepath_hint,
            oup,
            line_prefix,
            ..
        } = ai;
        if !is_real_file {
            // todo: read to memory and then use that blob if size < max
            writeln!(oup, "{}[rga: skipping sqlite in archive]", line_prefix,)?;
            return Ok(());
        }
        let inp_fname = filepath_hint;

        let conn = Connection::open_with_flags(inp_fname, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        let tables: Vec<String> = conn
            .prepare("select name from sqlite_master where type='table'")?
            .query_map(NO_PARAMS, |r| r.get::<_, String>(0))?
            .filter_map(|e| e.ok())
            .collect();
        eprintln!("db has {} tables", tables.len());
        for table in tables {
            // can't use query param at that position
            let mut sel = conn.prepare(&format!(
                "select * from {}",
                rusqlite::vtab::escape_double_quote(&table)
            ))?;
            let mut z = sel.query(NO_PARAMS)?;
            let col_names: Vec<String> = z
                .column_names()
                .ok_or_else(|| format_err!("no column names"))?
                .into_iter()
                .map(|e| e.to_owned())
                .collect();
            // writeln!(oup, "{}: {}", table, cols.join(", "))?;

            // kind of shitty (lossy) output. maybe output real csv or something?
            while let Some(row) = z.next()? {
                writeln!(
                    oup,
                    "{}: {}",
                    table,
                    col_names
                        .iter()
                        .enumerate()
                        .map(|(i, e)| format!("{}={}", e, format_blob(row.get_raw(i))))
                        .collect::<Vec<String>>()
                        .join(", ")
                )?;
            }
        }
        Ok(())
    }
}