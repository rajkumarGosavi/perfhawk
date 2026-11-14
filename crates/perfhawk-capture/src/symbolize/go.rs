//! Function symbols from an executable's symbol table (Go binaries carry one
//! unless built with `-ldflags=-s -w`).

use super::SymbolizeError;
use object::{Object, ObjectSymbol, SymbolKind};
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GoSym {
    /// Relative to the image base.
    pub start: u64,
    /// 0 when the format does not record sizes.
    pub size: u64,
    pub name: String,
}

pub fn go_symbols(path: &Path) -> Result<Vec<GoSym>, SymbolizeError> {
    let data = std::fs::read(path).map_err(|e| SymbolizeError::Other(e.to_string()))?;
    let file = object::File::parse(&*data).map_err(|e| SymbolizeError::Other(e.to_string()))?;
    let base = file.relative_address_base();
    let mut out: Vec<GoSym> = file
        .symbols()
        .filter(|s| s.kind() == SymbolKind::Text)
        .filter_map(|s| {
            let name = s.name().ok()?;
            if name.is_empty() {
                return None;
            }
            Some(GoSym {
                start: s.address().checked_sub(base)?,
                size: s.size(),
                name: name.to_string(),
            })
        })
        .collect();
    if out.is_empty() {
        return Err(SymbolizeError::Stripped);
    }
    out.sort_by_key(|s| s.start);
    Ok(out)
}
