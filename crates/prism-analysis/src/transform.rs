//! Reversible stack transforms (focus subtree, merge, drop, collapse
//! recursion / packages), parsed from the `tx` wire format.

use prism_model::{FuncId, Profile};

#[derive(Debug, Clone, PartialEq)]
pub enum Transform {
    FocusSubtree(String),
    Merge(String),
    Drop(String),
    CollapseRecursion,
    CollapsePackage,
}

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum TransformError {
    #[error("unknown transform kind in '{0}'")]
    UnknownKind(String),
    #[error("transform '{0}' needs a non-empty payload")]
    MissingPayload(String),
    #[error("transform '{0}' takes no payload")]
    UnexpectedPayload(String),
    #[error("bad percent escape in '{0}'")]
    BadEscape(String),
    #[error("too many transforms (max 32)")]
    TooMany,
}

const MAX_TRANSFORMS: usize = 32;

fn percent_decode(s: &str) -> Option<String> {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' {
            let hex = s.get(i + 1..i + 3)?;
            out.push(u8::from_str_radix(hex, 16).ok()?);
            i += 3;
        } else {
            out.push(b[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

pub fn parse_transforms(s: &str) -> Result<Vec<Transform>, TransformError> {
    if s.is_empty() {
        return Ok(vec![]);
    }
    let items: Vec<&str> = s.split(',').collect();
    if items.len() > MAX_TRANSFORMS {
        return Err(TransformError::TooMany);
    }
    items
        .into_iter()
        .map(|item| {
            let (kind, payload) = match item.split_once(':') {
                Some((k, p)) => (k, Some(p)),
                None => (item, None),
            };
            match kind {
                "rec" | "pkg" => {
                    if payload.is_some() {
                        return Err(TransformError::UnexpectedPayload(item.into()));
                    }
                    Ok(if kind == "rec" {
                        Transform::CollapseRecursion
                    } else {
                        Transform::CollapsePackage
                    })
                }
                "sub" | "merge" | "drop" => {
                    let raw = payload
                        .filter(|p| !p.is_empty())
                        .ok_or_else(|| TransformError::MissingPayload(item.into()))?;
                    let name = percent_decode(raw)
                        .ok_or_else(|| TransformError::BadEscape(item.into()))?;
                    if name.is_empty() {
                        return Err(TransformError::MissingPayload(item.into()));
                    }
                    Ok(match kind {
                        "sub" => Transform::FocusSubtree(name),
                        "merge" => Transform::Merge(name),
                        _ => Transform::Drop(name),
                    })
                }
                _ => Err(TransformError::UnknownKind(item.into())),
            }
        })
        .collect()
}

/// Package / namespace of a qualified function name (heuristic for Go paths
/// and .NET namespaces).
pub fn package_of(name: &str) -> &str {
    if let Some(slash) = name.rfind('/') {
        let rest = &name[slash + 1..];
        return match rest.find('.') {
            Some(dot) => &name[..slash + 1 + dot],
            None => name,
        };
    }
    let segs: Vec<&str> = name.split('.').collect();
    match segs.len() {
        1 => name,
        2 => segs[0],
        _ if segs[1].starts_with('(') => segs[0],
        n => {
            let len: usize = segs[..n - 2].iter().map(|s| s.len()).sum::<usize>() + (n - 3);
            &name[..len]
        }
    }
}

/// Applies one transform to a root-first function stack. Returns false when
/// the sample must be dropped.
pub(crate) fn apply(t: &Transform, p: &Profile, funcs: &mut Vec<FuncId>) -> bool {
    let name = |f: FuncId| p.str(p.functions[f.0 as usize].name);
    match t {
        Transform::FocusSubtree(n) => match funcs.iter().position(|&f| name(f) == n) {
            Some(i) => {
                funcs.drain(..i);
                true
            }
            None => false,
        },
        Transform::Merge(n) => {
            funcs.retain(|&f| name(f) != n);
            true
        }
        Transform::Drop(n) => !funcs.iter().any(|&f| name(f) == n),
        Transform::CollapseRecursion => {
            funcs.dedup();
            true
        }
        Transform::CollapsePackage => {
            let mut last: Option<&str> = None;
            let mut keep = Vec::with_capacity(funcs.len());
            for &f in funcs.iter() {
                let pkg = package_of(name(f));
                if last != Some(pkg) {
                    keep.push(f);
                }
                last = Some(pkg);
            }
            *funcs = keep;
            true
        }
    }
}
